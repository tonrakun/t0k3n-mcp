use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::fs::estimate_tokens;
use crate::security::{rel_display, safe_path};

// ─── read_notebook_cells ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadNotebookCellsParams {
    #[schemars(description = "Root-relative path to a .ipynb file.")]
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NotebookCellEntry {
    pub index: usize,
    pub cell_type: String,
    pub execution_count: Option<i64>,
    pub source_preview: String,
    pub output_count: usize,
    pub line_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadNotebookCellsResult {
    pub path: String,
    pub nbformat: u64,
    pub cells: Vec<NotebookCellEntry>,
    pub token_count: usize,
}

pub fn read_notebook_cells(
    root: &Path,
    params: ReadNotebookCellsParams,
) -> anyhow::Result<ReadNotebookCellsResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;

    let nb: Value =
        serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("JSON パース失敗: {e}"))?;

    let nbformat = nb.get("nbformat").and_then(|v| v.as_u64()).unwrap_or(4);
    let cells_raw = nb
        .get("cells")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("cells フィールドが見つかりません"))?;

    let cells: Vec<NotebookCellEntry> = cells_raw
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let cell_type = cell
                .get("cell_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let execution_count = cell.get("execution_count").and_then(|v| v.as_i64());
            let source = extract_source(cell);
            let line_count = source.lines().count();
            let source_preview = source
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(100)
                .collect();
            let output_count = cell
                .get("outputs")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            NotebookCellEntry {
                index: i,
                cell_type,
                execution_count,
                source_preview,
                output_count,
                line_count,
            }
        })
        .collect();

    let rel = rel_display(root, &file_path);
    let json = serde_json::to_string(&cells).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadNotebookCellsResult {
        path: rel,
        nbformat,
        cells,
        token_count,
    })
}

fn extract_source(cell: &Value) -> String {
    match cell.get("source") {
        Some(Value::Array(lines)) => lines
            .iter()
            .filter_map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join(""),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

// ─── read_notebook_cell ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadNotebookCellParams {
    #[schemars(description = "Root-relative path to a .ipynb file.")]
    pub path: String,
    #[schemars(description = "Cell index (0-based, from read_notebook_cells).")]
    pub index: usize,
    #[schemars(
        description = "Include cell outputs (default: false). Outputs may contain large binary data."
    )]
    pub include_outputs: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct NotebookCellOutput {
    pub output_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReadNotebookCellResult {
    pub path: String,
    pub index: usize,
    pub cell_type: String,
    pub execution_count: Option<i64>,
    pub source: String,
    pub outputs: Vec<NotebookCellOutput>,
    pub token_count: usize,
}

pub fn read_notebook_cell(
    root: &Path,
    params: ReadNotebookCellParams,
) -> anyhow::Result<ReadNotebookCellResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;

    let nb: Value =
        serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("JSON パース失敗: {e}"))?;

    let cells_raw = nb
        .get("cells")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("cells フィールドが見つかりません"))?;

    let cell = cells_raw.get(params.index).ok_or_else(|| {
        anyhow::anyhow!(
            "インデックス {} は範囲外です（全 {} セル）",
            params.index,
            cells_raw.len()
        )
    })?;

    let cell_type = cell
        .get("cell_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let execution_count = cell.get("execution_count").and_then(|v| v.as_i64());
    let source = extract_source(cell);

    let outputs = if params.include_outputs.unwrap_or(false) {
        cell.get("outputs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|o| {
                        let output_type = o
                            .get("output_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let text = match o.get("text") {
                            Some(Value::Array(lines)) => Some(
                                lines
                                    .iter()
                                    .filter_map(|l| l.as_str())
                                    .collect::<Vec<_>>()
                                    .join(""),
                            ),
                            Some(Value::String(s)) => Some(s.clone()),
                            _ => o.get("traceback").and_then(|v| v.as_array()).map(|ls| {
                                ls.iter()
                                    .filter_map(|l| l.as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }),
                        };
                        NotebookCellOutput { output_type, text }
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let rel = rel_display(root, &file_path);
    let combined = format!(
        "{}{}",
        source,
        outputs
            .iter()
            .filter_map(|o| o.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    );
    let token_count = estimate_tokens(&combined);

    Ok(ReadNotebookCellResult {
        path: rel,
        index: params.index,
        cell_type,
        execution_count,
        source,
        outputs,
        token_count,
    })
}
