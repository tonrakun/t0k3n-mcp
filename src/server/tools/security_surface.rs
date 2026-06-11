use std::path::Path;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadSecuritySurfaceParams {
    #[schemars(description = "Root-relative file or directory to scan. Omit to scan entire workspace.")]
    pub path: Option<String>,
    #[schemars(
        description = "Categories to scan. Options: \"injection\", \"xss\", \"secrets\", \"unsafe\", \"path_traversal\", \"all\" (default: \"all\")"
    )]
    pub categories: Option<Vec<String>>,
    #[schemars(description = "Maximum findings to return (default: 100)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SecurityFinding {
    pub path: String,
    pub line: usize,
    pub category: String,
    pub severity: String,
    pub description: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct ReadSecuritySurfaceResult {
    pub findings: Vec<SecurityFinding>,
    pub total: usize,
    pub by_category: std::collections::HashMap<String, usize>,
    pub by_severity: std::collections::HashMap<String, usize>,
    pub token_count: usize,
}

struct Rule {
    category: &'static str,
    severity: &'static str,
    pattern: &'static str,
    description: &'static str,
}

const RULES: &[Rule] = &[
    // --- Command injection ---
    Rule { category: "injection", severity: "high", pattern: ".exec(", description: "Potential command injection via .exec()" },
    Rule { category: "injection", severity: "high", pattern: "shell_exec(", description: "Shell command execution" },
    Rule { category: "injection", severity: "high", pattern: "system(", description: "Direct system() call" },
    Rule { category: "injection", severity: "high", pattern: "popen(", description: "popen() shell execution" },
    Rule { category: "injection", severity: "high", pattern: "subprocess.call(", description: "subprocess.call() execution" },
    Rule { category: "injection", severity: "high", pattern: "subprocess.Popen(", description: "subprocess.Popen() execution" },
    Rule { category: "injection", severity: "medium", pattern: "Command::new(", description: "Rust Command::new — verify input is sanitized" },
    Rule { category: "injection", severity: "high", pattern: "child_process.exec(", description: "Node.js child_process.exec()" },
    Rule { category: "injection", severity: "high", pattern: "child_process.spawn(", description: "Node.js child_process.spawn()" },
    Rule { category: "injection", severity: "high", pattern: "Runtime.getRuntime().exec(", description: "Java Runtime.exec()" },
    // SQL injection
    Rule { category: "injection", severity: "high", pattern: "format!(\"SELECT", description: "SQL query built with format! macro (possible injection)" },
    Rule { category: "injection", severity: "high", pattern: "format!(\"INSERT", description: "SQL INSERT built with format! macro" },
    Rule { category: "injection", severity: "high", pattern: "format!(\"UPDATE", description: "SQL UPDATE built with format! macro" },
    Rule { category: "injection", severity: "high", pattern: "format!(\"DELETE", description: "SQL DELETE built with format! macro" },
    Rule { category: "injection", severity: "high", pattern: "f\"SELECT", description: "Python f-string SQL query (possible injection)" },
    Rule { category: "injection", severity: "high", pattern: "f\"INSERT", description: "Python f-string SQL INSERT" },
    Rule { category: "injection", severity: "high", pattern: "\" + req.", description: "String concatenation with request data (possible injection)" },
    Rule { category: "injection", severity: "high", pattern: "\" + params.", description: "String concatenation with params (possible injection)" },
    Rule { category: "injection", severity: "medium", pattern: "raw_query(", description: "Raw SQL query — verify parameterization" },
    Rule { category: "injection", severity: "medium", pattern: ".execute(\"", description: "Direct SQL execute with string literal" },
    // --- XSS ---
    Rule { category: "xss", severity: "high", pattern: "innerHTML =", description: "Direct innerHTML assignment (XSS risk)" },
    Rule { category: "xss", severity: "high", pattern: "innerHTML+=", description: "innerHTML append (XSS risk)" },
    Rule { category: "xss", severity: "high", pattern: "dangerouslySetInnerHTML", description: "React dangerouslySetInnerHTML" },
    Rule { category: "xss", severity: "high", pattern: "document.write(", description: "document.write() XSS vector" },
    Rule { category: "xss", severity: "high", pattern: "eval(", description: "eval() execution of arbitrary code" },
    Rule { category: "xss", severity: "medium", pattern: "outerHTML =", description: "outerHTML assignment (XSS risk)" },
    Rule { category: "xss", severity: "medium", pattern: "insertAdjacentHTML(", description: "insertAdjacentHTML (verify escaping)" },
    Rule { category: "xss", severity: "high", pattern: "__html:", description: "React __html key (dangerouslySetInnerHTML)" },
    // --- Hardcoded secrets ---
    Rule { category: "secrets", severity: "critical", pattern: "password = \"", description: "Hardcoded password string" },
    Rule { category: "secrets", severity: "critical", pattern: "password=\"", description: "Hardcoded password string" },
    Rule { category: "secrets", severity: "critical", pattern: "api_key = \"", description: "Hardcoded API key" },
    Rule { category: "secrets", severity: "critical", pattern: "api_key=\"", description: "Hardcoded API key" },
    Rule { category: "secrets", severity: "critical", pattern: "secret = \"", description: "Hardcoded secret value" },
    Rule { category: "secrets", severity: "critical", pattern: "secret=\"", description: "Hardcoded secret value" },
    Rule { category: "secrets", severity: "critical", pattern: "token = \"", description: "Hardcoded token" },
    Rule { category: "secrets", severity: "high", pattern: "private_key = \"", description: "Hardcoded private key" },
    Rule { category: "secrets", severity: "high", pattern: "aws_secret", description: "AWS secret reference — verify not hardcoded" },
    Rule { category: "secrets", severity: "high", pattern: "-----BEGIN", description: "PEM certificate or private key in source" },
    // --- Unsafe code ---
    Rule { category: "unsafe", severity: "medium", pattern: "unsafe {", description: "Rust unsafe block" },
    Rule { category: "unsafe", severity: "medium", pattern: "unsafe fn ", description: "Rust unsafe function" },
    Rule { category: "unsafe", severity: "high", pattern: "from_raw(", description: "Raw pointer from_raw — verify ownership" },
    Rule { category: "unsafe", severity: "high", pattern: "transmute(", description: "mem::transmute — type safety bypass" },
    Rule { category: "unsafe", severity: "high", pattern: "ctypes.", description: "Python ctypes usage — native memory access" },
    Rule { category: "unsafe", severity: "medium", pattern: "@SuppressWarnings(\"unchecked\")", description: "Java unchecked cast suppression" },
    // --- Path traversal ---
    Rule { category: "path_traversal", severity: "high", pattern: "../", description: "Path traversal sequence in string literal" },
    Rule { category: "path_traversal", severity: "medium", pattern: "Path::new(req.", description: "File path from request (verify sanitization)" },
    Rule { category: "path_traversal", severity: "medium", pattern: "open(request.", description: "File open with request data" },
    Rule { category: "path_traversal", severity: "medium", pattern: "File::open(", description: "Rust File::open — verify path is sanitized" },
    Rule { category: "path_traversal", severity: "medium", pattern: "os.path.join(", description: "Python path join — verify no user traversal" },
];

const CODE_EXTS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "rb", "cs", "php", "kt", "swift", "c", "cpp",
];

pub fn read_security_surface(
    root: &Path,
    params: ReadSecuritySurfaceParams,
) -> anyhow::Result<ReadSecuritySurfaceResult> {
    let limit = params.limit.unwrap_or(100);

    let active_categories: Option<Vec<String>> = params.categories.as_ref().and_then(|cats| {
        if cats.iter().any(|c| c == "all") {
            None
        } else {
            Some(cats.clone())
        }
    });

    let search_root =
        scoped_root(root, params.path.as_deref()).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut findings: Vec<SecurityFinding> = Vec::new();

    'outer: for entry in WalkBuilder::new(&search_root)
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
        if !CODE_EXTS.contains(&ext) {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = rel_display(root, path);

        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            // Skip pure comments
            if trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with('*')
                || trimmed.starts_with("/*")
            {
                continue;
            }

            for rule in RULES {
                if let Some(ref cats) = active_categories
                    && !cats.contains(&rule.category.to_string()) {
                        continue;
                    }
                if !line.contains(rule.pattern) {
                    continue;
                }

                let snippet = line.trim().chars().take(120).collect::<String>();
                findings.push(SecurityFinding {
                    path: rel.clone(),
                    line: line_idx + 1,
                    category: rule.category.to_string(),
                    severity: rule.severity.to_string(),
                    description: rule.description.to_string(),
                    snippet,
                });

                if findings.len() >= limit {
                    break 'outer;
                }
            }
        }
    }

    let total = findings.len();

    let mut by_category: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut by_severity: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &findings {
        *by_category.entry(f.category.clone()).or_default() += 1;
        *by_severity.entry(f.severity.clone()).or_default() += 1;
    }

    let json = serde_json::to_string(&findings).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadSecuritySurfaceResult {
        findings,
        total,
        by_category,
        by_severity,
        token_count,
    })
}
