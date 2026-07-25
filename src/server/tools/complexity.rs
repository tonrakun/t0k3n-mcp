use std::path::Path;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::code::{ReadCodeSkeletonParams, read_code_skeleton};
use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadComplexityMapParams {
    #[schemars(description = "Root-relative file or directory. Omit to scan entire workspace.")]
    pub path: Option<String>,
    #[schemars(description = "Only return functions with complexity >= this value (default: 1)")]
    pub min_complexity: Option<u32>,
    #[schemars(description = "Maximum number of results sorted by complexity desc (default: 50)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ComplexityEntry {
    pub path: String,
    pub function: String,
    pub complexity: u32,
    pub start_line: usize,
    pub end_line: usize,
    /// "low" (1-5) | "medium" (6-10) | "high" (11-20) | "critical" (21+)
    pub risk: String,
}

#[derive(Debug, Serialize)]
pub struct ReadComplexityMapResult {
    pub entries: Vec<ComplexityEntry>,
    pub total_analyzed: usize,
    pub high_risk_count: usize,
    pub token_count: usize,
}

pub fn read_complexity_map(
    root: &Path,
    params: ReadComplexityMapParams,
) -> anyhow::Result<ReadComplexityMapResult> {
    let min_complexity = params.min_complexity.unwrap_or(1);
    let limit = params.limit.unwrap_or(50);

    let search_root =
        scoped_root(root, params.path.as_deref()).map_err(|e| anyhow::anyhow!("{e}"))?;

    let code_exts = [
        "rs", "py", "js", "jsx", "ts", "tsx", "go", "cpp", "cc", "cxx", "c", "java", "rb", "cs",
        "php", "kt", "swift", "lua",
    ];

    let mut entries: Vec<ComplexityEntry> = Vec::new();
    let mut total_analyzed: usize = 0;

    for entry in WalkBuilder::new(&search_root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !code_exts.contains(&ext) {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = rel_display(root, path);

        let skeleton_params = ReadCodeSkeletonParams {
            path: rel.clone(),
            include_blocks: Some(false),
        };
        let Ok(skeleton) = read_code_skeleton(root, skeleton_params) else {
            continue;
        };

        let lines: Vec<&str> = content.lines().collect();

        for item in &skeleton.skeleton {
            if !matches!(item.kind.as_str(), "function" | "method") {
                continue;
            }
            total_analyzed += 1;

            let start = item.start_line.saturating_sub(1);
            let end = item.end_line.min(lines.len());
            if start >= end {
                continue;
            }
            let body = lines[start..end].join("\n");
            let complexity = compute_complexity(&body, ext);

            if complexity < min_complexity {
                continue;
            }

            let risk = risk_label(complexity);
            entries.push(ComplexityEntry {
                path: rel.clone(),
                function: item.name.clone(),
                complexity,
                start_line: item.start_line,
                end_line: item.end_line,
                risk,
            });
        }
    }

    entries.sort_by_key(|e| std::cmp::Reverse(e.complexity));
    let high_risk_count = entries
        .iter()
        .filter(|e| e.risk == "high" || e.risk == "critical")
        .count();
    entries.truncate(limit);

    let json = serde_json::to_string(&entries).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadComplexityMapResult {
        entries,
        total_analyzed,
        high_risk_count,
        token_count,
    })
}

fn risk_label(c: u32) -> String {
    match c {
        1..=5 => "low",
        6..=10 => "medium",
        11..=20 => "high",
        _ => "critical",
    }
    .to_string()
}

/// Approximate cyclomatic complexity via keyword counting.
/// Starts at 1 and adds +1 per decision point.
fn compute_complexity(body: &str, _ext: &str) -> u32 {
    let mut count: u32 = 1;
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with("//") || t.starts_with('#') || t.starts_with('*') || t.starts_with("/*") {
            continue;
        }
        // Branch keywords
        count += kw_count(t, " if ") as u32;
        count += kw_count(t, "\tif ") as u32;
        count += kw_count(t, "(if ") as u32;
        count += kw_count(t, "} else if") as u32;
        count += kw_count(t, "else if(") as u32;
        count += kw_count(t, " elif ") as u32;
        count += kw_count(t, " elif(") as u32;
        count += kw_count(t, "for ") as u32;
        count += kw_count(t, "for(") as u32;
        count += kw_count(t, "while ") as u32;
        count += kw_count(t, "while(") as u32;
        count += kw_count(t, "loop {") as u32;
        count += kw_count(t, "match ") as u32;
        count += kw_count(t, " case ") as u32;
        count += kw_count(t, "catch ") as u32;
        count += kw_count(t, "catch(") as u32;
        count += kw_count(t, "except ") as u32;
        count += kw_count(t, "except:") as u32;
        // Logical operators
        count += kw_count(t, " && ") as u32;
        count += kw_count(t, " || ") as u32;
        // Ternary
        count += kw_count(t, " ? ") as u32;
        // Rust match arms
        count += kw_count(t, " => ") as u32;
    }
    count
}

fn kw_count(s: &str, kw: &str) -> usize {
    let mut n = 0;
    let mut pos = 0;
    while let Some(i) = s[pos..].find(kw) {
        n += 1;
        pos += i + kw.len();
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: with a `path` param the walk root used to come back
    /// canonicalized (`\\?\` verbatim on Windows), every rel path failed to
    /// strip, the skeleton re-read failed, and the result was silently empty.
    #[test]
    fn scoped_scan_analyzes_files() {
        let params = ReadComplexityMapParams {
            path: Some("src".to_string()),
            min_complexity: None,
            limit: None,
        };
        let result = read_complexity_map(std::path::Path::new("."), params).unwrap();
        assert!(
            result.total_analyzed > 0,
            "scoped scan must analyze functions"
        );
        assert!(result.entries.iter().all(|e| e.path.starts_with("src/")));
    }
}
