use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::code::{ReadCodeBodyParams, ReadCodeSkeletonParams, read_code_body, read_code_skeleton};
use super::fs::estimate_tokens;
use super::json_yaml::{ReadJsonYamlValueParams, read_json_yaml_value};
use super::markdown::{ReadMarkdownSectionParams, read_markdown_section};
use super::outline::{ReadFileOutlineParams, read_file_outline};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchReadItem {
    #[schemars(description = "Client-assigned ID to correlate results")]
    pub id: String,
    #[schemars(description = "Operation: code_skeleton | code_body | markdown_section | json_value | file_outline")]
    pub operation: String,
    #[schemars(description = "Root-relative file path")]
    pub path: String,
    #[schemars(description = "Skeleton IDs for code_body operation")]
    pub ids: Option<Vec<String>>,
    #[schemars(description = "Heading anchors for markdown_section operation")]
    pub anchors: Option<Vec<String>>,
    #[schemars(description = "Key path for json_value operation (e.g. 'dependencies.tokio')")]
    pub key_path: Option<String>,
    #[schemars(description = "Include block constructs for code_skeleton operation")]
    pub include_blocks: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchReadParams {
    #[schemars(description = "List of read operations to execute")]
    pub reads: Vec<BatchReadItem>,
    #[schemars(description = "When true, near-identical results (e.g. migrations, fixtures) are returned as one full template plus per-file unified diffs against it, instead of repeating similar content. Default false.")]
    pub factor: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct BatchReadItemResult {
    pub id: String,
    pub ok: bool,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// When factored: the ID of the template this result was diffed against.
    /// Its `data` then holds `{ template_ref, diff }` instead of the full content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_ref: Option<String>,
    pub token_count: usize,
}

#[derive(Debug, Serialize)]
pub struct BatchReadResult {
    pub results: Vec<BatchReadItemResult>,
    pub factored: usize,
    pub total_token_count: usize,
}

pub fn batch_read(root: &Path, params: BatchReadParams) -> anyhow::Result<BatchReadResult> {
    let mut results = Vec::new();

    for item in &params.reads {
        let (ok, data, error, token_count) = match execute_item(root, item) {
            Ok((data, tc)) => (true, data, None, tc),
            Err(e) => (false, Value::Null, Some(e.to_string()), 0),
        };
        results.push(BatchReadItemResult {
            id: item.id.clone(),
            ok,
            data,
            error,
            template_ref: None,
            token_count,
        });
    }

    let factored = if params.factor.unwrap_or(false) {
        factor_results(&mut results)
    } else {
        0
    };

    let total_token_count = results.iter().map(|r| r.token_count).sum();
    Ok(BatchReadResult { results, factored, total_token_count })
}

/// Minimum line-similarity (0.0–1.0) for two results to be factored together.
const FACTOR_THRESHOLD: f64 = 0.5;

/// Collapse near-identical results into template + diff. The first result of each
/// similar group keeps its full `data`; the rest get `{ template_ref, diff }`.
/// Returns the number of results that were factored. Order is preserved.
fn factor_results(results: &mut [BatchReadItemResult]) -> usize {
    let texts: Vec<Option<String>> = results
        .iter()
        .map(|r| if r.ok { factorable_text(&r.data) } else { None })
        .collect();

    let n = results.len();
    let mut assigned = vec![false; n];
    let mut factored = 0;

    for i in 0..n {
        if assigned[i] || texts[i].is_none() {
            continue;
        }
        let tmpl_text = texts[i].as_ref().unwrap();
        let tmpl_id = results[i].id.clone();

        for j in (i + 1)..n {
            if assigned[j] || texts[j].is_none() {
                continue;
            }
            let cand = texts[j].as_ref().unwrap();
            let td = similar::TextDiff::from_lines(tmpl_text, cand);
            if td.ratio() < FACTOR_THRESHOLD as f32 {
                continue;
            }
            let diff = td
                .unified_diff()
                .context_radius(2)
                .header(&tmpl_id, &results[j].id)
                .to_string();
            // Only worthwhile if the diff is actually smaller than the content.
            if diff.len() >= cand.len() {
                continue;
            }
            assigned[j] = true;
            factored += 1;
            results[j].token_count = estimate_tokens(&diff);
            results[j].template_ref = Some(tmpl_id.clone());
            results[j].data = serde_json::json!({ "template_ref": tmpl_id, "diff": diff });
        }
    }

    factored
}

/// Extract a line-structured text view of a result for similarity comparison.
/// Handles the common shapes (arrays of `{content}` / `{section}`, scalar
/// strings) and falls back to pretty JSON so any result can still be compared.
fn factorable_text(data: &Value) -> Option<String> {
    if let Some(arr) = data.as_array() {
        let mut s = String::new();
        for el in arr {
            let chunk = el
                .get("content")
                .or_else(|| el.get("section"))
                .or_else(|| el.get("value"))
                .and_then(|v| v.as_str());
            match chunk {
                Some(c) => {
                    s.push_str(c);
                    s.push('\n');
                }
                None => return None,
            }
        }
        return if s.is_empty() { None } else { Some(s) };
    }
    if let Some(s) = data.as_str() {
        return Some(s.to_string());
    }
    if data.is_null() {
        return None;
    }
    serde_json::to_string_pretty(data).ok()
}

fn execute_item(root: &Path, item: &BatchReadItem) -> anyhow::Result<(Value, usize)> {
    match item.operation.as_str() {
        "code_skeleton" => {
            let result = read_code_skeleton(root, ReadCodeSkeletonParams {
                path: item.path.clone(),
                include_blocks: item.include_blocks,
            })?;
            let data = serde_json::json!({
                "language": result.language,
                "skeleton": result.skeleton,
            });
            Ok((data, result.token_count))
        }
        "code_body" => {
            let ids = item.ids.clone().unwrap_or_default();
            anyhow::ensure!(!ids.is_empty(), "code_body requires ids");
            let result = read_code_body(root, ReadCodeBodyParams {
                path: item.path.clone(),
                ids,
                zoom: None,
            })?;
            let data = serde_json::to_value(&result.items)?;
            Ok((data, result.token_count))
        }
        "markdown_section" => {
            let anchors = item.anchors.clone().unwrap_or_default();
            anyhow::ensure!(!anchors.is_empty(), "markdown_section requires anchors");
            let result = read_markdown_section(root, ReadMarkdownSectionParams {
                path: item.path.clone(),
                anchors,
            })?;
            let data = serde_json::to_value(&result.sections)?;
            Ok((data, result.token_count))
        }
        "json_value" => {
            let key_path = item.key_path.clone().unwrap_or_default();
            anyhow::ensure!(!key_path.is_empty(), "json_value requires key_path");
            let result = read_json_yaml_value(root, ReadJsonYamlValueParams {
                path: item.path.clone(),
                key_path,
            })?;
            let tc = estimate_tokens(&result.value.to_string());
            Ok((result.value, tc))
        }
        "file_outline" => {
            let result = read_file_outline(root, ReadFileOutlineParams {
                path: item.path.clone(),
            })?;
            let data = serde_json::to_value(&result)?;
            let tc = estimate_tokens(&data.to_string());
            Ok((data, tc))
        }
        other => anyhow::bail!("Unknown operation: {}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, content: &str) -> BatchReadItemResult {
        BatchReadItemResult {
            id: id.to_string(),
            ok: true,
            data: serde_json::json!([{ "id": "function:1-3", "content": content }]),
            error: None,
            template_ref: None,
            token_count: estimate_tokens(content),
        }
    }

    #[test]
    fn factors_near_identical_results() {
        // Realistic: a long shared body with a single differing line, so the
        // unified diff is much smaller than re-sending the whole content.
        let shared: String = (0..30).map(|i| format!("    stmt_{i}();\n")).collect();
        let base = format!("fn migrate() {{\n{shared}    add_index(\"users\", \"email\");\n}}");
        let similar = format!("fn migrate() {{\n{shared}    add_index(\"orders\", \"email\");\n}}");
        let mut results = vec![item("a", &base), item("b", &similar)];

        let n = factor_results(&mut results);
        assert_eq!(n, 1);
        // template keeps full data
        assert!(results[0].template_ref.is_none());
        // second is factored into a diff referencing the template
        assert_eq!(results[1].template_ref.as_deref(), Some("a"));
        assert!(results[1].data.get("diff").is_some());
        assert_eq!(results[1].data.get("template_ref").unwrap(), "a");
    }

    #[test]
    fn leaves_dissimilar_results_untouched() {
        let mut results = vec![
            item("a", "completely unrelated alpha content here\nline two\nline three"),
            item("b", "totally different beta material\nnothing alike\nzzz"),
        ];
        let n = factor_results(&mut results);
        assert_eq!(n, 0);
        assert!(results[1].template_ref.is_none());
    }

    #[test]
    fn factorable_text_extracts_content_and_scalars() {
        let arr = serde_json::json!([{ "content": "a\nb" }, { "content": "c" }]);
        assert_eq!(factorable_text(&arr).as_deref(), Some("a\nb\nc\n"));
        assert_eq!(factorable_text(&serde_json::json!("scalar")).as_deref(), Some("scalar"));
        assert_eq!(factorable_text(&Value::Null), None);
    }
}
