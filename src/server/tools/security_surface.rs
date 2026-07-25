use std::path::Path;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadSecuritySurfaceParams {
    #[schemars(
        description = "Root-relative file or directory to scan. Omit to scan entire workspace."
    )]
    pub path: Option<String>,
    #[schemars(
        description = "Categories to scan. Options: \"injection\", \"xss\", \"secrets\", \"unsafe\", \"path_traversal\", \"all\" (default: \"all\")"
    )]
    pub categories: Option<Vec<String>>,
    #[schemars(description = "Maximum findings to return (default: 100)")]
    pub limit: Option<usize>,
    #[schemars(
        description = "Also scan test code (Rust #[cfg(test)] modules, tests/ directories). Test fixtures are a large false-positive source, so they are skipped by default."
    )]
    pub include_tests: Option<bool>,
    #[schemars(
        description = "Minimum confidence to report: \"low\" (default, everything), \"medium\", or \"high\". Confidence is how likely a match is a real issue rather than a benign use of the pattern."
    )]
    pub min_confidence: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SecurityFinding {
    pub path: String,
    pub line: usize,
    pub category: String,
    pub severity: String,
    /// How likely this match is a real problem rather than a benign use of the
    /// pattern. `severity` describes the impact *if* it is real; `confidence`
    /// describes whether it is real at all.
    pub confidence: String,
    pub description: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct ReadSecuritySurfaceResult {
    pub findings: Vec<SecurityFinding>,
    pub total: usize,
    pub by_category: std::collections::HashMap<String, usize>,
    pub by_severity: std::collections::HashMap<String, usize>,
    pub by_confidence: std::collections::HashMap<String, usize>,
    /// Always present: this is a line-pattern scanner, not a taint analyzer.
    pub note: &'static str,
    pub token_count: usize,
}

struct Rule {
    category: &'static str,
    /// Impact if the finding is real.
    severity: &'static str,
    /// Likelihood the match is real. "low" means the pattern is common in
    /// correct code (e.g. every `Command::new` is flagged, most are fine).
    confidence: &'static str,
    pattern: &'static str,
    description: &'static str,
}

/// `rule!(category, severity, confidence, pattern, description)`
macro_rules! rule {
    ($cat:expr, $sev:expr, $conf:expr, $pat:expr, $desc:expr) => {
        Rule {
            category: $cat,
            severity: $sev,
            confidence: $conf,
            pattern: $pat,
            description: $desc,
        }
    };
}

const RULES: &[Rule] = &[
    // --- Command injection ---
    // Spawning a process is not itself a vulnerability; only unsanitized input is.
    // These stay low/medium confidence so the agent verifies before reporting.
    rule!(
        "injection",
        "high",
        "low",
        ".exec(",
        "Potential command injection via .exec()"
    ),
    rule!(
        "injection",
        "high",
        "medium",
        "shell_exec(",
        "Shell command execution"
    ),
    rule!(
        "injection",
        "high",
        "low",
        "system(",
        "Direct system() call"
    ),
    rule!(
        "injection",
        "high",
        "medium",
        "popen(",
        "popen() shell execution"
    ),
    rule!(
        "injection",
        "high",
        "low",
        "subprocess.call(",
        "subprocess.call() execution"
    ),
    rule!(
        "injection",
        "high",
        "low",
        "subprocess.Popen(",
        "subprocess.Popen() execution"
    ),
    rule!(
        "injection",
        "medium",
        "low",
        "Command::new(",
        "Rust Command::new — verify input is sanitized"
    ),
    rule!(
        "injection",
        "high",
        "low",
        "child_process.exec(",
        "Node.js child_process.exec()"
    ),
    rule!(
        "injection",
        "high",
        "low",
        "child_process.spawn(",
        "Node.js child_process.spawn()"
    ),
    rule!(
        "injection",
        "high",
        "medium",
        "Runtime.getRuntime().exec(",
        "Java Runtime.exec()"
    ),
    // SQL injection — building a query by interpolation is a strong signal.
    rule!(
        "injection",
        "high",
        "high",
        "format!(\"SELECT",
        "SQL query built with format! macro (possible injection)"
    ),
    rule!(
        "injection",
        "high",
        "high",
        "format!(\"INSERT",
        "SQL INSERT built with format! macro"
    ),
    rule!(
        "injection",
        "high",
        "high",
        "format!(\"UPDATE",
        "SQL UPDATE built with format! macro"
    ),
    rule!(
        "injection",
        "high",
        "high",
        "format!(\"DELETE",
        "SQL DELETE built with format! macro"
    ),
    rule!(
        "injection",
        "high",
        "high",
        "f\"SELECT",
        "Python f-string SQL query (possible injection)"
    ),
    rule!(
        "injection",
        "high",
        "high",
        "f\"INSERT",
        "Python f-string SQL INSERT"
    ),
    rule!(
        "injection",
        "high",
        "medium",
        "\" + req.",
        "String concatenation with request data (possible injection)"
    ),
    rule!(
        "injection",
        "high",
        "medium",
        "\" + params.",
        "String concatenation with params (possible injection)"
    ),
    rule!(
        "injection",
        "medium",
        "medium",
        "raw_query(",
        "Raw SQL query — verify parameterization"
    ),
    // A literal SQL string is the *safe* shape (no interpolation), hence low.
    rule!(
        "injection",
        "medium",
        "low",
        ".execute(\"",
        "Direct SQL execute with string literal"
    ),
    // --- XSS ---
    rule!(
        "xss",
        "high",
        "high",
        "innerHTML =",
        "Direct innerHTML assignment (XSS risk)"
    ),
    rule!(
        "xss",
        "high",
        "high",
        "innerHTML+=",
        "innerHTML append (XSS risk)"
    ),
    rule!(
        "xss",
        "high",
        "high",
        "dangerouslySetInnerHTML",
        "React dangerouslySetInnerHTML"
    ),
    rule!(
        "xss",
        "high",
        "high",
        "document.write(",
        "document.write() XSS vector"
    ),
    rule!(
        "xss",
        "high",
        "medium",
        "eval(",
        "eval() execution of arbitrary code"
    ),
    rule!(
        "xss",
        "medium",
        "high",
        "outerHTML =",
        "outerHTML assignment (XSS risk)"
    ),
    rule!(
        "xss",
        "medium",
        "medium",
        "insertAdjacentHTML(",
        "insertAdjacentHTML (verify escaping)"
    ),
    rule!(
        "xss",
        "high",
        "high",
        "__html:",
        "React __html key (dangerouslySetInnerHTML)"
    ),
    // --- Hardcoded secrets ---
    rule!(
        "secrets",
        "critical",
        "high",
        "password = \"",
        "Hardcoded password string"
    ),
    rule!(
        "secrets",
        "critical",
        "high",
        "password=\"",
        "Hardcoded password string"
    ),
    rule!(
        "secrets",
        "critical",
        "high",
        "api_key = \"",
        "Hardcoded API key"
    ),
    rule!(
        "secrets",
        "critical",
        "high",
        "api_key=\"",
        "Hardcoded API key"
    ),
    rule!(
        "secrets",
        "critical",
        "high",
        "secret = \"",
        "Hardcoded secret value"
    ),
    rule!(
        "secrets",
        "critical",
        "high",
        "secret=\"",
        "Hardcoded secret value"
    ),
    rule!(
        "secrets",
        "critical",
        "high",
        "token = \"",
        "Hardcoded token"
    ),
    rule!(
        "secrets",
        "high",
        "high",
        "private_key = \"",
        "Hardcoded private key"
    ),
    rule!(
        "secrets",
        "high",
        "low",
        "aws_secret",
        "AWS secret reference — verify not hardcoded"
    ),
    rule!(
        "secrets",
        "high",
        "high",
        "-----BEGIN",
        "PEM certificate or private key in source"
    ),
    // --- Unsafe code ---
    rule!(
        "unsafe",
        "medium",
        "medium",
        "unsafe {",
        "Rust unsafe block"
    ),
    rule!(
        "unsafe",
        "medium",
        "medium",
        "unsafe fn ",
        "Rust unsafe function"
    ),
    rule!(
        "unsafe",
        "high",
        "low",
        "from_raw(",
        "Raw pointer from_raw — verify ownership"
    ),
    rule!(
        "unsafe",
        "high",
        "medium",
        "transmute(",
        "mem::transmute — type safety bypass"
    ),
    rule!(
        "unsafe",
        "high",
        "low",
        "ctypes.",
        "Python ctypes usage — native memory access"
    ),
    rule!(
        "unsafe",
        "medium",
        "medium",
        "@SuppressWarnings(\"unchecked\")",
        "Java unchecked cast suppression"
    ),
    // --- Path traversal ---
    rule!(
        "path_traversal",
        "high",
        "low",
        "../",
        "Path traversal sequence in string literal"
    ),
    rule!(
        "path_traversal",
        "medium",
        "high",
        "Path::new(req.",
        "File path from request (verify sanitization)"
    ),
    rule!(
        "path_traversal",
        "medium",
        "high",
        "open(request.",
        "File open with request data"
    ),
    rule!(
        "path_traversal",
        "medium",
        "low",
        "File::open(",
        "Rust File::open — verify path is sanitized"
    ),
    rule!(
        "path_traversal",
        "medium",
        "low",
        "os.path.join(",
        "Python path join — verify no user traversal"
    ),
];

const NOTE: &str = "Heuristic line-pattern scan, not taint analysis. `severity` is impact if real; \
                    `confidence` is how likely the match is real. Verify anything below high \
                    confidence by reading the code. Test code is skipped unless include_tests:true.";

fn confidence_rank(c: &str) -> u8 {
    match c {
        "high" => 2,
        "medium" => 1,
        _ => 0,
    }
}

/// True when the pattern only means something as *code* (a call, an assignment, a
/// path expression). Such a pattern appearing inside a string literal is the pattern
/// being named, not used. Content patterns (`../`, `-----BEGIN`, `aws_secret`) are
/// expected inside literals, so they are exempt from that suppression.
fn is_code_pattern(pattern: &str) -> bool {
    pattern.contains('(') || pattern.contains('=') || pattern.contains("::")
}

/// True when every occurrence of `pattern` in `line` starts inside a quoted string
/// literal. Such matches are usually the pattern being *named* rather than used —
/// a rule table, a test fixture, an error message — and are the single largest
/// false-positive source. Kept deliberately simple (no escape-sequence tracking):
/// it only decides whether to suppress a heuristic finding.
fn only_matches_inside_string_literal(line: &str, pattern: &str) -> bool {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut quote = b'"';
    let mut any_match = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == quote {
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
        }
        // Does the pattern start at this byte?
        if line.is_char_boundary(i) && line[i..].starts_with(pattern) {
            any_match = true;
            // `in_string` is true here when the quote that opened the literal was
            // seen before this byte — i.e. the match begins inside the literal.
            if !in_string {
                return false;
            }
        }
        i += 1;
    }
    any_match
}

const CODE_EXTS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "rb", "cs", "php", "kt", "swift", "c",
    "cpp",
];

pub fn read_security_surface(
    root: &Path,
    params: ReadSecuritySurfaceParams,
) -> anyhow::Result<ReadSecuritySurfaceResult> {
    let limit = params.limit.unwrap_or(100);
    let include_tests = params.include_tests.unwrap_or(false);
    let min_confidence = confidence_rank(
        params
            .min_confidence
            .as_deref()
            .unwrap_or("low")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
    );

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

        let rel = rel_display(root, path);
        if !include_tests && is_test_path(&rel) {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            // Skip pure comments
            if trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with('*')
                || trimmed.starts_with("/*")
            {
                // A Rust `#[cfg(test)]` attribute starts the file's test module, which by
                // convention runs to EOF; its fixtures are pure false positives.
                if !include_tests && trimmed.starts_with("#[cfg(test)]") {
                    break;
                }
                continue;
            }

            for rule in RULES {
                if let Some(ref cats) = active_categories
                    && !cats.contains(&rule.category.to_string())
                {
                    continue;
                }
                if confidence_rank(rule.confidence) < min_confidence {
                    continue;
                }
                if !line.contains(rule.pattern) {
                    continue;
                }
                // A code pattern quoted inside a string literal is the pattern being
                // named (rule tables, error messages), not executed.
                if is_code_pattern(rule.pattern)
                    && only_matches_inside_string_literal(line, rule.pattern)
                {
                    continue;
                }

                let snippet = line.trim().chars().take(120).collect::<String>();
                findings.push(SecurityFinding {
                    path: rel.clone(),
                    line: line_idx + 1,
                    category: rule.category.to_string(),
                    severity: rule.severity.to_string(),
                    confidence: rule.confidence.to_string(),
                    description: rule.description.to_string(),
                    snippet,
                });

                if findings.len() >= limit {
                    break 'outer;
                }
            }
        }
    }

    // Report the most actionable findings first: confidence before severity, since a
    // high-severity guess still costs the agent a wasted verification round-trip.
    findings.sort_by(|a, b| {
        confidence_rank(&b.confidence)
            .cmp(&confidence_rank(&a.confidence))
            .then_with(|| severity_rank(&b.severity).cmp(&severity_rank(&a.severity)))
    });

    let total = findings.len();

    let mut by_category: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut by_severity: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut by_confidence: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for f in &findings {
        *by_category.entry(f.category.clone()).or_default() += 1;
        *by_severity.entry(f.severity.clone()).or_default() += 1;
        *by_confidence.entry(f.confidence.clone()).or_default() += 1;
    }

    let json = serde_json::to_string(&findings).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadSecuritySurfaceResult {
        findings,
        total,
        by_category,
        by_severity,
        by_confidence,
        note: NOTE,
        token_count,
    })
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "critical" => 3,
        "high" => 2,
        "medium" => 1,
        _ => 0,
    }
}

/// Path-based test detection, complementing the `#[cfg(test)]` cutoff.
fn is_test_path(rel: &str) -> bool {
    let norm = rel.replace('\\', "/");
    norm.split('/').any(|seg| {
        seg == "tests"
            || seg == "test"
            || seg == "__tests__"
            || seg == "spec"
            || seg == "fixtures"
            || seg == "testdata"
    }) || norm.ends_with("_test.go")
        || norm.ends_with("_test.py")
        || norm.ends_with("_test.rs")
        || norm.ends_with("_spec.rb")
        || norm.contains(".test.")
        || norm.contains(".spec.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_patterns_quoted_in_a_rule_table_are_suppressed() {
        // The scanner's own rule table must not report itself.
        let line =
            r#"    rule!("injection", "high", "low", ".exec(", "Potential command injection"),"#;
        assert!(only_matches_inside_string_literal(line, ".exec("));
        // A real call site is not suppressed.
        assert!(!only_matches_inside_string_literal(
            "child.exec(cmd)",
            ".exec("
        ));
    }

    #[test]
    fn secret_patterns_spanning_into_a_literal_are_not_suppressed() {
        // The match starts at `password`, outside the literal, so it survives.
        let line = r#"let cfg = Config { password = "hunter2" };"#;
        assert!(!only_matches_inside_string_literal(line, "password = \""));
    }

    #[test]
    fn content_patterns_are_exempt_from_string_suppression() {
        assert!(!is_code_pattern("../"));
        assert!(!is_code_pattern("-----BEGIN"));
        assert!(is_code_pattern("Command::new("));
        assert!(is_code_pattern("innerHTML ="));
    }

    #[test]
    fn test_paths_are_detected() {
        assert!(is_test_path("src/tests/helpers.rs"));
        assert!(is_test_path("pkg/foo_test.go"));
        assert!(is_test_path("web/app.spec.ts"));
        assert!(is_test_path("api/fixtures/user.json"));
        assert!(!is_test_path("src/server/tools/security_surface.rs"));
        // A path merely containing "test" as a substring is not a test path.
        assert!(!is_test_path("src/latest/mod.rs"));
    }

    #[test]
    fn min_confidence_filters_low_signal_rules() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.rs");
        // A low-confidence match (Command::new) and a high-confidence one (SQL format!).
        std::fs::write(
            &file,
            "fn go(name: &str) {\n    Command::new(name);\n    let q = format!(\"SELECT {}\", name);\n}\n",
        )
        .unwrap();

        let all = read_security_surface(
            dir.path(),
            ReadSecuritySurfaceParams {
                path: None,
                categories: None,
                limit: None,
                include_tests: None,
                min_confidence: None,
            },
        )
        .unwrap();
        assert!(all.total >= 2, "expected both matches, got {}", all.total);
        // Highest confidence sorts first.
        assert_eq!(all.findings[0].confidence, "high");

        let high_only = read_security_surface(
            dir.path(),
            ReadSecuritySurfaceParams {
                path: None,
                categories: None,
                limit: None,
                include_tests: None,
                min_confidence: Some("high".to_string()),
            },
        )
        .unwrap();
        assert!(high_only.findings.iter().all(|f| f.confidence == "high"));
        assert!(high_only.total < all.total);
    }

    #[test]
    fn test_files_are_skipped_by_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests").join("fixture.rs"),
            "let q = format!(\"SELECT {}\", x);\n",
        )
        .unwrap();

        let params = |include_tests| ReadSecuritySurfaceParams {
            path: None,
            categories: None,
            limit: None,
            include_tests,
            min_confidence: None,
        };
        assert_eq!(
            read_security_surface(dir.path(), params(None))
                .unwrap()
                .total,
            0
        );
        assert!(
            read_security_surface(dir.path(), params(Some(true)))
                .unwrap()
                .total
                > 0
        );
    }

    #[test]
    fn cfg_test_module_cuts_off_scanning() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            "fn real() {}\n#[cfg(test)]\nmod tests {\n    let q = format!(\"SELECT {}\", x);\n}\n",
        )
        .unwrap();
        let result = read_security_surface(
            dir.path(),
            ReadSecuritySurfaceParams {
                path: None,
                categories: None,
                limit: None,
                include_tests: None,
                min_confidence: None,
            },
        )
        .unwrap();
        assert_eq!(
            result.total, 0,
            "findings inside #[cfg(test)] must be skipped"
        );
    }
}
