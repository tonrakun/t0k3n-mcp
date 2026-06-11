//! read_context_pack — task-driven bulk context collection.
//!
//! One call replaces the explore phase (tree → search → skeleton → body
//! round trips, each of which re-sends the whole conversation). Files and
//! symbols are ranked by lexical relevance to the query, then a pack is
//! greedily filled up to the token budget: file ranking first, then
//! relevant signatures, then the highest-scoring symbol bodies.

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::security::{rel_display, safe_path, scoped_root};
use super::code::{ReadCodeSkeletonParams, read_code_skeleton};
use super::fs::estimate_tokens;

const DEFAULT_BUDGET: usize = 5000;
const DEFAULT_MAX_FILES: usize = 8;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go",
    "cpp", "cc", "cxx", "hpp", "hh", "h", "c", "java", "kt", "rb",
    "cs", "php", "swift", "lua",
];

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "when",
    "should", "would", "could", "have", "has", "are", "was", "were", "not",
    "fix", "add", "implement", "make", "update", "change", "refactor",
    "function", "method", "file", "code", "bug", "error", "issue", "where",
    "how", "why", "what", "all", "any", "new", "old", "use", "using",
];

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadContextPackParams {
    #[schemars(description = "Task description. English code-related keywords work best (identifiers, file names, concepts).")]
    pub query: String,
    #[schemars(description = "Token budget for the pack (default: 5000)")]
    pub budget: Option<usize>,
    #[schemars(description = "Restrict to this directory (root-relative). Omit for whole workspace.")]
    pub path: Option<String>,
    #[schemars(description = "Max files to consider for the pack (default: 8)")]
    pub max_files: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct PackFile {
    pub path: String,
    pub score: usize,
    pub language: String,
}

#[derive(Debug, Serialize)]
pub struct PackSymbol {
    pub path: String,
    pub id: String,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub score: usize,
}

#[derive(Debug, Serialize)]
pub struct PackBody {
    pub path: String,
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ReadContextPackResult {
    pub keywords: Vec<String>,
    pub files: Vec<PackFile>,
    pub symbols: Vec<PackSymbol>,
    pub bodies: Vec<PackBody>,
    pub bodies_omitted_for_budget: usize,
    pub budget: usize,
    pub token_count: usize,
}

fn extract_keywords(query: &str) -> Vec<String> {
    let mut kws: Vec<String> = Vec::new();
    for token in query.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        let t = token.trim().to_lowercase();
        if t.len() < 3 || STOPWORDS.contains(&t.as_str()) || kws.contains(&t) {
            continue;
        }
        // split snake_case compounds into parts as well
        if t.contains('_') {
            for part in t.split('_') {
                let p = part.to_string();
                if p.len() >= 3 && !STOPWORDS.contains(&p.as_str()) && !kws.contains(&p) {
                    kws.push(p);
                }
            }
        }
        kws.push(t);
    }
    kws
}

fn score_text(text_lower: &str, keywords: &[String], per_kw_cap: usize) -> usize {
    keywords
        .iter()
        .map(|kw| text_lower.matches(kw.as_str()).count().min(per_kw_cap))
        .sum()
}

pub fn read_context_pack(root: &Path, params: ReadContextPackParams) -> anyhow::Result<ReadContextPackResult> {
    let budget = params.budget.unwrap_or(DEFAULT_BUDGET).max(500);
    let max_files = params.max_files.unwrap_or(DEFAULT_MAX_FILES).clamp(1, 30);
    let keywords = extract_keywords(&params.query);
    if keywords.is_empty() {
        anyhow::bail!("no usable keywords in query — include identifiers, file names or concepts (min 3 chars)");
    }

    let scope = scoped_root(root, params.path.as_deref())?;

    // 1. rank files by path + content keyword hits
    let mut scored_files: Vec<(String, usize)> = Vec::new();
    for entry in WalkBuilder::new(&scope)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !CODE_EXTENSIONS.contains(&ext) {
            continue;
        }
        if entry.metadata().map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(true) {
            continue;
        }
        let rel = rel_display(root, p);
        let path_score = score_text(&rel.to_lowercase(), &keywords, 1) * 5;
        let content_score = std::fs::read_to_string(p)
            .map(|c| score_text(&c.to_lowercase(), &keywords, 20))
            .unwrap_or(0);
        let score = path_score + content_score;
        if score > 0 {
            scored_files.push((rel, score));
        }
    }
    scored_files.sort_by_key(|f| std::cmp::Reverse(f.1));
    scored_files.truncate(max_files);

    // 2. skeletons for the top files, score symbols
    let mut files = Vec::new();
    let mut symbols: Vec<PackSymbol> = Vec::new();
    for (rel, score) in &scored_files {
        let Ok(sk) = read_code_skeleton(root, ReadCodeSkeletonParams { path: rel.clone(), include_blocks: Some(false) }) else {
            continue;
        };
        files.push(PackFile { path: rel.clone(), score: *score, language: sk.language });
        for item in sk.skeleton {
            let name_score = score_text(&item.name.to_lowercase(), &keywords, 1) * 10;
            let sig_score = score_text(&item.signature.to_lowercase(), &keywords, 3) * 3;
            let s = name_score + sig_score;
            if s > 0 {
                symbols.push(PackSymbol {
                    path: rel.clone(),
                    id: item.id,
                    kind: item.kind,
                    name: item.name,
                    signature: item.signature,
                    score: s,
                });
            }
        }
    }
    symbols.sort_by_key(|s| std::cmp::Reverse(s.score));

    // 3. greedy fill: ranking + signatures are always in; bodies until budget
    let mut used = estimate_tokens(&serde_json::to_string(&files).unwrap_or_default())
        + estimate_tokens(&serde_json::to_string(&symbols).unwrap_or_default());
    let mut bodies = Vec::new();
    let mut omitted = 0usize;
    for sym in &symbols {
        let Ok(abs) = safe_path(root, &sym.path) else { continue };
        let Ok(content) = std::fs::read_to_string(&abs) else { continue };
        let lines: Vec<&str> = content.lines().collect();
        let Some((start, end)) = sym.id.rsplit_once(':').and_then(|(_, r)| {
            let (s, e) = r.split_once('-')?;
            Some((s.parse::<usize>().ok()?, e.parse::<usize>().ok()?))
        }) else { continue };
        if start == 0 || end > lines.len() {
            continue;
        }
        let body = lines[start - 1..end].join("\n");
        let cost = estimate_tokens(&body);
        if used + cost > budget {
            omitted += 1;
            continue;
        }
        used += cost;
        bodies.push(PackBody { path: sym.path.clone(), id: sym.id.clone(), name: sym.name.clone(), content: body });
    }

    Ok(ReadContextPackResult {
        keywords,
        files,
        symbols,
        bodies,
        bodies_omitted_for_budget: omitted,
        budget,
        token_count: used,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("auth.rs"),
            "pub fn login_user(name: &str) -> bool {\n    validate_password(name)\n}\n\nfn validate_password(name: &str) -> bool {\n    !name.is_empty()\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("billing.rs"),
            "pub fn charge_invoice(amount: u64) -> u64 {\n    amount * 2\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn relevant_file_and_symbol_ranked_first() {
        let dir = workspace();
        let r = read_context_pack(dir.path(), ReadContextPackParams {
            query: "fix the login password validation".into(),
            budget: None,
            path: None,
            max_files: None,
        })
        .unwrap();
        assert_eq!(r.files[0].path, "auth.rs");
        assert!(r.symbols.iter().any(|s| s.name.contains("validate_password")));
        assert!(r.bodies.iter().any(|b| b.content.contains("is_empty")));
        assert!(!r.bodies.iter().any(|b| b.content.contains("charge_invoice")));
    }

    #[test]
    fn budget_omits_bodies() {
        let dir = workspace();
        let r = read_context_pack(dir.path(), ReadContextPackParams {
            query: "login password validation".into(),
            budget: Some(500), // floor; ranking+signatures may already exceed body room
            path: None,
            max_files: None,
        })
        .unwrap();
        assert!(r.token_count <= 500 || r.bodies.is_empty());
    }

    #[test]
    fn empty_query_rejected() {
        let dir = workspace();
        assert!(read_context_pack(dir.path(), ReadContextPackParams {
            query: "fix the bug".into(), // all stopwords
            budget: None,
            path: None,
            max_files: None,
        })
        .is_err());
    }
}
