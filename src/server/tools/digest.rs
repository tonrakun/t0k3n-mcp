//! `project_digest` — warm-start architecture summary.
//!
//! Every session tends to repeat the same exploration: directory tree → workspace
//! stats → skeletons of the entry points. This caches that into a single ~2k-token
//! digest keyed by the current git HEAD, so a fresh session can orient in one call.
//! The cache (`.t0k3n/digest.json`) is invalidated automatically when HEAD changes;
//! pass `refresh: true` to rebuild regardless (e.g. on a dirty working tree).

use std::path::Path;
use std::process::Command;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::code::{ReadCodeSkeletonParams, read_code_skeleton};
use super::fs::{ReadDirectoryTreeParams, estimate_tokens, read_directory_tree};
use super::stats::{ReadWorkspaceStatsParams, read_workspace_stats};

const DEFAULT_BUDGET: usize = 2000;
const MAX_ENTRY_POINTS: usize = 8;
const MAX_ENTRY_SYMBOLS: usize = 6;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ProjectDigestParams {
    /// Rebuild the digest even if a cache for the current git HEAD exists.
    pub refresh: Option<bool>,
    /// Approximate token budget for the digest (default 2000).
    pub budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangLine {
    pub language: String,
    pub files: usize,
    pub lines: usize,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoint {
    pub path: String,
    pub language: String,
    pub tokens: usize,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDigest {
    pub git_head: String,
    pub total_files: usize,
    pub total_lines: usize,
    pub total_tokens: usize,
    pub by_language: Vec<LangLine>,
    pub entry_points: Vec<EntryPoint>,
    pub directory_tree: String,
}

#[derive(Debug, Serialize)]
pub struct ProjectDigestResult {
    pub cached: bool,
    pub dirty: bool,
    pub digest: ProjectDigest,
    pub token_count: usize,
}

/// On-disk cache envelope.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    git_head: String,
    digest: ProjectDigest,
}

pub fn project_digest(
    root: &Path,
    params: ProjectDigestParams,
) -> anyhow::Result<ProjectDigestResult> {
    let refresh = params.refresh.unwrap_or(false);
    let budget = params.budget.unwrap_or(DEFAULT_BUDGET).max(500);
    let head = git_head(root).unwrap_or_else(|| "no-git".into());
    let dirty = git_dirty(root);
    let cache_path = root.join(".t0k3n").join("digest.json");

    if !refresh
        && head != "no-git"
        && let Some(env) = load_cache(&cache_path)
        && env.git_head == head
    {
        return Ok(finish(env.digest, true, dirty));
    }

    let digest = build_digest(root, head.clone(), budget)?;
    let _ = save_cache(&cache_path, &head, &digest);
    Ok(finish(digest, false, dirty))
}

fn finish(digest: ProjectDigest, cached: bool, dirty: bool) -> ProjectDigestResult {
    let token_count = estimate_tokens(&serde_json::to_string(&digest).unwrap_or_default());
    ProjectDigestResult {
        cached,
        dirty,
        digest,
        token_count,
    }
}

fn build_digest(root: &Path, head: String, budget: usize) -> anyhow::Result<ProjectDigest> {
    let stats = read_workspace_stats(root, ReadWorkspaceStatsParams { glob: None })?;
    let by_language: Vec<LangLine> = stats
        .by_language
        .into_iter()
        .take(8)
        .map(|l| LangLine {
            language: l.language,
            files: l.files,
            lines: l.lines,
            pct: l.pct,
        })
        .collect();

    let entry_points = collect_entry_points(root);

    // A shallow tree is enough to orient; trim if it blows the budget.
    let tree = read_directory_tree(
        root,
        ReadDirectoryTreeParams {
            path: None,
            depth: Some(2),
        },
    )
    .map(|t| t.tree)
    .unwrap_or_default();
    let directory_tree = trim_to_budget(&tree, budget / 3);

    Ok(ProjectDigest {
        git_head: head,
        total_files: stats.total_files,
        total_lines: stats.total_lines,
        total_tokens: stats.total_tokens,
        by_language,
        entry_points,
        directory_tree,
    })
}

/// Rank candidate files by how likely they are to be an architectural entry
/// point (conventional name + shallow depth), then return the top few with their
/// top-level symbol signatures.
fn collect_entry_points(root: &Path) -> Vec<EntryPoint> {
    let mut scored: Vec<(i32, String)> = Vec::new();

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !is_code_ext(ext) {
            continue;
        }
        let rel = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let depth = rel.matches('/').count() as i32;

        let mut score = name_score(stem);
        if score == 0 {
            continue; // only surface conventional entry points
        }
        score -= depth; // prefer shallow files
        scored.push((score, rel));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.truncate(MAX_ENTRY_POINTS);

    scored
        .into_iter()
        .filter_map(|(_, rel)| entry_point_for(root, &rel))
        .collect()
}

fn entry_point_for(root: &Path, rel: &str) -> Option<EntryPoint> {
    let content = std::fs::read_to_string(root.join(rel)).ok()?;
    let tokens = estimate_tokens(&content);
    let skel = read_code_skeleton(
        root,
        ReadCodeSkeletonParams {
            path: rel.to_string(),
            include_blocks: Some(false),
        },
    )
    .ok()?;
    let symbols: Vec<String> = skel
        .skeleton
        .iter()
        .take(MAX_ENTRY_SYMBOLS)
        .map(|s| s.signature.clone())
        .collect();
    Some(EntryPoint {
        path: rel.to_string(),
        language: skel.language,
        tokens,
        symbols,
    })
}

/// Higher is more likely to be an entry point. 0 = not a conventional name.
fn name_score(stem: &str) -> i32 {
    match stem.to_lowercase().as_str() {
        "main" => 10,
        "lib" | "index" => 9,
        "app" | "server" | "application" => 8,
        "mod" | "__init__" => 6,
        "cli" | "router" | "routes" | "handler" | "handlers" => 5,
        "config" | "settings" => 4,
        _ => 0,
    }
}

fn is_code_ext(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "go"
            | "java"
            | "kt"
            | "rb"
            | "cs"
            | "php"
            | "swift"
            | "cpp"
            | "cc"
            | "c"
    )
}

fn trim_to_budget(text: &str, token_budget: usize) -> String {
    if estimate_tokens(text) <= token_budget {
        return text.to_string();
    }
    // Roughly 4 chars per token; keep a proportional prefix.
    let char_budget = token_budget.saturating_mul(4);
    let mut out: String = text.chars().take(char_budget).collect();
    out.push_str("\n… (truncated)");
    out
}

// ─── git helpers ─────────────────────────────────────────────────────────────

pub fn git_head(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn git_dirty(root: &Path) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

// ─── cache I/O ───────────────────────────────────────────────────────────────

fn load_cache(path: &Path) -> Option<CacheEnvelope> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_cache(path: &Path, head: &str, digest: &ProjectDigest) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let env = CacheEnvelope {
        git_head: head.to_string(),
        digest: digest.clone(),
    };
    let json = serde_json::to_string(&env).unwrap_or_default();
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_score_prefers_main_then_lib() {
        assert!(name_score("main") > name_score("lib"));
        assert!(name_score("lib") > name_score("mod"));
        assert_eq!(name_score("random_helper"), 0);
    }

    #[test]
    fn trim_keeps_short_text_intact() {
        let s = "small tree";
        assert_eq!(trim_to_budget(s, 100), s);
    }

    #[test]
    fn trim_truncates_long_text() {
        let s = "x".repeat(10_000);
        let out = trim_to_budget(&s, 50);
        assert!(out.len() < s.len());
        assert!(out.ends_with("(truncated)"));
    }

    #[test]
    fn digest_builds_and_caches_for_a_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\nfn helper(x: i32) -> i32 { x + 1 }\n",
        )
        .unwrap();

        // no git here → head is "no-git", so it always rebuilds (cached:false)
        let r = project_digest(
            root,
            ProjectDigestParams {
                refresh: None,
                budget: None,
            },
        )
        .unwrap();
        assert!(!r.cached);
        assert_eq!(r.digest.git_head, "no-git");
        assert!(r.digest.total_files >= 1);
        let ep = r
            .digest
            .entry_points
            .iter()
            .find(|e| e.path == "src/main.rs");
        assert!(ep.is_some(), "main.rs should be an entry point");
        assert!(r.token_count > 0);
    }
}
