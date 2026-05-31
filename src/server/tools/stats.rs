use std::collections::HashMap;
use std::path::Path;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadWorkspaceStatsParams {
    #[schemars(description = "Glob filter (e.g. 'src/**/*.ts'). Omit to include all files.")]
    pub glob: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LanguageStat {
    pub language: String,
    pub files: usize,
    pub lines: usize,
    pub tokens: usize,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct LargestFile {
    pub path: String,
    pub tokens: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadWorkspaceStatsResult {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_tokens: usize,
    pub by_language: Vec<LanguageStat>,
    pub largest_files: Vec<LargestFile>,
    pub token_count: usize,
}

pub fn read_workspace_stats(root: &Path, params: ReadWorkspaceStatsParams) -> anyhow::Result<ReadWorkspaceStatsResult> {
    let glob_re = params.glob.as_deref().map(glob_to_regex).and_then(|p| regex::Regex::new(&p).ok());

    // (files, lines, tokens)
    let mut by_language: HashMap<String, (usize, usize, usize)> = HashMap::new();
    let mut all_files: Vec<(String, usize)> = Vec::new();

    for entry in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() { continue; }

        let rel = path.strip_prefix(root).unwrap_or(path)
            .to_string_lossy().replace('\\', "/");

        if let Some(ref re) = glob_re {
            if !re.is_match(&rel) { continue; }
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let language = ext_to_language(&ext);

        let Ok(content) = std::fs::read_to_string(path) else { continue };
        let lines = content.lines().count();
        let tokens = estimate_tokens(&content);

        let stat = by_language.entry(language).or_default();
        stat.0 += 1;
        stat.1 += lines;
        stat.2 += tokens;

        all_files.push((rel, tokens));
    }

    let total_files: usize = by_language.values().map(|(f, _, _)| f).sum();
    let total_lines: usize = by_language.values().map(|(_, l, _)| l).sum();
    let total_tokens: usize = by_language.values().map(|(_, _, t)| t).sum();

    let mut by_language_vec: Vec<LanguageStat> = by_language
        .into_iter()
        .map(|(lang, (files, lines, tokens))| {
            let pct = if total_tokens > 0 {
                (tokens as f64 / total_tokens as f64 * 100.0 * 10.0).round() / 10.0
            } else { 0.0 };
            LanguageStat { language: lang, files, lines, tokens, pct }
        })
        .collect();
    by_language_vec.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    all_files.sort_by(|(_, a), (_, b)| b.cmp(a));
    let largest_files: Vec<LargestFile> = all_files.into_iter().take(10)
        .map(|(path, tokens)| LargestFile { path, tokens })
        .collect();

    let result_json = serde_json::json!({
        "total_files": total_files,
        "total_lines": total_lines,
        "total_tokens": total_tokens,
        "by_language": by_language_vec,
        "largest_files": largest_files,
    });
    let token_count = estimate_tokens(&result_json.to_string());

    Ok(ReadWorkspaceStatsResult {
        total_files,
        total_lines,
        total_tokens,
        by_language: by_language_vec,
        largest_files,
        token_count,
    })
}

fn ext_to_language(ext: &str) -> String {
    match ext {
        "rs" => "rust",
        "py" | "pyw" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "go" => "go",
        "cpp" | "cc" | "cxx" => "c++",
        "c" => "c",
        "h" | "hpp" | "hh" => "c/c++ header",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "cs" => "c#",
        "php" => "php",
        "lua" => "lua",
        "md" | "markdown" | "mdx" => "markdown",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "sh" | "bash" => "shell",
        "sql" => "sql",
        "proto" => "protobuf",
        "graphql" | "gql" => "graphql",
        "tf" | "tfvars" => "terraform",
        "" => "no extension",
        other => other,
    }.to_string()
}

fn glob_to_regex(glob: &str) -> String {
    let mut re = String::from("^");
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') { chars.next(); }
                re.push_str(".*");
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
    }
    re.push('$');
    re
}
