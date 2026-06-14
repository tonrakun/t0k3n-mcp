//! set_config_value — write counterpart of read_json_yaml_value (Phase 15).
//!
//! Sets a value at a dot-notation key path in a JSON / YAML / TOML file,
//! creating intermediate objects as needed. Round-trips through serde_json
//! (preserve_order is on, so JSON key order is kept). Comments in YAML/TOML are
//! best-effort only — re-serialization drops them. Opt-in write tool.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

use super::json_yaml::{PathSegment, parse_file, tokenize_path};
use super::writes::unified_diff;
use crate::security::safe_path;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetConfigValueParams {
    #[schemars(description = "Root-relative path to a JSON, YAML, or TOML file")]
    pub path: String,
    #[schemars(description = "Dot-notation key path, e.g. 'scripts.build' or 'items[0].name'. Intermediate objects are created if missing.")]
    pub key_path: String,
    #[schemars(description = "New value (any JSON type: string, number, bool, object, array, null)")]
    pub value: Value,
    #[schemars(description = "true = return the would-be diff without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SetConfigValueResult {
    pub old_value: Value,
    pub new_value: Value,
    pub created: bool,
    pub diff: String,
    pub written: bool,
}

/// Set `new` at the path described by `segs`, returning the previous value
/// (Null if the key did not exist). Creates intermediate objects for Key
/// segments; Index segments require an existing array slot.
fn set_at_path(cur: &mut Value, segs: &[PathSegment], new: Value) -> anyhow::Result<Value> {
    match segs {
        [] => anyhow::bail!("empty key_path"),
        [seg] => match seg {
            PathSegment::Key(k) => {
                if cur.is_null() {
                    *cur = Value::Object(Map::new());
                }
                let obj = cur
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("cannot set key '{k}': parent is not an object"))?;
                Ok(obj.insert(k.clone(), new).unwrap_or(Value::Null))
            }
            PathSegment::Index(i) => {
                let arr = cur
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("cannot index [{i}]: parent is not an array"))?;
                let slot = arr
                    .get_mut(*i)
                    .ok_or_else(|| anyhow::anyhow!("index {i} out of bounds"))?;
                Ok(std::mem::replace(slot, new))
            }
        },
        [seg, rest @ ..] => match seg {
            PathSegment::Key(k) => {
                if cur.is_null() {
                    *cur = Value::Object(Map::new());
                }
                let obj = cur
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("path segment '{k}' is not an object"))?;
                let child = obj.entry(k.clone()).or_insert(Value::Object(Map::new()));
                set_at_path(child, rest, new)
            }
            PathSegment::Index(i) => {
                let arr = cur
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("cannot index [{i}]: not an array"))?;
                let child = arr
                    .get_mut(*i)
                    .ok_or_else(|| anyhow::anyhow!("index {i} out of bounds"))?;
                set_at_path(child, rest, new)
            }
        },
    }
}

fn serialize(ext: &str, value: &Value) -> anyhow::Result<String> {
    match ext {
        "yaml" | "yml" => Ok(serde_yaml::to_string(value)?),
        "toml" => {
            let toml_val = toml::Value::try_from(value)
                .map_err(|e| anyhow::anyhow!("cannot represent value as TOML: {e}"))?;
            Ok(toml::to_string_pretty(&toml_val)?)
        }
        _ => Ok(serde_json::to_string_pretty(value)?),
    }
}

pub fn set_config_value(
    root: &Path,
    params: SetConfigValueParams,
) -> anyhow::Result<SetConfigValueResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let uses_crlf = content.contains("\r\n");
    let had_trailing_newline = content.ends_with('\n');

    let mut value = parse_file(&path, &content)?;
    let segments = tokenize_path(&params.key_path);
    if segments.is_empty() {
        anyhow::bail!("key_path must not be empty");
    }

    let old_value = set_at_path(&mut value, &segments, params.value.clone())?;
    let created = old_value.is_null();

    // Re-serialize, normalizing trailing newline and EOL to match the original.
    let mut out = serialize(ext, &value)?;
    let out_normalized = out.replace("\r\n", "\n");
    out = out_normalized.trim_end_matches('\n').to_string();
    if had_trailing_newline {
        out.push('\n');
    }
    if uses_crlf {
        out = out.replace('\n', "\r\n");
    }

    let diff = unified_diff(&content, &out);

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        std::fs::write(&path, &out)?;
    }

    Ok(SetConfigValueResult {
        old_value,
        new_value: params.value,
        created,
        diff,
        written: !dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str, content: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), content).unwrap();
        dir
    }

    #[test]
    fn sets_existing_json_key_preserving_order() {
        let dir = setup("p.json", "{\n  \"name\": \"x\",\n  \"version\": \"1.0.0\"\n}\n");
        let r = set_config_value(
            dir.path(),
            SetConfigValueParams {
                path: "p.json".into(),
                key_path: "version".into(),
                value: Value::String("2.0.0".into()),
                dry_run: None,
            },
        )
        .unwrap();
        assert!(r.written);
        assert!(!r.created);
        assert_eq!(r.old_value, Value::String("1.0.0".into()));
        let out = std::fs::read_to_string(dir.path().join("p.json")).unwrap();
        assert!(out.contains("\"2.0.0\""));
        // key order preserved: name before version
        assert!(out.find("name").unwrap() < out.find("version").unwrap());
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn creates_nested_key() {
        let dir = setup("p.json", "{\n  \"name\": \"x\"\n}\n");
        let r = set_config_value(
            dir.path(),
            SetConfigValueParams {
                path: "p.json".into(),
                key_path: "scripts.build".into(),
                value: Value::String("cargo build".into()),
                dry_run: None,
            },
        )
        .unwrap();
        assert!(r.created);
        let out = std::fs::read_to_string(dir.path().join("p.json")).unwrap();
        assert!(out.contains("scripts"));
        assert!(out.contains("cargo build"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = setup("p.json", "{\"a\":1}\n");
        let r = set_config_value(
            dir.path(),
            SetConfigValueParams {
                path: "p.json".into(),
                key_path: "a".into(),
                value: Value::from(2),
                dry_run: Some(true),
            },
        )
        .unwrap();
        assert!(!r.written);
        assert_eq!(std::fs::read_to_string(dir.path().join("p.json")).unwrap(), "{\"a\":1}\n");
        assert!(r.diff.contains("2"));
    }

    #[test]
    fn sets_yaml_value() {
        let dir = setup("c.yaml", "name: x\nport: 8080\n");
        set_config_value(
            dir.path(),
            SetConfigValueParams {
                path: "c.yaml".into(),
                key_path: "port".into(),
                value: Value::from(9090),
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join("c.yaml")).unwrap();
        assert!(out.contains("9090"));
    }

    #[test]
    fn sets_toml_value() {
        let dir = setup("c.toml", "name = \"x\"\nversion = \"1.0.0\"\n");
        set_config_value(
            dir.path(),
            SetConfigValueParams {
                path: "c.toml".into(),
                key_path: "version".into(),
                value: Value::String("2.0.0".into()),
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join("c.toml")).unwrap();
        assert!(out.contains("2.0.0"));
    }

    #[test]
    fn array_index_out_of_bounds_errors() {
        let dir = setup("p.json", "{\"items\":[1,2]}\n");
        let r = set_config_value(
            dir.path(),
            SetConfigValueParams {
                path: "p.json".into(),
                key_path: "items[5]".into(),
                value: Value::from(9),
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }
}
