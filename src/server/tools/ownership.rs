//! read_code_ownership — fuses `git log` churn with per-author line contribution
//! to answer "why is this code like this / who should I ask". One `git log
//! --numstat` pass yields, per file: commit count (churn), top authors by lines
//! added (ownership share), and the date it was last touched.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use super::fs::estimate_tokens;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadCodeOwnershipParams {
    #[schemars(description = "Restrict to this file or directory (root-relative). Omit for the whole repo.")]
    pub path: Option<String>,
    #[schemars(description = "Number of hotspot files to return, sorted by churn (default 20).")]
    pub top_n: Option<usize>,
    #[schemars(description = "Only consider history since this point, e.g. \"3 months ago\" or \"2025-01-01\".")]
    pub since: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OwnerShare {
    pub author: String,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct Hotspot {
    pub path: String,
    pub commits: usize,
    pub last_modified: String,
    pub primary_owner: String,
    pub owners: Vec<OwnerShare>,
}

#[derive(Debug, Serialize)]
pub struct ReadCodeOwnershipResult {
    pub hotspots: Vec<Hotspot>,
    pub token_count: usize,
}

#[derive(Default)]
struct FileStat {
    commits: usize,
    last_modified: String,
    /// author -> lines added
    author_lines: HashMap<String, u64>,
}

// Record/unit separators keep the format unambiguous vs author names containing '|'.
const RS: char = '\u{1e}';
const US: char = '\u{1f}';

pub fn read_code_ownership(
    root: &Path,
    params: ReadCodeOwnershipParams,
) -> Result<ReadCodeOwnershipResult, String> {
    let top_n = params.top_n.unwrap_or(20).clamp(1, 500);

    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.args([
        "log",
        "--no-merges",
        &format!("--pretty=format:{RS}%an{US}%aI"),
        "--numstat",
        "--date=short",
    ]);
    if let Some(s) = &params.since {
        cmd.arg(format!("--since={s}"));
    }
    if let Some(p) = &params.path {
        cmd.arg("--").arg(p);
    }

    let output = cmd.output().map_err(|e| format!("git コマンド実行失敗: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git log 失敗: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);

    let stats = aggregate(&text);
    let mut hotspots = rank(stats, top_n);

    // Stable, useful ordering: churn desc, then path asc (already applied in rank).
    let json = serde_json::to_string(&hotspots).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    // hotspots is owned; return it.
    hotspots.shrink_to_fit();
    Ok(ReadCodeOwnershipResult { hotspots, token_count })
}

/// Parse `git log --numstat` output into per-file stats.
fn aggregate(text: &str) -> HashMap<String, FileStat> {
    let mut stats: HashMap<String, FileStat> = HashMap::new();
    let mut cur_author = String::new();
    let mut cur_date = String::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(RS) {
            // commit header: <author>US<iso-date>
            if let Some((author, date)) = rest.split_once(US) {
                cur_author = author.to_string();
                cur_date = date.to_string();
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        // numstat line: <added>\t<deleted>\t<path>  (added/deleted may be "-" for binary)
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(_deleted), Some(path)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let added: u64 = added.parse().unwrap_or(0);
        let entry = stats.entry(path.to_string()).or_default();
        entry.commits += 1;
        // log is newest-first, so the first date seen is the most recent.
        if entry.last_modified.is_empty() {
            entry.last_modified = cur_date.clone();
        }
        *entry.author_lines.entry(cur_author.clone()).or_insert(0) += added;
    }
    stats
}

/// Rank files by churn and compute author shares.
fn rank(stats: HashMap<String, FileStat>, top_n: usize) -> Vec<Hotspot> {
    let mut entries: Vec<(String, FileStat)> = stats.into_iter().collect();
    entries.sort_by(|a, b| b.1.commits.cmp(&a.1.commits).then(a.0.cmp(&b.0)));
    entries.truncate(top_n);

    entries
        .into_iter()
        .map(|(path, st)| {
            let total_lines: u64 = st.author_lines.values().sum();
            let mut owners: Vec<OwnerShare> = st
                .author_lines
                .iter()
                .map(|(author, &lines)| OwnerShare {
                    author: author.clone(),
                    pct: if total_lines == 0 {
                        0.0
                    } else {
                        (lines as f64 / total_lines as f64 * 1000.0).round() / 10.0
                    },
                })
                .collect();
            // Highest share first; tie-break by name for determinism.
            owners.sort_by(|a, b| {
                b.pct
                    .partial_cmp(&a.pct)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.author.cmp(&b.author))
            });
            let primary_owner = owners
                .first()
                .map(|o| o.author.clone())
                .unwrap_or_default();
            owners.truncate(5);
            Hotspot {
                path,
                commits: st.commits,
                last_modified: st.last_modified,
                primary_owner,
                owners,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_churn_owners_and_recency() {
        // Two commits, newest first (alice), then bob.
        let text = format!(
            "{RS}Alice{US}2026-06-10\n10\t0\tsrc/a.rs\n5\t1\tsrc/b.rs\n\n{RS}Bob{US}2026-01-01\n2\t0\tsrc/a.rs\n"
        );
        let stats = aggregate(&text);
        let a = &stats["src/a.rs"];
        assert_eq!(a.commits, 2);
        assert_eq!(a.last_modified, "2026-06-10"); // newest wins
        assert_eq!(a.author_lines["Alice"], 10);
        assert_eq!(a.author_lines["Bob"], 2);

        let hotspots = rank(stats, 20);
        // a.rs has more churn than b.rs → first.
        assert_eq!(hotspots[0].path, "src/a.rs");
        assert_eq!(hotspots[0].primary_owner, "Alice");
        let alice = hotspots[0].owners.iter().find(|o| o.author == "Alice").unwrap();
        assert!((alice.pct - 83.3).abs() < 0.5); // 10/12
    }

    #[test]
    fn binary_numstat_dashes_do_not_panic() {
        let text = format!("{RS}Alice{US}2026-06-10\n-\t-\tassets/logo.png\n");
        let stats = aggregate(&text);
        let s = &stats["assets/logo.png"];
        assert_eq!(s.commits, 1);
        assert_eq!(s.author_lines["Alice"], 0);
    }

    #[test]
    fn top_n_limits_results() {
        let mut text = String::new();
        for i in 0..10 {
            text.push_str(&format!("{RS}Dev{US}2026-06-10\n1\t0\tf{i}.rs\n"));
        }
        let stats = aggregate(&text);
        let hotspots = rank(stats, 3);
        assert_eq!(hotspots.len(), 3);
    }
}
