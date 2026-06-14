//! read_dependency_audit — drives the ecosystem's vulnerability scanner
//! check-only and normalizes the result. The dependency-side counterpart to
//! read_security_surface (which scans your own code). Like read_type_diagnostics,
//! a missing scanner returns a non-error note with an install hint, so the tool
//! is safe to call speculatively.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use super::diagnostics::{looks_unavailable, run_shell};
use super::fs::estimate_tokens;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadDependencyAuditParams {
    #[schemars(description = "Only return vulnerabilities at this severity or above: low | moderate | high | critical.")]
    pub severity: Option<String>,
    #[schemars(description = "Cap the number of vulnerabilities returned (after severity sort).")]
    pub max_items: Option<usize>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Vulnerability {
    pub package: String,
    pub severity: String,
    pub id: String,
    pub affected: String,
    pub patched: Option<String>,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct ReadDependencyAuditResult {
    pub scanner_available: bool,
    pub ecosystem: Option<String>,
    pub vulnerabilities: Vec<Vulnerability>,
    pub hint: Option<String>,
    pub token_count: usize,
}

fn severity_rank(s: &str) -> u8 {
    match s.to_lowercase().as_str() {
        "critical" => 4,
        "high" => 3,
        "moderate" | "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

/// Map a CVSS base score to a severity bucket (used by osv-scanner / cargo audit).
fn cvss_to_severity(score: f64) -> String {
    if score >= 9.0 {
        "critical"
    } else if score >= 7.0 {
        "high"
    } else if score >= 4.0 {
        "moderate"
    } else if score > 0.0 {
        "low"
    } else {
        "unknown"
    }
    .to_string()
}

#[derive(Clone, Copy)]
enum Ecosystem {
    Cargo,
    Npm,
    Pip,
    Osv,
}

impl Ecosystem {
    fn name(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo",
            Ecosystem::Npm => "npm",
            Ecosystem::Pip => "pip",
            Ecosystem::Osv => "osv",
        }
    }
    fn command(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo audit --json",
            Ecosystem::Npm => "npm audit --json",
            Ecosystem::Pip => "pip-audit -f json",
            Ecosystem::Osv => "osv-scanner --format json -r .",
        }
    }
    fn install_hint(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo install cargo-audit で導入してください",
            Ecosystem::Npm => "Node.js / npm を導入してください",
            Ecosystem::Pip => "pip install pip-audit で導入してください",
            Ecosystem::Osv => "osv-scanner を導入してください（github.com/google/osv-scanner）",
        }
    }
}

fn detect_ecosystem(root: &Path) -> Option<Ecosystem> {
    if root.join("Cargo.toml").exists() {
        Some(Ecosystem::Cargo)
    } else if root.join("package.json").exists() {
        Some(Ecosystem::Npm)
    } else if root.join("pyproject.toml").exists()
        || root.join("requirements.txt").exists()
    {
        Some(Ecosystem::Pip)
    } else if root.join("go.mod").exists() {
        // No native go scanner here; osv-scanner covers Go modules.
        Some(Ecosystem::Osv)
    } else {
        None
    }
}

pub fn read_dependency_audit(
    root: &Path,
    params: ReadDependencyAuditParams,
) -> anyhow::Result<ReadDependencyAuditResult> {
    let Some(eco) = detect_ecosystem(root) else {
        return Ok(ReadDependencyAuditResult {
            scanner_available: false,
            ecosystem: None,
            vulnerabilities: Vec::new(),
            hint: Some(
                "No supported manifest found (Cargo.toml / package.json / pyproject.toml / requirements.txt / go.mod).".to_string(),
            ),
            token_count: 0,
        });
    };

    let (stdout, stderr) = run_shell(eco.command(), root, TIMEOUT);

    // Scanners exit non-zero when they find vulnerabilities, so we can't use the
    // exit code. Instead: empty stdout + an "unavailable" stderr means no scanner.
    if stdout.trim().is_empty() && looks_unavailable(&stderr) {
        return Ok(ReadDependencyAuditResult {
            scanner_available: false,
            ecosystem: Some(eco.name().to_string()),
            vulnerabilities: Vec::new(),
            hint: Some(format!(
                "{} が見つかりませんでした（スキップ）。{}。",
                eco.command(),
                eco.install_hint()
            )),
            token_count: 0,
        });
    }

    let mut vulns = match eco {
        Ecosystem::Cargo => parse_cargo_audit(&stdout),
        Ecosystem::Npm => parse_npm_audit(&stdout),
        Ecosystem::Pip => parse_pip_audit(&stdout),
        Ecosystem::Osv => parse_osv(&stdout),
    };

    // Severity filter (minimum level).
    if let Some(min) = &params.severity {
        let min_rank = severity_rank(min);
        vulns.retain(|v| severity_rank(&v.severity) >= min_rank);
    }

    // Sort by severity desc, then package name for determinism.
    vulns.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then(a.package.cmp(&b.package))
            .then(a.id.cmp(&b.id))
    });

    if let Some(max) = params.max_items {
        vulns.truncate(max);
    }

    let json = serde_json::to_string(&vulns).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadDependencyAuditResult {
        scanner_available: true,
        ecosystem: Some(eco.name().to_string()),
        vulnerabilities: vulns,
        hint: None,
        token_count,
    })
}

fn truncate_title(s: &str) -> String {
    let one_line = s.replace('\n', " ");
    if one_line.chars().count() > 160 {
        let truncated: String = one_line.chars().take(157).collect();
        format!("{truncated}...")
    } else {
        one_line
    }
}

/// npm audit --json (npm v7+): top-level `vulnerabilities` map.
fn parse_npm_audit(stdout: &str) -> Vec<Vulnerability> {
    let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(stdout) else {
        return Vec::new();
    };
    let Some(map) = v.get("vulnerabilities").and_then(|x| x.as_object()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, info) in map {
        let severity = info
            .get("severity")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();
        let range = info
            .get("range")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let patched = match info.get("fixAvailable") {
            Some(serde_json::Value::Bool(true)) => Some("fix available".to_string()),
            Some(serde_json::Value::Object(o)) => o
                .get("version")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            _ => None,
        };
        // Pull the first advisory object from `via` for id/title.
        let (id, title) = info
            .get("via")
            .and_then(|x| x.as_array())
            .and_then(|arr| arr.iter().find(|e| e.is_object()))
            .map(|adv| {
                let id = adv
                    .get("url")
                    .and_then(|u| u.as_str())
                    .or_else(|| adv.get("source").and_then(|s| s.as_str()))
                    .unwrap_or("")
                    .to_string();
                let title = adv
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                (id, title)
            })
            .unwrap_or_default();
        out.push(Vulnerability {
            package: name.clone(),
            severity,
            id,
            affected: range,
            patched,
            title: truncate_title(&title),
        });
    }
    out
}

/// cargo audit --json: `vulnerabilities.list[]`.
fn parse_cargo_audit(stdout: &str) -> Vec<Vulnerability> {
    let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(stdout) else {
        return Vec::new();
    };
    let Some(list) = v
        .get("vulnerabilities")
        .and_then(|x| x.get("list"))
        .and_then(|x| x.as_array())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in list {
        let advisory = item.get("advisory");
        let id = advisory
            .and_then(|a| a.get("id"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let title = advisory
            .and_then(|a| a.get("title"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let severity = advisory
            .and_then(|a| a.get("cvss"))
            .and_then(|x| x.as_str())
            .and_then(parse_cvss_score)
            .map(cvss_to_severity)
            .unwrap_or_else(|| "unknown".to_string());
        let package = item
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let affected = item
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let patched = item
            .get("versions")
            .and_then(|x| x.get("patched"))
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty());
        out.push(Vulnerability {
            package,
            severity,
            id,
            affected,
            patched,
            title: truncate_title(&title),
        });
    }
    out
}

/// CVSS vector or bare score → numeric base score (best effort).
fn parse_cvss_score(s: &str) -> Option<f64> {
    s.parse::<f64>().ok()
}

/// pip-audit -f json: `{ "dependencies": [ { name, version, vulns: [...] } ] }`.
fn parse_pip_audit(stdout: &str) -> Vec<Vulnerability> {
    let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(stdout) else {
        return Vec::new();
    };
    // Newer format nests under "dependencies"; older was a bare array.
    let deps = v
        .get("dependencies")
        .and_then(|x| x.as_array())
        .or_else(|| v.as_array());
    let Some(deps) = deps else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dep in deps {
        let name = dep.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let version = dep
            .get("version")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let Some(vulns) = dep.get("vulns").and_then(|x| x.as_array()) else {
            continue;
        };
        for vuln in vulns {
            let id = vuln.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let title = vuln
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let patched = vuln
                .get("fix_versions")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|s| !s.is_empty());
            out.push(Vulnerability {
                package: name.clone(),
                // pip-audit does not reliably emit severity.
                severity: "unknown".to_string(),
                id,
                affected: version.clone(),
                patched,
                title: truncate_title(&title),
            });
        }
    }
    out
}

/// osv-scanner --format json: `results[].packages[].{package, vulnerabilities, groups}`.
fn parse_osv(stdout: &str) -> Vec<Vulnerability> {
    let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(stdout) else {
        return Vec::new();
    };
    let Some(results) = v.get("results").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for result in results {
        let Some(packages) = result.get("packages").and_then(|x| x.as_array()) else {
            continue;
        };
        for pkg in packages {
            let name = pkg
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let version = pkg
                .get("package")
                .and_then(|p| p.get("version"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            // groups carry max_severity (CVSS score string).
            let group_severity = pkg
                .get("groups")
                .and_then(|x| x.as_array())
                .and_then(|g| g.first())
                .and_then(|g| g.get("max_severity"))
                .and_then(|x| x.as_str())
                .and_then(parse_cvss_score)
                .map(cvss_to_severity);
            let Some(vulns) = pkg.get("vulnerabilities").and_then(|x| x.as_array()) else {
                continue;
            };
            for vuln in vulns {
                let id = vuln.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let title = vuln
                    .get("summary")
                    .and_then(|x| x.as_str())
                    .or_else(|| vuln.get("details").and_then(|x| x.as_str()))
                    .unwrap_or("")
                    .to_string();
                out.push(Vulnerability {
                    package: name.clone(),
                    severity: group_severity.clone().unwrap_or_else(|| "unknown".to_string()),
                    id,
                    affected: version.clone(),
                    patched: None,
                    title: truncate_title(&title),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ranking_and_cvss_buckets() {
        assert!(severity_rank("critical") > severity_rank("high"));
        assert!(severity_rank("high") > severity_rank("moderate"));
        assert!(severity_rank("moderate") > severity_rank("low"));
        assert_eq!(cvss_to_severity(9.8), "critical");
        assert_eq!(cvss_to_severity(7.5), "high");
        assert_eq!(cvss_to_severity(5.0), "moderate");
        assert_eq!(cvss_to_severity(2.0), "low");
    }

    #[test]
    fn parses_npm_audit() {
        let json = r#"{
            "vulnerabilities": {
                "lodash": {
                    "name": "lodash",
                    "severity": "high",
                    "range": "<4.17.21",
                    "fixAvailable": { "name": "lodash", "version": "4.17.21" },
                    "via": [ { "source": 1065, "title": "Prototype Pollution", "url": "https://github.com/advisories/GHSA-x", "severity": "high" } ]
                }
            }
        }"#;
        let v = parse_npm_audit(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].package, "lodash");
        assert_eq!(v[0].severity, "high");
        assert_eq!(v[0].title, "Prototype Pollution");
        assert_eq!(v[0].patched.as_deref(), Some("4.17.21"));
    }

    #[test]
    fn parses_cargo_audit() {
        let json = r#"{
            "vulnerabilities": {
                "list": [
                    {
                        "advisory": { "id": "RUSTSEC-2021-0001", "title": "Some flaw", "cvss": "7.5" },
                        "package": { "name": "time", "version": "0.1.0" },
                        "versions": { "patched": [">=0.2.23"] }
                    }
                ]
            }
        }"#;
        let v = parse_cargo_audit(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "RUSTSEC-2021-0001");
        assert_eq!(v[0].package, "time");
        assert_eq!(v[0].severity, "high");
        assert_eq!(v[0].patched.as_deref(), Some(">=0.2.23"));
    }

    #[test]
    fn parses_pip_audit() {
        let json = r#"{
            "dependencies": [
                { "name": "flask", "version": "0.5", "vulns": [ { "id": "PYSEC-2019-1", "description": "XSS", "fix_versions": ["0.12.3"] } ] },
                { "name": "safe", "version": "1.0", "vulns": [] }
            ]
        }"#;
        let v = parse_pip_audit(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "PYSEC-2019-1");
        assert_eq!(v[0].package, "flask");
        assert_eq!(v[0].patched.as_deref(), Some("0.12.3"));
    }

    #[test]
    fn parses_osv() {
        let json = r#"{
            "results": [
                {
                    "packages": [
                        {
                            "package": { "name": "golang.org/x/net", "version": "0.1.0", "ecosystem": "Go" },
                            "vulnerabilities": [ { "id": "GHSA-abcd", "summary": "HTTP/2 flaw" } ],
                            "groups": [ { "ids": ["GHSA-abcd"], "max_severity": "7.5" } ]
                        }
                    ]
                }
            ]
        }"#;
        let v = parse_osv(json);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "GHSA-abcd");
        assert_eq!(v[0].severity, "high");
        assert_eq!(v[0].title, "HTTP/2 flaw");
    }

    #[test]
    fn malformed_json_yields_empty() {
        assert!(parse_npm_audit("not json").is_empty());
        assert!(parse_cargo_audit("{}").is_empty());
        assert!(parse_osv("{}").is_empty());
    }

    #[test]
    fn no_manifest_is_non_error() {
        let dir = tempfile::tempdir().unwrap();
        let r = read_dependency_audit(
            dir.path(),
            ReadDependencyAuditParams { severity: None, max_items: None },
        )
        .unwrap();
        assert!(!r.scanner_available);
        assert!(r.hint.is_some());
    }
}
