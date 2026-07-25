//! Compact output rendering — strips the "JSON tax" from tool responses.
//!
//! In compact mode every tool response is rendered as indented plain text:
//! - homogeneous object arrays become pipe-separated tables (keys emitted once)
//! - null values and empty arrays/objects are omitted
//! - multi-line strings become indented blocks
//!
//! LLMs read this as easily as JSON, but repeated keys, braces and quotes
//! disappear, typically saving 20-40% of the response tokens.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Compact,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }
}

pub fn to_compact_text(v: &Value) -> String {
    let mut out = String::new();
    match v {
        Value::Object(_) => write_object_members(&mut out, v, 0),
        _ => write_value(&mut out, v, 0),
    }
    let trimmed = out.trim_end().to_string();
    if trimmed.is_empty() {
        "(empty)".to_string()
    } else {
        trimmed
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Array(_) | Value::Object(_))
}

fn is_empty_container(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

fn scalar_cell(v: &Value) -> String {
    match v {
        Value::String(s) => s.replace('\n', "\\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn scalar_inline(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Render an array of objects as a pipe table if every value is scalar.
fn try_table(arr: &[Value], depth: usize) -> Option<String> {
    if arr.len() < 2 {
        return None;
    }
    let mut cols: Vec<&str> = Vec::new();
    for item in arr {
        let obj = item.as_object()?;
        for (k, v) in obj {
            if !is_scalar(v) {
                return None;
            }
            if !cols.iter().any(|c| c == k) {
                cols.push(k);
            }
        }
    }
    if cols.is_empty() {
        return None;
    }
    let mut s = String::new();
    indent(&mut s, depth);
    s.push_str(&cols.join(" | "));
    s.push('\n');
    for item in arr {
        let obj = item.as_object().unwrap();
        let row: Vec<String> = cols
            .iter()
            .map(|c| obj.get(*c).map(scalar_cell).unwrap_or_default())
            .collect();
        indent(&mut s, depth);
        s.push_str(row.join(" | ").trim_end());
        s.push('\n');
    }
    Some(s)
}

fn write_object_members(out: &mut String, v: &Value, depth: usize) {
    let Some(obj) = v.as_object() else { return };
    for (k, val) in obj {
        if is_empty_container(val) {
            continue;
        }
        match val {
            Value::String(s) if s.contains('\n') => {
                indent(out, depth);
                out.push_str(k);
                out.push_str(":\n");
                for line in s.lines() {
                    indent(out, depth + 1);
                    out.push_str(line);
                    out.push('\n');
                }
            }
            v if is_scalar(v) => {
                indent(out, depth);
                out.push_str(k);
                out.push_str(": ");
                out.push_str(&scalar_inline(v));
                out.push('\n');
            }
            Value::Array(arr) => {
                indent(out, depth);
                out.push_str(k);
                if arr.iter().all(is_scalar) {
                    let joined = arr.iter().map(scalar_cell).collect::<Vec<_>>().join(", ");
                    if joined.len() <= 120 {
                        out.push_str(": [");
                        out.push_str(&joined);
                        out.push_str("]\n");
                        continue;
                    }
                }
                out.push_str(":\n");
                write_array_items(out, arr, depth + 1);
            }
            Value::Object(_) => {
                indent(out, depth);
                out.push_str(k);
                out.push_str(":\n");
                write_object_members(out, val, depth + 1);
            }
            _ => {}
        }
    }
}

fn write_array_items(out: &mut String, arr: &[Value], depth: usize) {
    if let Some(table) = try_table(arr, depth) {
        out.push_str(&table);
        return;
    }
    for item in arr {
        match item {
            v if is_scalar(v) => {
                indent(out, depth);
                out.push_str("- ");
                out.push_str(&scalar_cell(v));
                out.push('\n');
            }
            Value::Object(_) => {
                indent(out, depth);
                out.push_str("-\n");
                write_object_members(out, item, depth + 1);
            }
            Value::Array(inner) => {
                indent(out, depth);
                out.push_str("-\n");
                write_array_items(out, inner, depth + 1);
            }
            _ => {}
        }
    }
}

fn write_value(out: &mut String, v: &Value, depth: usize) {
    match v {
        Value::Array(arr) => write_array_items(out, arr, depth),
        Value::Object(_) => write_object_members(out, v, depth),
        other => {
            indent(out, depth);
            out.push_str(&scalar_inline(other));
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn homogeneous_object_array_renders_as_table() {
        let v = json!({
            "skeleton": [
                {"name": "foo", "kind": "function", "start_line": 1, "end_line": 10},
                {"name": "bar", "kind": "struct", "start_line": 12, "end_line": 20}
            ],
            "token_count": 42
        });
        let s = to_compact_text(&v);
        assert!(s.contains("name | kind | start_line | end_line"));
        assert!(s.contains("foo | function | 1 | 10"));
        assert!(s.contains("token_count: 42"));
        // keys appear once in the header, not per row
        assert_eq!(s.matches("start_line").count(), 1);
    }

    #[test]
    fn nulls_and_empty_arrays_are_omitted() {
        let v = json!({"a": null, "b": [], "c": "x"});
        let s = to_compact_text(&v);
        assert_eq!(s, "c: x");
    }

    #[test]
    fn multiline_string_becomes_indented_block() {
        let v = json!({"content": "line1\nline2"});
        let s = to_compact_text(&v);
        assert!(s.contains("content:\n  line1\n  line2"));
    }

    #[test]
    fn short_scalar_array_inlines() {
        let v = json!({"tags": ["a", "b"]});
        assert_eq!(to_compact_text(&v), "tags: [a, b]");
    }

    #[test]
    fn heterogeneous_array_falls_back_to_blocks() {
        let v = json!({"items": [{"a": 1}, {"a": {"nested": true}}]});
        let s = to_compact_text(&v);
        assert!(s.contains("-\n"));
        assert!(s.contains("nested: true"));
    }

    #[test]
    fn compact_is_smaller_than_pretty_json() {
        let v = json!({
            "skeleton": (0..50).map(|i| json!({
                "id": format!("function:{i}-{}", i+10),
                "kind": "function",
                "name": format!("fn_{i}"),
                "signature": format!("fn fn_{i}(x: usize) -> usize"),
                "start_line": i,
                "end_line": i + 10
            })).collect::<Vec<_>>(),
            "token_count": 999
        });
        let compact = to_compact_text(&v).len();
        let pretty = serde_json::to_string_pretty(&v).unwrap().len();
        assert!(compact * 2 < pretty, "compact={compact} pretty={pretty}");
    }
}
