//! `read_type_diagnostics` — LSP-equivalent static type diagnostics.
//!
//! Rather than running a long-lived language server, this drives each language's
//! own diagnostics engine in check-only mode (the same compilers/type-checkers
//! that `rust-analyzer`, `tsserver`, `pyright` and `gopls` wrap) and parses the
//! result into a single, deduplicated, token-compact list of diagnostics:
//! `{file, line, col, severity, code, message}`.
//!
//! - Rust       → `cargo check --message-format=json`
//! - TypeScript → `npx --no-install tsc --noEmit --pretty false`
//! - Python     → `pyright --outputjson` (falls back to `mypy`)
//! - Go         → `go vet ./...`
//!
//! When the checker is not installed the tool returns `checker_available: false`
//! with an install hint instead of erroring, so it is safe to call speculatively.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::{rel_display, safe_path};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadTypeDiagnosticsParams {
    /// Optional file or directory (root-relative) to scope diagnostics to. Omit for the whole workspace.
    pub path: Option<String>,
    /// Force a checker: rust | typescript | python | go. Omit to auto-detect from the manifest/extension.
    pub language: Option<String>,
    /// Minimum severity to include: error | warning | hint. Default: warning.
    pub severity: Option<String>,
    /// Maximum diagnostics to return (default 100).
    pub max_items: Option<usize>,
    /// Maximum execution time in seconds (default 180, max 600).
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsSummary {
    pub errors: usize,
    pub warnings: usize,
    pub hints: usize,
    pub shown: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadTypeDiagnosticsResult {
    pub language: String,
    pub checker: String,
    pub checker_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub summary: DiagnosticsSummary,
    pub token_count: usize,
}

/// Numeric rank for severities — higher is more severe. Used for both the
/// minimum-severity filter and for ordering errors before warnings.
fn severity_rank(s: &str) -> u8 {
    match s.trim().to_lowercase().as_str() {
        "error" | "err" => 3,
        "warning" | "warn" => 2,
        "hint" | "info" | "information" | "note" | "help" => 1,
        _ => 2,
    }
}

pub fn read_type_diagnostics(
    root: &Path,
    params: ReadTypeDiagnosticsParams,
) -> anyhow::Result<ReadTypeDiagnosticsResult> {
    let scoped = match &params.path {
        Some(p) => Some(safe_path(root, p).map_err(|e| anyhow::anyhow!("{e}"))?),
        None => None,
    };
    let scoped_ref = scoped.as_deref();

    let lang = detect_language(root, scoped_ref, params.language.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "型チェッカー対象言語を判別できませんでした。language を rust/typescript/python/go のいずれかで指定してください。"
        )
    })?;

    let timeout = Duration::from_secs(params.timeout_secs.unwrap_or(180).min(600));
    let min_sev = severity_rank(params.severity.as_deref().unwrap_or("warning"));
    let max_items = params.max_items.unwrap_or(100);

    let (checker, available, raw) = match lang {
        "rust" => check_rust(root, timeout),
        "typescript" => check_typescript(root, timeout),
        "python" => check_python(root, scoped_ref, timeout),
        "go" => check_go(root, timeout),
        _ => unreachable!("detect_language only yields known languages"),
    };

    if !available {
        let note = unavailable_hint(lang, &checker);
        return Ok(ReadTypeDiagnosticsResult {
            language: lang.to_string(),
            checker,
            checker_available: false,
            note: Some(note),
            diagnostics: vec![],
            summary: DiagnosticsSummary { errors: 0, warnings: 0, hints: 0, shown: 0, total: 0 },
            token_count: 40,
        });
    }

    // Normalise file paths to root-relative form and apply the optional path scope.
    let scope_prefix = scoped_ref.map(|p| rel_display(root, p));
    let mut diags: Vec<Diagnostic> = raw
        .into_iter()
        .map(|mut d| {
            d.file = normalize_file(root, &d.file);
            d
        })
        .filter(|d| severity_rank(&d.severity) >= min_sev)
        .filter(|d| match &scope_prefix {
            Some(prefix) if !prefix.is_empty() && prefix != "." => d.file.starts_with(prefix.as_str()),
            _ => true,
        })
        .collect();

    // Deduplicate identical diagnostics (a checker may report the same span twice).
    diags.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags.dedup();

    let errors = diags.iter().filter(|d| d.severity == "error").count();
    let warnings = diags.iter().filter(|d| d.severity == "warning").count();
    let hints = diags.iter().filter(|d| d.severity == "hint").count();
    let total = diags.len();

    if diags.len() > max_items {
        diags.truncate(max_items);
    }
    let shown = diags.len();

    let repr = diags
        .iter()
        .map(|d| format!("{}:{}:{} {} {}", d.file, d.line, d.col, d.severity, d.message))
        .collect::<Vec<_>>()
        .join("\n");
    let token_count = estimate_tokens(&repr).max(20);

    Ok(ReadTypeDiagnosticsResult {
        language: lang.to_string(),
        checker,
        checker_available: true,
        note: None,
        diagnostics: diags,
        summary: DiagnosticsSummary { errors, warnings, hints, shown, total },
        token_count,
    })
}

fn detect_language(
    root: &Path,
    path: Option<&Path>,
    forced: Option<&str>,
) -> Option<&'static str> {
    if let Some(f) = forced {
        return match f.trim().to_lowercase().as_str() {
            "rust" | "rs" | "cargo" => Some("rust"),
            "typescript" | "ts" | "tsx" | "javascript" | "js" => Some("typescript"),
            "python" | "py" => Some("python"),
            "go" | "golang" => Some("go"),
            _ => None,
        };
    }

    if let Some(ext) = path.and_then(|p| p.extension()).and_then(|e| e.to_str()) {
        match ext {
            "rs" => return Some("rust"),
            "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" => {
                return Some("typescript");
            }
            "py" | "pyi" => return Some("python"),
            "go" => return Some("go"),
            _ => {}
        }
    }

    if root.join("Cargo.toml").exists() {
        return Some("rust");
    }
    if root.join("tsconfig.json").exists() || root.join("package.json").exists() {
        return Some("typescript");
    }
    if root.join("pyproject.toml").exists()
        || root.join("setup.py").exists()
        || root.join("setup.cfg").exists()
        || root.join("mypy.ini").exists()
    {
        return Some("python");
    }
    if root.join("go.mod").exists() {
        return Some("go");
    }
    None
}

fn unavailable_hint(lang: &str, checker: &str) -> String {
    let install = match lang {
        "rust" => "rustup component add などで Rust ツールチェインを導入してください",
        "typescript" => "プロジェクトに typescript を devDependency として追加してください（npm i -D typescript）",
        "python" => "pip install pyright もしくは pip install mypy で型チェッカーを導入してください",
        "go" => "Go ツールチェインを導入してください（go.dev/dl）",
        _ => "対応する型チェッカーを導入してください",
    };
    format!("{checker} が見つかりませんでした（check-only スキップ）。{install}。")
}

/// Convert a checker-emitted path (absolute or relative-to-cwd) into a stable
/// root-relative form with forward slashes.
fn normalize_file(root: &Path, raw: &str) -> String {
    let p = Path::new(raw);
    let s = if p.is_absolute() {
        rel_display(root, p)
    } else {
        raw.replace('\\', "/")
    };
    s.trim_start_matches("./").to_string()
}

// ─── command execution ───────────────────────────────────────────────────────

/// Run a command line through the platform shell, capturing stdout/stderr with a
/// timeout. Routing through the shell resolves npm `.cmd` shims and PATH the same
/// way `run_command` does. Returns `(stdout, stderr)`.
fn run_shell(cmdline: &str, cwd: &Path, timeout: Duration) -> (String, String) {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", cmdline]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", cmdline]);
        c
    };
    cmd.current_dir(cwd);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (String::new(), format!("spawn failed: {e}")),
    };

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
        Ok(Err(e)) => (String::new(), format!("wait failed: {e}")),
        Err(_) => (String::new(), "(timed out)".into()),
    }
}

/// Heuristic: does this stderr indicate the checker binary itself is missing?
fn looks_unavailable(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    s.contains("command not found")
        || s.contains("is not recognized")
        || s.contains("not recognized as an internal or external command")
        || s.contains("no such file or directory")
        || s.contains("could not determine executable to run")
        || s.contains("spawn failed")
}

// ─── per-language drivers ────────────────────────────────────────────────────

fn check_rust(cwd: &Path, timeout: Duration) -> (String, bool, Vec<Diagnostic>) {
    let (stdout, stderr) = run_shell(
        "cargo check --message-format=json --quiet --all-targets",
        cwd,
        timeout,
    );
    if stdout.trim().is_empty() && looks_unavailable(&stderr) {
        return ("cargo check".into(), false, vec![]);
    }
    ("cargo check".into(), true, parse_cargo_json(&stdout))
}

fn check_typescript(cwd: &Path, timeout: Duration) -> (String, bool, Vec<Diagnostic>) {
    let (stdout, stderr) = run_shell("npx --no-install tsc --noEmit --pretty false", cwd, timeout);
    let combined = format!("{stdout}\n{stderr}");
    if looks_unavailable(&combined) {
        return ("tsc".into(), false, vec![]);
    }
    ("tsc".into(), true, parse_tsc(&combined))
}

fn check_python(cwd: &Path, path: Option<&Path>, timeout: Duration) -> (String, bool, Vec<Diagnostic>) {
    let target = path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into());

    // Prefer pyright (JSON output, no config required).
    let (stdout, stderr) = run_shell(&format!("pyright --outputjson \"{target}\""), cwd, timeout);
    if !looks_unavailable(&stderr) && stdout.trim_start().starts_with('{') {
        return ("pyright".into(), true, parse_pyright_json(&stdout));
    }

    // Fall back to mypy.
    let (mout, merr) = run_shell(
        &format!("mypy --show-column-numbers --no-error-summary --no-color-output \"{target}\""),
        cwd,
        timeout,
    );
    let combined = format!("{mout}\n{merr}");
    if looks_unavailable(&combined) {
        return ("pyright/mypy".into(), false, vec![]);
    }
    ("mypy".into(), true, parse_mypy(&combined))
}

fn check_go(cwd: &Path, timeout: Duration) -> (String, bool, Vec<Diagnostic>) {
    let (stdout, stderr) = run_shell("go vet ./...", cwd, timeout);
    let combined = format!("{stdout}\n{stderr}");
    if stdout.trim().is_empty() && looks_unavailable(&stderr) {
        return ("go vet".into(), false, vec![]);
    }
    ("go vet".into(), true, parse_go(&combined))
}

// ─── output parsers (pure — unit-tested) ─────────────────────────────────────

/// Parse `cargo check --message-format=json` line-delimited JSON.
fn parse_cargo_json(stdout: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        let severity = match msg.get("level").and_then(|l| l.as_str()).unwrap_or("") {
            "error" | "error: internal compiler error" => "error",
            "warning" => "warning",
            "note" | "help" => "hint",
            _ => continue,
        };
        let message = msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        // Skip the aggregate trailers cargo emits after the real diagnostics.
        if message.starts_with("aborting due to")
            || message.starts_with("For more information about this error")
            || message.starts_with("Some errors have detailed")
        {
            continue;
        }
        let code = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
        let span = msg.get("spans").and_then(|s| s.as_array()).and_then(|arr| {
            arr.iter()
                .find(|s| s.get("is_primary").and_then(|b| b.as_bool()).unwrap_or(false))
                .or_else(|| arr.first())
        });
        let (file, line, col) = match span {
            Some(s) => (
                s.get("file_name").and_then(|f| f.as_str()).unwrap_or("").to_string(),
                s.get("line_start").and_then(|l| l.as_u64()).unwrap_or(0) as u32,
                s.get("column_start").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
            ),
            None => continue, // span-less messages carry no location worth surfacing
        };
        if file.is_empty() {
            continue;
        }
        out.push(Diagnostic { file, line, col, severity: severity.into(), code, message });
    }
    out
}

/// Parse `tsc --noEmit --pretty false` lines like:
/// `src/x.ts(10,5): error TS2322: Type 'string' is not assignable ...`
fn parse_tsc(output: &str) -> Vec<Diagnostic> {
    let re = Regex::new(r"^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.*)$").unwrap();
    output
        .lines()
        .filter_map(|l| {
            let c = re.captures(l.trim())?;
            Some(Diagnostic {
                file: c[1].to_string(),
                line: c[2].parse().ok()?,
                col: c[3].parse().ok()?,
                severity: if &c[4] == "warning" { "warning" } else { "error" }.into(),
                code: Some(c[5].to_string()),
                message: c[6].trim().to_string(),
            })
        })
        .collect()
}

/// Parse `pyright --outputjson`.
fn parse_pyright_json(output: &str) -> Vec<Diagnostic> {
    let v: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v.get("generalDiagnostics").and_then(|d| d.as_array()) {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|d| {
            let file = d.get("file").and_then(|f| f.as_str())?.to_string();
            let severity = match d.get("severity").and_then(|s| s.as_str()).unwrap_or("error") {
                "error" => "error",
                "warning" => "warning",
                _ => "hint",
            };
            let start = d.get("range").and_then(|r| r.get("start"));
            // pyright positions are 0-based.
            let line = start.and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as u32 + 1;
            let col = start.and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as u32 + 1;
            let message = d
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .replace('\n', " ");
            let code = d.get("rule").and_then(|r| r.as_str()).map(|s| s.to_string());
            Some(Diagnostic { file, line, col, severity: severity.into(), code, message })
        })
        .collect()
}

/// Parse `mypy --show-column-numbers` lines like:
/// `x.py:10:5: error: Incompatible types  [assignment]`
fn parse_mypy(output: &str) -> Vec<Diagnostic> {
    let re = Regex::new(r"^(.+?):(\d+):(?:(\d+):)?\s+(error|warning|note):\s+(.*)$").unwrap();
    let code_re = Regex::new(r"\s*\[([a-z0-9-]+)\]\s*$").unwrap();
    output
        .lines()
        .filter_map(|l| {
            let c = re.captures(l.trim())?;
            let severity = match &c[4] {
                "warning" => "warning",
                "note" => "hint",
                _ => "error",
            };
            let mut message = c[5].trim().to_string();
            let code = code_re
                .captures(&message)
                .map(|m| m[1].to_string());
            if code.is_some() {
                message = code_re.replace(&message, "").trim().to_string();
            }
            Some(Diagnostic {
                file: c[1].to_string(),
                line: c[2].parse().ok()?,
                col: c.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
                severity: severity.into(),
                code,
                message,
            })
        })
        .collect()
}

/// Parse `go vet` / `go build` lines like:
/// `./x.go:10:5: undefined: foo`
fn parse_go(output: &str) -> Vec<Diagnostic> {
    let re = Regex::new(r"^(.+?\.go):(\d+):(?:(\d+):)?\s+(.*)$").unwrap();
    output
        .lines()
        .filter_map(|l| {
            let c = re.captures(l.trim())?;
            Some(Diagnostic {
                file: c[1].to_string(),
                line: c[2].parse().ok()?,
                col: c.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0),
                severity: "error".into(),
                code: None,
                message: c[4].trim().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_json_extracts_primary_span() {
        let line = r#"{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","code":{"code":"E0308"},"spans":[{"file_name":"src/x.rs","line_start":10,"column_start":5,"is_primary":true}]}}"#;
        let diags = parse_cargo_json(line);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.file, "src/x.rs");
        assert_eq!(d.line, 10);
        assert_eq!(d.col, 5);
        assert_eq!(d.severity, "error");
        assert_eq!(d.code.as_deref(), Some("E0308"));
        assert_eq!(d.message, "mismatched types");
    }

    #[test]
    fn cargo_json_skips_non_compiler_and_trailers() {
        let out = [
            r#"{"reason":"compiler-artifact","target":{"name":"x"}}"#,
            r#"{"reason":"compiler-message","message":{"level":"error","message":"aborting due to 1 previous error","spans":[]}}"#,
            r#"not json"#,
        ]
        .join("\n");
        assert!(parse_cargo_json(&out).is_empty());
    }

    #[test]
    fn cargo_json_picks_primary_among_multiple_spans() {
        let line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable: `y`","code":{"code":"unused_variables"},"spans":[{"file_name":"a.rs","line_start":1,"column_start":1,"is_primary":false},{"file_name":"b.rs","line_start":7,"column_start":9,"is_primary":true}]}}"#;
        let diags = parse_cargo_json(line);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file, "b.rs");
        assert_eq!(diags[0].line, 7);
        assert_eq!(diags[0].severity, "warning");
    }

    #[test]
    fn tsc_parses_error_line() {
        let out = "src/app.ts(12,3): error TS2322: Type 'string' is not assignable to type 'number'.";
        let diags = parse_tsc(out);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.file, "src/app.ts");
        assert_eq!(d.line, 12);
        assert_eq!(d.col, 3);
        assert_eq!(d.severity, "error");
        assert_eq!(d.code.as_deref(), Some("TS2322"));
    }

    #[test]
    fn tsc_ignores_unrelated_lines() {
        let out = "Version 5.4.2\nsrc/x.ts(1,1): error TS1005: ';' expected.\n";
        assert_eq!(parse_tsc(out).len(), 1);
    }

    #[test]
    fn pyright_json_is_one_based() {
        let out = r#"{"generalDiagnostics":[{"file":"/proj/x.py","severity":"error","rule":"reportGeneralTypeIssues","message":"Cannot assign","range":{"start":{"line":9,"character":4}}}]}"#;
        let diags = parse_pyright_json(out);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.line, 10);
        assert_eq!(d.col, 5);
        assert_eq!(d.severity, "error");
        assert_eq!(d.code.as_deref(), Some("reportGeneralTypeIssues"));
    }

    #[test]
    fn mypy_parses_code_and_column() {
        let out = "x.py:10:5: error: Incompatible types in assignment  [assignment]";
        let diags = parse_mypy(out);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.file, "x.py");
        assert_eq!(d.line, 10);
        assert_eq!(d.col, 5);
        assert_eq!(d.code.as_deref(), Some("assignment"));
        assert_eq!(d.message, "Incompatible types in assignment");
    }

    #[test]
    fn mypy_handles_missing_column() {
        let out = "x.py:3: note: Revealed type is \"builtins.int\"";
        let diags = parse_mypy(out);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].col, 0);
        assert_eq!(diags[0].severity, "hint");
    }

    #[test]
    fn go_parses_vet_line() {
        let out = "./main.go:10:6: undefined: foo";
        let diags = parse_go(out);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.file, "./main.go");
        assert_eq!(d.line, 10);
        assert_eq!(d.col, 6);
        assert_eq!(d.severity, "error");
    }

    #[test]
    fn severity_rank_orders_correctly() {
        assert!(severity_rank("error") > severity_rank("warning"));
        assert!(severity_rank("warning") > severity_rank("hint"));
        assert_eq!(severity_rank("unknown"), severity_rank("warning"));
    }

    #[test]
    fn detect_language_by_extension_and_manifest() {
        let tmp = std::env::temp_dir();
        assert_eq!(
            detect_language(&tmp, Some(Path::new("a.ts")), None),
            Some("typescript")
        );
        assert_eq!(detect_language(&tmp, Some(Path::new("a.rs")), None), Some("rust"));
        assert_eq!(detect_language(&tmp, None, Some("go")), Some("go"));
        assert_eq!(detect_language(&tmp, None, Some("bogus")), None);
    }
}
