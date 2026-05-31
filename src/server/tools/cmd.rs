use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::safe_path;
use super::fs::estimate_tokens;

// ─── run_command ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunCommandParams {
    #[schemars(description = "Shell command to execute (e.g. 'cargo build --release', 'npm test', 'go build ./...'). Executed via sh -c on Unix / cmd /C on Windows.")]
    pub command: String,
    #[schemars(description = "Working directory relative to project root. Omit to use root.")]
    pub cwd: Option<String>,
    #[schemars(description = "Maximum execution time in seconds (default: 120, max: 600).")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct RunCommandResult {
    pub command: String,
    pub exit_code: i32,
    pub success: bool,
    pub duration_ms: u64,
    /// On success: last ~30 non-empty lines (where most tools print their final summary).
    /// On failure: last ~20 non-empty lines for context.
    pub summary: String,
    /// Lines matching error patterns (e.g. error[E0123], npm ERR!, SyntaxError:).
    /// Empty on success.
    pub errors: Vec<String>,
    /// Lines matching warning patterns (e.g. warning:, deprecated). Always extracted.
    pub warnings: Vec<String>,
    pub token_count: usize,
}

pub fn run_command(root: &Path, params: RunCommandParams) -> anyhow::Result<RunCommandResult> {
    let cwd = match &params.cwd {
        Some(d) => safe_path(root, d).map_err(|e| anyhow::anyhow!("{e}"))?,
        None => root.to_path_buf(),
    };

    let timeout = Duration::from_secs(params.timeout_secs.unwrap_or(120).min(600));
    let command_str = params.command.trim().to_string();

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", &command_str]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", &command_str]);
        c
    };

    cmd.current_dir(&cwd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let start = Instant::now();
    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("コマンド起動失敗: {e}"))?;

    // Run in a thread so we can enforce a timeout via channel recv_timeout.
    // If the timeout fires the child is orphaned (best-effort); the thread will
    // eventually clean up when the process finishes on its own.
    let (tx, rx) = mpsc::channel::<std::io::Result<std::process::Output>>();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(result) => result.map_err(|e| anyhow::anyhow!("実行失敗: {e}"))?,
        Err(_) => {
            return Err(anyhow::anyhow!(
                "タイムアウト: コマンドが {} 秒以内に完了しませんでした。\
                 timeout_secs を増やすか、コマンドを確認してください。",
                timeout.as_secs()
            ));
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = output.status.code().unwrap_or(-1);
    let success = output.status.success();

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Many build tools write progress to stdout and errors to stderr;
    // merge both so filtering sees everything.
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (true, false) => stderr,
        (false, true) => stdout,
        (false, false) => format!("{stdout}\n{stderr}"),
    };

    let all_lines: Vec<&str> = combined.lines().collect();

    let warnings = extract_warnings(&all_lines);
    let (errors, summary) = if success {
        (vec![], build_success_summary(&all_lines))
    } else {
        let errs = extract_errors(&all_lines);
        let summ = build_failure_summary(&all_lines);
        (errs, summ)
    };

    let repr = format!("{}\n{}", summary, errors.join("\n"));
    let token_count = estimate_tokens(&repr);

    Ok(RunCommandResult {
        command: command_str,
        exit_code,
        success,
        duration_ms,
        summary,
        errors,
        warnings,
        token_count,
    })
}

// ─── Output classification ────────────────────────────────────────────────────

fn extract_errors(lines: &[&str]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();

    for (i, &line) in lines.iter().enumerate() {
        if is_error_line(line) {
            // Prepend one context line (non-noise, non-empty, not already last)
            if i > 0 {
                let prev = lines[i - 1];
                if !prev.trim().is_empty()
                    && !is_noise_line(prev)
                    && result.last().map_or(true, |l: &String| l.trim() != prev.trim())
                {
                    result.push(format!("  {prev}"));
                }
            }
            result.push(line.to_string());
        }
    }

    result.dedup();
    result
}

fn extract_warnings(lines: &[&str]) -> Vec<String> {
    let mut result: Vec<String> = lines
        .iter()
        .filter(|&&l| is_warning_line(l))
        .map(|&l| l.to_string())
        .collect();
    result.dedup();
    result
}

/// Success summary: last 30 non-empty lines (past build progress noise).
fn build_success_summary(lines: &[&str]) -> String {
    let tail: Vec<&str> = lines
        .iter()
        .rev()
        .filter(|&&l| !l.trim().is_empty())
        .take(30)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    tail.join("\n")
}

/// Failure summary: last 20 non-empty lines for context.
fn build_failure_summary(lines: &[&str]) -> String {
    let tail: Vec<&str> = lines
        .iter()
        .rev()
        .filter(|&&l| !l.trim().is_empty())
        .take(20)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    tail.join("\n")
}

// ─── Pattern matching ─────────────────────────────────────────────────────────

fn is_error_line(line: &str) -> bool {
    let t = line.trim();
    let lower = t.to_lowercase();

    // ── Rust / Cargo ──────────────────────────────────────────────────────────
    if t.starts_with("error[") { return true; }
    if t.starts_with("error: ") || t == "error" { return true; }
    if lower.contains("could not compile") || lower.contains("failed to compile") { return true; }
    if lower.contains("build failed") { return true; }
    // aborting due to N previous error(s)
    if lower.starts_with("aborting due to") && lower.contains("error") { return true; }

    // ── TypeScript / tsc ──────────────────────────────────────────────────────
    // src/foo.ts(10,5): error TS2304: Cannot find name 'foo'.
    if line.contains("error TS") { return true; }
    // Found N error(s) — summary line
    if lower.starts_with("found ") && lower.contains("error") { return true; }

    // ── npm / Node ────────────────────────────────────────────────────────────
    if lower.starts_with("npm err!") { return true; }
    // Unhandled/uncaught exceptions
    if t.contains("Error: ") && (lower.contains("unhandled") || lower.contains("uncaught") || t.starts_with("Error: ")) { return true; }

    // ── Python ────────────────────────────────────────────────────────────────
    // Exception class names at start of line: SyntaxError: ...
    const PY_EXCEPTIONS: &[&str] = &[
        "SyntaxError:", "TypeError:", "NameError:", "ValueError:",
        "ImportError:", "ModuleNotFoundError:", "AttributeError:",
        "RuntimeError:", "FileNotFoundError:", "OSError:", "KeyError:",
        "IndexError:", "ZeroDivisionError:", "AssertionError:",
        "IndentationError:", "UnicodeDecodeError:",
    ];
    for exc in PY_EXCEPTIONS {
        if t.starts_with(exc) { return true; }
    }
    if t == "Traceback (most recent call last):" { return true; }

    // ── Go ────────────────────────────────────────────────────────────────────
    // ./pkg/foo.go:12:5: undefined: Bar
    if is_go_error(line) { return true; }

    // ── Make / CMake ──────────────────────────────────────────────────────────
    if lower.contains("*** error") || lower.starts_with("make: ***") { return true; }
    if lower.starts_with("cmake error") || lower.contains("cmake error:") { return true; }

    // ── Maven / Gradle ────────────────────────────────────────────────────────
    if lower.contains("build failure") { return true; }
    if lower.contains("[error]") { return true; }
    if lower.contains("compilation error") { return true; }

    // ── Test runners (cargo test, pytest, Jest) ───────────────────────────────
    // "FAILED tests/foo.py::bar" or "FAILED" standalone
    if t == "FAILED" || lower == "failed" { return true; }
    if t.starts_with("FAILED ") { return true; }
    // "failures:" section header in cargo test output
    if lower.trim_start().starts_with("failures:") { return true; }
    // Jest failure marker
    if lower.starts_with("● ") { return true; }
    // "X failed" — summary lines
    if lower.contains(" failed") && (lower.contains(" passed") || lower.contains(" error")) { return true; }

    // ── Generic ───────────────────────────────────────────────────────────────
    // "error:" prefix after optional file path prefix (e.g. "src/lib.rs: error:")
    if lower.contains(": error:") || lower.contains(": error ") { return true; }

    false
}

fn is_warning_line(line: &str) -> bool {
    let t = line.trim();
    let lower = t.to_lowercase();

    // ── Rust / Cargo ──────────────────────────────────────────────────────────
    if t.starts_with("warning[") { return true; }
    if t.starts_with("warning: ") { return true; }

    // ── TypeScript ────────────────────────────────────────────────────────────
    if line.contains("warning TS") { return true; }

    // ── npm ───────────────────────────────────────────────────────────────────
    if lower.starts_with("npm warn ") { return true; }

    // ── Generic warn / warning prefix ─────────────────────────────────────────
    if lower.starts_with("warn: ") || lower.starts_with("warning: ") { return true; }
    if lower.contains(": warning:") { return true; }

    // ── Deprecation ───────────────────────────────────────────────────────────
    if lower.contains("deprecated") && lower.contains("warning") { return true; }

    false
}

/// Lines that are pure build progress noise — never useful as error context.
fn is_noise_line(line: &str) -> bool {
    let t = line.trim();
    t.is_empty()
        || t.starts_with("Compiling ")
        || t.starts_with("Downloading ")
        || t.starts_with("Installing ")
        || t.starts_with("Fetching ")
        || t.starts_with("Resolving ")
        || t.starts_with("Checking ")
        || t.starts_with("Locking ")
        || t.starts_with("  Downloaded ")
        || t.starts_with("  Unpacking ")
}

fn is_go_error(line: &str) -> bool {
    if !line.contains(".go:") { return false; }
    let lower = line.to_lowercase();
    lower.contains("undefined")
        || lower.contains("cannot")
        || lower.contains("invalid")
        || lower.contains("undeclared")
        || lower.contains("declared and not used")
        || lower.contains("imported and not used")
        || lower.contains("does not implement")
        || lower.contains("type mismatch")
        || lower.contains("not enough arguments")
        || lower.contains("too many arguments")
}
