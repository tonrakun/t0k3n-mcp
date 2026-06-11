use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::security::safe_path;
use super::fs::estimate_tokens;

const ENV_TEMPLATE_NAMES: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.dist",
    ".env.local.example",
];

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadEnvSchemaParams {
    #[schemars(description = "Root-relative path to a specific .env* or docker-compose.yml file. Omit to auto-scan workspace root for .env.example / .env.sample / .env.template / .env.dist and docker-compose.yml.")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvVarEntry {
    pub key: String,
    pub default_value: Option<String>,
    pub description: Option<String>,
    pub required: bool,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadEnvSchemaResult {
    pub vars: Vec<EnvVarEntry>,
    pub sources: Vec<String>,
    pub token_count: usize,
}

pub fn read_env_schema(root: &Path, params: ReadEnvSchemaParams) -> anyhow::Result<ReadEnvSchemaResult> {
    let mut all_vars: Vec<EnvVarEntry> = Vec::new();
    let mut sources: Vec<String> = Vec::new();

    if let Some(ref p) = params.path {
        let full = safe_path(root, p)?;
        let fname = full.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if fname == "docker-compose.yml" || fname == "docker-compose.yaml" {
            let vars = parse_docker_compose(&full, p)?;
            if !vars.is_empty() { sources.push(p.clone()); all_vars.extend(vars); }
        } else {
            let content = std::fs::read_to_string(&full)
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", p, e))?;
            let vars = parse_env_file(&content, p);
            if !vars.is_empty() { sources.push(p.clone()); all_vars.extend(vars); }
        }
    } else {
        for name in ENV_TEMPLATE_NAMES {
            let candidate = root.join(name);
            if !candidate.exists() { continue; }
            let content = std::fs::read_to_string(&candidate).unwrap_or_default();
            let vars = parse_env_file(&content, name);
            if !vars.is_empty() { sources.push(name.to_string()); all_vars.extend(vars); }
        }
        for dc in &["docker-compose.yml", "docker-compose.yaml"] {
            let candidate = root.join(dc);
            if !candidate.exists() { continue; }
            if let Ok(vars) = parse_docker_compose(&candidate, dc)
                && !vars.is_empty() { sources.push(dc.to_string()); all_vars.extend(vars); }
        }
    }

    // Dedup: first occurrence per key wins
    let mut seen: HashSet<String> = HashSet::new();
    all_vars.retain(|v| seen.insert(v.key.clone()));

    let json = serde_json::to_string(&all_vars).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadEnvSchemaResult { vars: all_vars, sources, token_count })
}

fn parse_env_file(content: &str, source: &str) -> Vec<EnvVarEntry> {
    let mut vars = Vec::new();
    let mut pending_desc: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            pending_desc.clear();
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix('#') {
            let c = comment.trim();
            if !c.is_empty() { pending_desc.push(c.to_string()); }
            continue;
        }
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                pending_desc.clear();
                continue;
            }
            let raw = trimmed[eq_pos + 1..].trim();
            let default_value = if raw.is_empty() {
                None
            } else {
                Some(raw.trim_matches('"').trim_matches('\'').to_string())
            };
            vars.push(EnvVarEntry {
                key,
                required: default_value.is_none(),
                description: if pending_desc.is_empty() { None } else { Some(pending_desc.join(" ")) },
                default_value,
                source: source.to_string(),
            });
        }
        pending_desc.clear();
    }
    vars
}

fn parse_docker_compose(path: &Path, source: &str) -> anyhow::Result<Vec<EnvVarEntry>> {
    let content = std::fs::read_to_string(path)?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&content)?;
    let mut vars = Vec::new();

    let services = match doc.as_mapping().and_then(|m| m.get("services")) {
        Some(v) => v.clone(),
        None => return Ok(vars),
    };
    let svc_map = match services.as_mapping() {
        Some(m) => m.clone(),
        None => return Ok(vars),
    };

    for (_svc, svc_val) in &svc_map {
        let env_val = match svc_val.as_mapping().and_then(|m| m.get("environment")) {
            Some(v) => v.clone(),
            None => continue,
        };

        if let Some(seq) = env_val.as_sequence() {
            for item in seq {
                if let Some(s) = item.as_str() {
                    push_kv(s, source, &mut vars);
                }
            }
        } else if let Some(map) = env_val.as_mapping() {
            for (k, v) in map {
                let Some(key) = k.as_str() else { continue };
                let default_value = if v.is_null() {
                    None
                } else if let Some(s) = v.as_str() {
                    if s.is_empty() { None } else { Some(s.to_string()) }
                } else if let Some(n) = v.as_i64() {
                    Some(n.to_string())
                } else { v.as_bool().map(|b| b.to_string()) };
                vars.push(EnvVarEntry {
                    key: key.to_string(),
                    required: default_value.is_none(),
                    description: None,
                    default_value,
                    source: source.to_string(),
                });
            }
        }
    }
    Ok(vars)
}

fn push_kv(s: &str, source: &str, vars: &mut Vec<EnvVarEntry>) {
    if let Some(eq) = s.find('=') {
        let key = s[..eq].trim().to_string();
        let raw = s[eq + 1..].trim();
        let default_value = if raw.is_empty() { None } else { Some(raw.to_string()) };
        vars.push(EnvVarEntry {
            key,
            required: default_value.is_none(),
            description: None,
            default_value,
            source: source.to_string(),
        });
    }
}
