use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::safe_path;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadPackageManifestParams {
    #[schemars(
        description = "Path to a specific manifest file, or omit to auto-scan workspace root."
    )]
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DependencyEntry {
    pub name: String,
    pub version: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct ManifestEntry {
    pub path: String,
    pub ecosystem: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub dependencies: Vec<DependencyEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct ReadPackageManifestResult {
    pub manifests: Vec<ManifestEntry>,
    pub token_count: usize,
}

const MANIFEST_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
];

pub fn read_package_manifest(
    root: &Path,
    params: ReadPackageManifestParams,
) -> anyhow::Result<ReadPackageManifestResult> {
    let mut manifests = Vec::new();

    if let Some(ref p) = params.path {
        let full = safe_path(root, p)?;
        if let Some(entry) = parse_manifest(&full, p) {
            manifests.push(entry);
        }
    } else {
        for name in MANIFEST_FILES {
            let candidate = root.join(name);
            if !candidate.exists() {
                continue;
            }
            if let Some(entry) = parse_manifest(&candidate, name) {
                manifests.push(entry);
            }
        }
    }

    let json = serde_json::to_string(&manifests).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadPackageManifestResult {
        manifests,
        token_count,
    })
}

fn parse_manifest(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let name = path.file_name()?.to_str()?;
    match name {
        "package.json" => parse_package_json(path, rel),
        "Cargo.toml" => parse_cargo_toml(path, rel),
        "pyproject.toml" => parse_pyproject_toml(path, rel),
        "requirements.txt" => parse_requirements_txt(path, rel),
        "go.mod" => parse_go_mod(path, rel),
        "pom.xml" => parse_pom_xml(path, rel),
        "build.gradle" | "build.gradle.kts" => parse_gradle(path, rel),
        _ => None,
    }
}

fn parse_package_json(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;

    let mut deps = Vec::new();
    for (key, kind) in &[
        ("dependencies", "runtime"),
        ("devDependencies", "dev"),
        ("peerDependencies", "optional"),
        ("optionalDependencies", "optional"),
    ] {
        if let Some(obj) = v.get(key).and_then(|d| d.as_object()) {
            for (dep_name, ver) in obj {
                deps.push(DependencyEntry {
                    name: dep_name.clone(),
                    version: ver.as_str().unwrap_or("*").to_string(),
                    kind: kind.to_string(),
                });
            }
        }
    }

    let scripts = v.get("scripts").and_then(|s| s.as_object()).map(|obj| {
        obj.iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
            .collect::<HashMap<_, _>>()
    });

    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "npm".to_string(),
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
        version: v
            .get("version")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
        dependencies: deps,
        scripts,
    })
}

fn parse_cargo_toml(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let doc: toml::Value = toml::from_str(&content).ok()?;

    let pkg = doc.get("package");
    let name = pkg
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let version = pkg
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut deps = Vec::new();
    for (section, kind) in &[
        ("dependencies", "runtime"),
        ("dev-dependencies", "dev"),
        ("build-dependencies", "build"),
    ] {
        if let Some(table) = doc.get(section).and_then(|d| d.as_table()) {
            for (dep_name, dep_val) in table {
                let ver = match dep_val {
                    toml::Value::String(s) => s.clone(),
                    toml::Value::Table(t) => t
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*")
                        .to_string(),
                    _ => "*".to_string(),
                };
                deps.push(DependencyEntry {
                    name: dep_name.clone(),
                    version: ver,
                    kind: kind.to_string(),
                });
            }
        }
    }

    // Workspace members listed as scripts for visibility
    let scripts = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, v)| v.as_str().map(|s| (format!("member_{i}"), s.to_string())))
                .collect::<HashMap<_, _>>()
        })
        .filter(|m| !m.is_empty());

    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "cargo".to_string(),
        name,
        version,
        dependencies: deps,
        scripts,
    })
}

fn parse_pyproject_toml(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let doc: toml::Value = toml::from_str(&content).ok()?;

    // Poetry style
    if let Some(poetry) = doc.get("tool").and_then(|t| t.get("poetry")) {
        let name = poetry
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
        let version = poetry
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let mut deps = Vec::new();
        for (section, kind) in &[("dependencies", "runtime"), ("dev-dependencies", "dev")] {
            if let Some(table) = poetry.get(section).and_then(|d| d.as_table()) {
                for (dep_name, dep_val) in table {
                    if dep_name == "python" {
                        continue;
                    }
                    let ver = match dep_val {
                        toml::Value::String(s) => s.clone(),
                        toml::Value::Table(t) => t
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("*")
                            .to_string(),
                        _ => "*".to_string(),
                    };
                    deps.push(DependencyEntry {
                        name: dep_name.clone(),
                        version: ver,
                        kind: kind.to_string(),
                    });
                }
            }
        }
        return Some(ManifestEntry {
            path: rel.to_string(),
            ecosystem: "python".to_string(),
            name,
            version,
            dependencies: deps,
            scripts: None,
        });
    }

    // PEP 517/518 style
    let project = doc.get("project");
    let name = project
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let version = project
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut deps = Vec::new();

    if let Some(arr) = project
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in arr {
            if let Some(s) = dep.as_str() {
                let (n, v) = parse_pep_dep(s);
                deps.push(DependencyEntry {
                    name: n,
                    version: v,
                    kind: "runtime".to_string(),
                });
            }
        }
    }
    if let Some(opt) = project
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(|d| d.as_table())
    {
        for (group, arr) in opt {
            let kind = if group.contains("dev") || group.contains("test") {
                "dev"
            } else {
                "optional"
            };
            if let Some(arr) = arr.as_array() {
                for dep in arr {
                    if let Some(s) = dep.as_str() {
                        let (n, v) = parse_pep_dep(s);
                        deps.push(DependencyEntry {
                            name: n,
                            version: v,
                            kind: kind.to_string(),
                        });
                    }
                }
            }
        }
    }

    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "python".to_string(),
        name,
        version,
        dependencies: deps,
        scripts: None,
    })
}

fn parse_pep_dep(s: &str) -> (String, String) {
    let re = Regex::new(r"^([A-Za-z0-9_\-\.]+)\s*(.*)$").unwrap();
    if let Some(cap) = re.captures(s) {
        (cap[1].to_string(), cap[2].trim().to_string())
    } else {
        (s.to_string(), "*".to_string())
    }
}

fn parse_requirements_txt(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut deps = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        let (name, ver) = parse_pep_dep(line);
        deps.push(DependencyEntry {
            name,
            version: ver,
            kind: "runtime".to_string(),
        });
    }
    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "python".to_string(),
        name: None,
        version: None,
        dependencies: deps,
        scripts: None,
    })
}

fn parse_go_mod(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut mod_name = None;
    let mut go_version = None;
    let mut deps = Vec::new();
    let mut in_require = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            mod_name = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("go ") {
            go_version = Some(rest.trim().to_string());
        } else if trimmed == "require (" {
            in_require = true;
        } else if trimmed == ")" {
            in_require = false;
        } else if in_require || trimmed.starts_with("require ") {
            let dep_line = trimmed.trim_start_matches("require ").trim();
            let parts: Vec<&str> = dep_line.splitn(2, ' ').collect();
            if parts.len() == 2 {
                let kind = if dep_line.ends_with("// indirect") {
                    "optional"
                } else {
                    "runtime"
                };
                deps.push(DependencyEntry {
                    name: parts[0].to_string(),
                    version: parts[1].trim_end_matches("// indirect").trim().to_string(),
                    kind: kind.to_string(),
                });
            }
        }
    }

    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "go".to_string(),
        name: mod_name,
        version: go_version,
        dependencies: deps,
        scripts: None,
    })
}

fn parse_pom_xml(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let re_dep = Regex::new(
        r"(?s)<dependency>\s*<groupId>([^<]+)</groupId>\s*<artifactId>([^<]+)</artifactId>(?:\s*<version>([^<]+)</version>)?(?:[^<]*<scope>([^<]+)</scope>)?[^<]*</dependency>"
    ).unwrap();
    let name_re = Regex::new(r"<artifactId>([^<]+)</artifactId>").unwrap();
    let version_re = Regex::new(r"<version>([^<]+)</version>").unwrap();

    let name = name_re.captures(&content).map(|c| c[1].trim().to_string());
    let version = version_re
        .captures(&content)
        .map(|c| c[1].trim().to_string());

    let mut deps = Vec::new();
    for cap in re_dep.captures_iter(&content) {
        let group = cap[1].trim().to_string();
        let artifact = cap[2].trim().to_string();
        let ver = cap
            .get(3)
            .map(|v| v.as_str().trim().to_string())
            .unwrap_or_else(|| "*".to_string());
        let kind = match cap.get(4).map(|s| s.as_str().trim()) {
            Some("test") => "dev",
            Some("provided") | Some("optional") => "optional",
            Some("system") => "build",
            _ => "runtime",
        };
        deps.push(DependencyEntry {
            name: format!("{}:{}", group, artifact),
            version: ver,
            kind: kind.to_string(),
        });
    }

    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "maven".to_string(),
        name,
        version,
        dependencies: deps,
        scripts: None,
    })
}

fn parse_gradle(path: &Path, rel: &str) -> Option<ManifestEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let re = Regex::new(
        r#"(?m)^\s*(implementation|testImplementation|api|compileOnly|runtimeOnly|testCompileOnly|annotationProcessor|kapt)\s*[\("']([^"'\)]+)["'\)]"#
    ).unwrap();

    let mut deps = Vec::new();
    for cap in re.captures_iter(&content) {
        let config = &cap[1];
        let dep_str = cap[2].trim();
        let parts: Vec<&str> = dep_str.splitn(3, ':').collect();
        if parts.len() >= 2 {
            let name = format!("{}:{}", parts[0], parts[1]);
            let version = parts.get(2).unwrap_or(&"*").to_string();
            let kind = match config {
                "testImplementation" | "testCompileOnly" => "dev",
                "compileOnly" | "runtimeOnly" | "annotationProcessor" | "kapt" => "optional",
                _ => "runtime",
            };
            deps.push(DependencyEntry {
                name,
                version,
                kind: kind.to_string(),
            });
        }
    }

    Some(ManifestEntry {
        path: rel.to_string(),
        ecosystem: "gradle".to_string(),
        name: None,
        version: None,
        dependencies: deps,
        scripts: None,
    })
}
