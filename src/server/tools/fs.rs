use ignore::WalkBuilder;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write as FmtWrite;
use std::path::Path;

use crate::security::{rel_display, safe_path, scoped_root};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadDirectoryTreeParams {
    #[schemars(description = "Root-relative path to start from (omit for project root)")]
    pub path: Option<String>,
    #[schemars(description = "Maximum depth (default: 3, max: 10)")]
    pub depth: Option<usize>,
}

pub struct DirectoryTreeResult {
    pub tree: String,
    pub token_count: usize,
}

pub fn read_directory_tree(
    root: &Path,
    params: ReadDirectoryTreeParams,
) -> anyhow::Result<DirectoryTreeResult> {
    let start = scoped_root(root, params.path.as_deref())?;
    let depth = params.depth.unwrap_or(3).min(10);

    let mut out = String::new();
    let _ = writeln!(out, "./");

    let walker = WalkBuilder::new(&start)
        .max_depth(Some(depth))
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    let start_depth = start.components().count();

    for entry in walker.flatten() {
        let path = entry.path();
        if path == start {
            continue;
        }
        let depth_diff = path.components().count() - start_depth;
        if depth_diff == 0 {
            continue;
        }
        let indent = "│   ".repeat(depth_diff.saturating_sub(1));
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let is_dir = path.is_dir();
        let suffix = if is_dir { "/" } else { "" };
        let _ = writeln!(out, "{}├── {}{}", indent, name, suffix);
    }

    let token_count = estimate_tokens(&out);
    Ok(DirectoryTreeResult {
        tree: out,
        token_count,
    })
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchFileParams {
    #[schemars(description = "Root-relative file path to search")]
    pub path: String,
    #[schemars(description = "Search query (regex supported)")]
    pub query: String,
    #[schemars(description = "Lines of context before/after match (default: 2, max: 10)")]
    pub context_lines: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line: usize,
    pub content: String,
    pub context: Vec<String>,
}

pub struct SearchFileResult {
    pub matches: Vec<SearchMatch>,
    pub token_count: usize,
}

pub fn search_file(root: &Path, params: SearchFileParams) -> anyhow::Result<SearchFileResult> {
    let path = safe_path(root, &params.path)?;
    if path.is_dir() {
        anyhow::bail!("'{}' is a directory, not a file", params.path);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", params.path, e))?;
    let lines: Vec<&str> = content.lines().collect();
    let ctx = params.context_lines.unwrap_or(2).min(10);

    let re = Regex::new(&params.query)
        .map_err(|e| anyhow::anyhow!("Invalid regex '{}': {}", params.query, e))?;
    let mut matches = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if re.is_match(line) {
            let start = i.saturating_sub(ctx);
            let end = (i + ctx + 1).min(lines.len());
            let context: Vec<String> = (start..end)
                .filter(|&j| j != i)
                .map(|j| format!("{}: {}", j + 1, lines[j]))
                .collect();
            matches.push(SearchMatch {
                line: i + 1,
                content: line.to_string(),
                context,
            });
        }
    }

    let json = serde_json::to_string(&matches).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(SearchFileResult {
        matches,
        token_count,
    })
}

// ─── read_token_map ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadTokenMapParams {
    #[schemars(description = "Root-relative subdirectory to scan. Omit for entire workspace.")]
    pub path: Option<String>,
    #[schemars(
        description = "Maximum number of files to return (default: 50, max: 200). Results are sorted by token count descending."
    )]
    pub limit: Option<usize>,
    #[schemars(
        description = "Only include files matching this glob pattern (e.g. '*.ts', '*.rs'). Omit for all files."
    )]
    pub glob: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenMapEntry {
    pub path: String,
    pub estimated_tokens: usize,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct ReadTokenMapResult {
    pub files: Vec<TokenMapEntry>,
    pub total_tokens: usize,
    pub file_count: usize,
    pub token_count: usize,
}

pub fn read_token_map(
    root: &Path,
    params: ReadTokenMapParams,
) -> anyhow::Result<ReadTokenMapResult> {
    let start = scoped_root(root, params.path.as_deref())?;

    let limit = params.limit.unwrap_or(50).min(200);
    let glob_pat = params.glob.as_deref();

    let glob_re: Option<Regex> = if let Some(pat) = glob_pat {
        let regex_pat = glob_to_regex(pat);
        Some(Regex::new(&regex_pat).unwrap_or_else(|_| Regex::new(".*").unwrap()))
    } else {
        None
    };

    let mut entries: Vec<TokenMapEntry> = Vec::new();
    let mut total_tokens = 0usize;

    for entry in WalkBuilder::new(&start)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let path = entry.path().to_path_buf();
        if path.is_dir() {
            continue;
        }

        let rel = rel_display(root, &path);

        // Apply glob filter
        if let Some(ref re) = glob_re {
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            if !re.is_match(&file_name) && !re.is_match(&rel) {
                continue;
            }
        }

        // Skip obviously binary files by extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && matches!(
                ext,
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "ico"
                    | "svg"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "eot"
                    | "mp4"
                    | "webm"
                    | "mp3"
                    | "zip"
                    | "tar"
                    | "gz"
                    | "exe"
                    | "dll"
                    | "so"
                    | "dylib"
                    | "bin"
                    | "db"
                    | "sqlite"
                    | "lock"
            )
        {
            continue;
        }

        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        // Estimate tokens cheaply: read first 64KB, extrapolate
        let tokens = estimate_file_tokens(&path, size_bytes);
        total_tokens += tokens;
        entries.push(TokenMapEntry {
            path: rel,
            estimated_tokens: tokens,
            size_bytes,
        });
    }

    let file_count = entries.len();
    entries.sort_by_key(|e| std::cmp::Reverse(e.estimated_tokens));
    entries.truncate(limit);

    let json = serde_json::to_string(&entries).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadTokenMapResult {
        files: entries,
        total_tokens,
        file_count,
        token_count,
    })
}

fn estimate_file_tokens(path: &std::path::Path, size_bytes: u64) -> usize {
    // Read up to 32KB to compute average, then extrapolate for larger files
    const SAMPLE: u64 = 32768;
    if size_bytes == 0 {
        return 0;
    }

    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return size_bytes as usize / 4;
    };
    let sample_len = size_bytes.min(SAMPLE) as usize;
    let mut buf = vec![0u8; sample_len];
    let Ok(read) = f.read(&mut buf) else {
        return size_bytes as usize / 4;
    };
    buf.truncate(read);

    let Ok(text) = std::str::from_utf8(&buf) else {
        return 0;
    }; // binary
    let sample_tokens = estimate_tokens(text);
    if read as u64 >= size_bytes {
        sample_tokens
    } else {
        // Extrapolate
        (sample_tokens as f64 * (size_bytes as f64 / read as f64)) as usize
    }
}

fn glob_to_regex(glob: &str) -> String {
    let mut re = String::from("^");
    for ch in glob.chars() {
        match ch {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                re.push('\\');
                re.push(ch);
            }
            c => re.push(c),
        }
    }
    re.push('$');
    re
}

pub fn estimate_tokens(text: &str) -> usize {
    // CJK/Japanese chars are 3 UTF-8 bytes but ~1 token each (len/4 underestimates 40-60%).
    // Split by character class for better accuracy across Latin, CJK, and mixed content.
    let mut ascii = 0usize;
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        let cp = ch as u32;
        if ch.is_ascii() {
            ascii += 1;
        } else if matches!(cp, 0x3000..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    // ASCII ~4 chars/token, CJK ~1 char/token, other ~2 chars/token
    (ascii / 4 + cjk + other / 2).max(1)
}
