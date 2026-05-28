use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use chrono::{TimeZone, Utc};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadGitDiffParams {
    #[schemars(description = "Base ref to diff against (default: HEAD). Examples: HEAD, main, abc1234")]
    pub base: Option<String>,
    #[schemars(description = "Limit diff to this file path (relative to workspace root). Omit for all changes.")]
    pub path: Option<String>,
    #[schemars(description = "Return only --stat summary instead of full diff (default: false)")]
    pub stat_only: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadGitDiffResult {
    pub diff: String,
    pub token_count: usize,
}

pub fn read_git_diff(root: &Path, params: ReadGitDiffParams) -> Result<ReadGitDiffResult, String> {
    let base = params.base.as_deref().unwrap_or("HEAD");
    let stat_only = params.stat_only.unwrap_or(false);

    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.arg("diff");

    if stat_only {
        cmd.arg("--stat");
    } else {
        // Reduce context lines to keep output compact
        cmd.arg("--unified=2");
    }

    cmd.arg(base);

    if let Some(ref p) = params.path {
        cmd.arg("--").arg(p);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("git コマンド実行失敗: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff 失敗: {stderr}"));
    }

    let diff = String::from_utf8_lossy(&output.stdout).into_owned();
    let token_count = estimate_tokens(&diff);

    Ok(ReadGitDiffResult { diff, token_count })
}

// ─── read_git_log ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadGitLogParams {
    #[schemars(description = "Limit log to this file or directory path (root-relative). Omit for all commits.")]
    pub path: Option<String>,
    #[schemars(description = "Filter by author name or email (substring match)")]
    pub author: Option<String>,
    #[schemars(description = "Show commits newer than this date, e.g. '2024-01-01' or '2 weeks ago'")]
    pub since: Option<String>,
    #[schemars(description = "Show commits older than this date")]
    pub until: Option<String>,
    #[schemars(description = "Maximum number of commits to return (default: 20, max: 100)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitLogEntry {
    pub sha: String,
    pub sha_short: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadGitLogResult {
    pub entries: Vec<GitLogEntry>,
    pub token_count: usize,
}

pub fn read_git_log(root: &Path, params: ReadGitLogParams) -> Result<ReadGitLogResult, String> {
    let limit = params.limit.unwrap_or(20).min(100);

    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.args(["log", "--format=COMMIT:%H|%ae|%ad|%s", "--date=short", "--name-only", &format!("-n{}", limit)]);

    if let Some(ref a) = params.author { cmd.arg(format!("--author={a}")); }
    if let Some(ref s) = params.since  { cmd.arg(format!("--since={s}")); }
    if let Some(ref u) = params.until  { cmd.arg(format!("--until={u}")); }
    if let Some(ref p) = params.path   { cmd.arg("--").arg(p); }

    let output = cmd.output().map_err(|e| format!("git コマンド実行失敗: {e}"))?;
    if !output.status.success() {
        return Err(format!("git log 失敗: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<GitLogEntry> = Vec::new();
    let mut cur_header: Option<(String, String, String, String)> = None;
    let mut cur_files: Vec<String> = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("COMMIT:") {
            if let Some((sha, author, date, msg)) = cur_header.take() {
                entries.push(GitLogEntry {
                    sha_short: sha[..sha.len().min(7)].to_string(),
                    sha, author, date, message: msg,
                    files: std::mem::take(&mut cur_files),
                });
            }
            let p: Vec<&str> = rest.splitn(4, '|').collect();
            if p.len() == 4 {
                cur_header = Some((p[0].to_string(), p[1].to_string(), p[2].to_string(), p[3].to_string()));
            }
        } else if !line.trim().is_empty() {
            cur_files.push(line.to_string());
        }
    }
    if let Some((sha, author, date, msg)) = cur_header {
        entries.push(GitLogEntry {
            sha_short: sha[..sha.len().min(7)].to_string(),
            sha, author, date, message: msg, files: cur_files,
        });
    }

    let json = serde_json::to_string(&entries).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadGitLogResult { entries, token_count })
}

// ─── read_git_blame_body ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadGitBlameBodyParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Start line number (1-based, use start_line from read_code_skeleton)")]
    pub start_line: u32,
    #[schemars(description = "End line number (1-based, use end_line from read_code_skeleton)")]
    pub end_line: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlameLineEntry {
    pub line_no: u32,
    pub sha_short: String,
    pub author: String,
    pub date: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadGitBlameBodyResult {
    pub path: String,
    pub lines: Vec<BlameLineEntry>,
    pub token_count: usize,
}

pub fn read_git_blame_body(root: &Path, params: ReadGitBlameBodyParams) -> Result<ReadGitBlameBodyResult, String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.args(["blame", &format!("-L{},{}", params.start_line, params.end_line), "--porcelain", "--", &params.path]);

    let output = cmd.output().map_err(|e| format!("git コマンド実行失敗: {e}"))?;
    if !output.status.success() {
        return Err(format!("git blame 失敗: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let lines = parse_porcelain_blame(&text);
    let json = serde_json::to_string(&lines).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadGitBlameBodyResult { path: params.path, lines, token_count })
}

fn parse_porcelain_blame(output: &str) -> Vec<BlameLineEntry> {
    use std::collections::HashMap;
    let mut sha_cache: HashMap<String, (String, String)> = HashMap::new();
    let mut entries = Vec::new();

    let mut cur_sha = String::new();
    let mut cur_line_no: u32 = 0;
    let mut pending_author = String::new();
    let mut pending_time: i64 = 0;

    for line in output.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            let (author, date) = sha_cache.get(&cur_sha)
                .cloned()
                .unwrap_or_else(|| (pending_author.clone(), format_unix_date(pending_time)));
            entries.push(BlameLineEntry {
                line_no: cur_line_no,
                sha_short: cur_sha[..cur_sha.len().min(7)].to_string(),
                author, date,
                content: content.to_string(),
            });
        } else if line.starts_with("author ") && !line.starts_with("author-") {
            pending_author = line["author ".len()..].to_string();
        } else if let Some(rest) = line.strip_prefix("author-time ") {
            if let Ok(ts) = rest.parse::<i64>() {
                pending_time = ts;
                sha_cache.insert(cur_sha.clone(), (pending_author.clone(), format_unix_date(ts)));
            }
        } else {
            let mut iter = line.splitn(4, ' ');
            if let (Some(sha), Some(_orig), Some(final_line)) = (iter.next(), iter.next(), iter.next()) {
                if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    cur_sha = sha.to_string();
                    cur_line_no = final_line.parse().unwrap_or(0);
                    if let Some((a, _)) = sha_cache.get(&cur_sha) {
                        pending_author = a.clone();
                    }
                }
            }
        }
    }
    entries
}

fn format_unix_date(ts: i64) -> String {
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

// ─── read_changed_files ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadChangedFilesParams {
    #[schemars(description = "Base ref to diff against (default: HEAD). Examples: HEAD, main, abc1234.")]
    pub base: Option<String>,
    #[schemars(description = "Include untracked files in addition to modified/staged files (default: false).")]
    pub include_untracked: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ChangedFileEntry {
    pub path: String,
    pub status: String,  // "modified" | "added" | "deleted" | "renamed" | "untracked"
    pub added: usize,
    pub deleted: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadChangedFilesResult {
    pub base: String,
    pub files: Vec<ChangedFileEntry>,
    pub total_added: usize,
    pub total_deleted: usize,
    pub file_count: usize,
    pub token_count: usize,
}

pub fn read_changed_files(root: &Path, params: ReadChangedFilesParams) -> Result<ReadChangedFilesResult, String> {
    let base = params.base.as_deref().unwrap_or("HEAD").to_string();
    let include_untracked = params.include_untracked.unwrap_or(false);

    // git diff --numstat <base> to get per-file added/deleted counts
    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.args(["diff", "--numstat", &base]);
    let output = cmd.output().map_err(|e| format!("git コマンド実行失敗: {e}"))?;
    if !output.status.success() {
        return Err(format!("git diff 失敗: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let diff_text = String::from_utf8_lossy(&output.stdout);

    // git diff --name-status <base> to get status (M/A/D/R)
    let mut cmd2 = Command::new("git");
    cmd2.current_dir(root);
    cmd2.args(["diff", "--name-status", &base]);
    let output2 = cmd2.output().map_err(|e| format!("git コマンド実行失敗: {e}"))?;
    let status_text = String::from_utf8_lossy(&output2.stdout);

    // Build status map: path → status
    let mut status_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in status_text.lines() {
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            let s = match parts[0].chars().next().unwrap_or('M') {
                'A' => "added",
                'D' => "deleted",
                'R' => "renamed",
                _ => "modified",
            };
            // For renames: "R100\told\tnew" — take the new path
            let path = if parts[0].starts_with('R') {
                parts[1].splitn(2, '\t').last().unwrap_or(parts[1])
            } else {
                parts[1]
            };
            status_map.insert(path.to_string(), s.to_string());
        }
    }

    // Parse numstat: added\tdeleted\tpath
    let mut files: Vec<ChangedFileEntry> = Vec::new();
    let mut total_added = 0usize;
    let mut total_deleted = 0usize;

    for line in diff_text.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() == 3 {
            let added: usize   = parts[0].parse().unwrap_or(0);
            let deleted: usize = parts[1].parse().unwrap_or(0);
            let path = parts[2].to_string();
            let status = status_map.get(&path).cloned().unwrap_or_else(|| "modified".to_string());
            total_added += added;
            total_deleted += deleted;
            files.push(ChangedFileEntry { path, status, added, deleted });
        }
    }

    // Optionally include untracked
    if include_untracked {
        let mut cmd3 = Command::new("git");
        cmd3.current_dir(root);
        cmd3.args(["ls-files", "--others", "--exclude-standard"]);
        if let Ok(out3) = cmd3.output() {
            for line in String::from_utf8_lossy(&out3.stdout).lines() {
                if !line.is_empty() {
                    files.push(ChangedFileEntry {
                        path: line.to_string(),
                        status: "untracked".to_string(),
                        added: 0,
                        deleted: 0,
                    });
                }
            }
        }
    }

    // Sort: deleted files last, then by path
    files.sort_by(|a, b| {
        let a_del = (a.status == "deleted") as u8;
        let b_del = (b.status == "deleted") as u8;
        a_del.cmp(&b_del).then_with(|| a.path.cmp(&b.path))
    });

    let file_count = files.len();
    let json = serde_json::to_string(&files).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadChangedFilesResult { base, files, total_added, total_deleted, file_count, token_count })
}
