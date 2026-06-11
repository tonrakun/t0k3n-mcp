use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::code::{ReadCodeSkeletonParams, read_code_skeleton};
use super::fs::estimate_tokens;
use crate::security::rel_display;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadPrContextParams {
    #[schemars(description = "Feature branch name to analyze (e.g. feature/auth-refactor)")]
    pub branch: String,
    #[schemars(description = "Base branch to compare against (default: main)")]
    pub base: Option<String>,
    #[schemars(description = "Maximum files to include skeletons for (default: 10)")]
    pub max_files: Option<usize>,
    #[schemars(description = "Include code skeletons for changed files (default: true)")]
    pub include_skeletons: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ChangedFileSummary {
    pub path: String,
    pub status: String,  // "M" | "A" | "D" | "R"
    pub added_lines: i64,
    pub deleted_lines: i64,
    pub skeleton: Option<Vec<SkeletonSummary>>,
}

#[derive(Debug, Serialize)]
pub struct SkeletonSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize)]
pub struct RelatedTestFile {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct CommitSummary {
    pub sha_short: String,
    pub message: String,
    pub author: String,
}

#[derive(Debug, Serialize)]
pub struct ReadPrContextResult {
    pub branch: String,
    pub base: String,
    pub changed_files: Vec<ChangedFileSummary>,
    pub total_files: usize,
    pub total_added: i64,
    pub total_deleted: i64,
    pub related_tests: Vec<RelatedTestFile>,
    pub commits: Vec<CommitSummary>,
    pub token_count: usize,
}

pub fn read_pr_context(root: &Path, params: ReadPrContextParams) -> anyhow::Result<ReadPrContextResult> {
    let base = params.base.as_deref().unwrap_or("main").to_string();
    let max_files = params.max_files.unwrap_or(10);
    let include_skeletons = params.include_skeletons.unwrap_or(true);
    let branch = params.branch.clone();

    // 1. Get changed files: git diff --name-status base...branch
    let diff_range = format!("{}...{}", base, branch);
    let numstat_out = Command::new("git")
        .args(["diff", "--numstat", &diff_range])
        .current_dir(root)
        .output()
        .map_err(|e| anyhow::anyhow!("git diff failed: {e}"))?;

    let numstat = String::from_utf8_lossy(&numstat_out.stdout);

    let status_str_owned = Command::new("git")
        .args(["diff", "--name-status", &diff_range])
        .current_dir(root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let status_str = status_str_owned.as_str();

    // Parse status map: path -> status
    let mut status_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in status_str.lines() {
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            let status = parts[0].trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
            let path = parts[1].trim().replace('\\', "/");
            status_map.insert(path, status);
        }
    }

    // Parse numstat
    let mut all_files: Vec<(String, i64, i64)> = Vec::new();
    for line in numstat.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() == 3 {
            let added: i64 = parts[0].trim().parse().unwrap_or(0);
            let deleted: i64 = parts[1].trim().parse().unwrap_or(0);
            let path = parts[2].trim().replace('\\', "/");
            all_files.push((path, added, deleted));
        }
    }

    let total_files = all_files.len();
    let total_added: i64 = all_files.iter().map(|(_, a, _)| *a).sum();
    let total_deleted: i64 = all_files.iter().map(|(_, _, d)| *d).sum();

    // Sort by (|added + deleted|) desc to prioritize most-changed files
    all_files.sort_by(|(_, a1, d1), (_, a2, d2)| (a2 + d2).cmp(&(a1 + d1)));

    let files_for_skeleton = all_files.iter().take(max_files);

    let mut changed_files: Vec<ChangedFileSummary> = Vec::new();
    let mut related_test_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (path, added, deleted) in files_for_skeleton {
        let status = status_map.get(path).cloned().unwrap_or_else(|| "M".to_string());

        let skeleton = if include_skeletons && status != "D" {
            let sk_params = ReadCodeSkeletonParams {
                path: path.clone(),
                include_blocks: Some(false),
            };
            read_code_skeleton(root, sk_params)
                .ok()
                .map(|r| r.skeleton.into_iter().map(|s| SkeletonSummary {
                    id: s.id,
                    name: s.name,
                    kind: s.kind,
                    start_line: s.start_line,
                    end_line: s.end_line,
                }).collect())
        } else {
            None
        };

        // Look for related tests
        let stem = std::path::Path::new(path.as_str())
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if !stem.is_empty() {
            find_related_tests(root, &stem, path, &mut related_test_set);
        }

        changed_files.push(ChangedFileSummary {
            path: path.clone(),
            status,
            added_lines: *added,
            deleted_lines: *deleted,
            skeleton,
        });
    }

    // 2. Get commits on branch not in base
    let log_str_owned = Command::new("git")
        .args([
            "log",
            "--oneline",
            "--format=%h\t%an\t%s",
            &diff_range,
        ])
        .current_dir(root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let log_str = log_str_owned.as_str();

    let commits: Vec<CommitSummary> = log_str
        .lines()
        .take(20)
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() == 3 {
                Some(CommitSummary {
                    sha_short: parts[0].to_string(),
                    author: parts[1].to_string(),
                    message: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    let related_tests: Vec<RelatedTestFile> = related_test_set
        .into_iter()
        .map(|path| RelatedTestFile { path: path.clone(), reason: "filename match".to_string() })
        .collect();

    let json = serde_json::json!({
        "branch": branch,
        "base": base,
        "changed_files": changed_files,
        "commits": commits,
    });
    let token_count = estimate_tokens(&json.to_string());

    Ok(ReadPrContextResult {
        branch,
        base,
        changed_files,
        total_files,
        total_added,
        total_deleted,
        related_tests,
        commits,
        token_count,
    })
}

fn find_related_tests(
    root: &Path,
    stem: &str,
    source_path: &str,
    found: &mut std::collections::HashSet<String>,
) {
    let test_patterns = [
        format!("{}.test.", stem),
        format!("{}.spec.", stem),
        format!("{}_test.", stem),
        format!("test_{}", stem),
        format!("{}Test.", stem),
        format!("{}Spec.", stem),
    ];

    let test_dirs = ["tests", "test", "__tests__", "spec", "__spec__"];

    // Walk the workspace looking for test files matching the stem
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let rel = rel_display(root, path);

        if rel == source_path {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        let in_test_dir = test_dirs.iter().any(|d| rel.contains(&format!("/{}/", d)) || rel.starts_with(&format!("{}/", d)));

        let matches_pattern = test_patterns.iter().any(|p| file_name.contains(p.to_lowercase().as_str()));

        if matches_pattern || in_test_dir && file_name.contains(&stem.to_lowercase()) {
            found.insert(rel);
        }
    }
}
