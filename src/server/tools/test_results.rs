use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::safe_path;

// ─── read_test_results ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadTestResultsParams {
    #[schemars(
        description = "Root-relative path to a test output file. Use instead of 'text' for file-based output."
    )]
    pub path: Option<String>,
    #[schemars(description = "Raw test output text (paste from terminal). Use instead of 'path'.")]
    pub text: Option<String>,
    #[schemars(
        description = "Test framework hint: 'jest', 'vitest', 'pytest', 'cargo', 'go', 'auto' (default: auto)."
    )]
    pub framework: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestSuiteEntry {
    pub name: String,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub failed_tests: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TestFailure {
    pub name: String,
    pub suite: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ReadTestResultsResult {
    pub framework: String,
    pub summary: TestSummary,
    pub suites: Vec<TestSuiteEntry>,
    pub failures: Vec<TestFailure>,
    pub token_count: usize,
}

pub fn read_test_results(
    root: &Path,
    params: ReadTestResultsParams,
) -> anyhow::Result<ReadTestResultsResult> {
    let text = if let Some(t) = params.text {
        t
    } else if let Some(ref p) = params.path {
        let file_path = safe_path(root, p)?;
        std::fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?
    } else {
        anyhow::bail!("'path' か 'text' のどちらかを指定してください");
    };

    let framework = detect_framework(&text, params.framework.as_deref());

    let (summary, suites, failures) = match framework.as_str() {
        "jest" | "vitest" => parse_jest(&text),
        "pytest" => parse_pytest(&text),
        "cargo" => parse_cargo(&text),
        "go" => parse_go_test(&text),
        _ => parse_generic_test(&text),
    };

    let json = serde_json::to_string(&(&summary, &suites, &failures)).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadTestResultsResult {
        framework,
        summary,
        suites,
        failures,
        token_count,
    })
}

fn detect_framework(text: &str, hint: Option<&str>) -> String {
    if let Some(h) = hint
        && h != "auto"
    {
        return h.to_string();
    }
    // Jest/Vitest: PASS/FAIL lines with file paths, bullet symbols
    if (text.contains("\nPASS ") || text.contains("\nFAIL ")) && text.contains(" ms)") {
        return "jest".to_string();
    }
    // pytest: module::test_name PASSED/FAILED
    if (text.contains(" PASSED") || text.contains(" FAILED")) && text.contains("::") {
        return "pytest".to_string();
    }
    // cargo test
    if text.contains("test result:") || (text.contains("... ok") && text.contains("running ")) {
        return "cargo".to_string();
    }
    // go test
    if text.contains("--- PASS:") || text.contains("--- FAIL:") {
        return "go".to_string();
    }
    "generic".to_string()
}

// ─── Jest / Vitest parser ─────────────────────────────────────────────────────

fn parse_jest(text: &str) -> (TestSummary, Vec<TestSuiteEntry>, Vec<TestFailure>) {
    let mut suites: Vec<TestSuiteEntry> = Vec::new();
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut current_suite = String::new();
    let mut suite_passed = 0usize;
    let mut suite_failed = 0usize;
    let mut suite_skipped = 0usize;
    let mut suite_failures: Vec<String> = Vec::new();
    let mut total_passed = 0usize;
    let mut total_failed = 0usize;
    let mut total_skipped = 0usize;
    let mut duration_ms: Option<u64> = None;

    let re_suite = Regex::new(r"^(PASS|FAIL)\s+(.+)$").unwrap();
    // ✓ ✔ √ — passing
    let re_pass = Regex::new(r"^\s+[✓✔√]\s+(.+?)(?:\s+\(\d+\s*m?s\))?\s*$").unwrap();
    // ✕ ✗ × ● — failing
    let re_fail = Regex::new(r"^\s+[✕✗×●]\s+(.+?)(?:\s+\(\d+\s*m?s\))?\s*$").unwrap();
    // ○ ⊘ ↓ — skipped
    let re_skip = Regex::new(r"^\s+[○⊘↓]\s+(.+)$").unwrap();
    let re_summary =
        Regex::new(r"Tests:\s+(?:(\d+) failed(?:,\s*)?)?(?:(\d+) skipped(?:,\s*)?)?(\d+) passed")
            .unwrap();
    let re_duration = Regex::new(r"Time:\s+([\d.]+)\s*s").unwrap();

    let flush_suite =
        |suite: &str, p: usize, f: usize, s: usize, fl: &[String]| -> TestSuiteEntry {
            TestSuiteEntry {
                name: suite.to_string(),
                passed: p,
                failed: f,
                skipped: s,
                failed_tests: fl.to_vec(),
            }
        };

    for line in text.lines() {
        if let Some(cap) = re_suite.captures(line) {
            if !current_suite.is_empty() {
                suites.push(flush_suite(
                    &current_suite,
                    suite_passed,
                    suite_failed,
                    suite_skipped,
                    &suite_failures,
                ));
            }
            current_suite = cap[2].to_string();
            suite_passed = 0;
            suite_failed = 0;
            suite_skipped = 0;
            suite_failures = Vec::new();
        } else if re_pass.is_match(line) {
            suite_passed += 1;
        } else if let Some(cap) = re_fail.captures(line) {
            suite_failed += 1;
            let name = cap[1].to_string();
            suite_failures.push(name.clone());
            failures.push(TestFailure {
                name,
                suite: current_suite.clone(),
                message: String::new(),
            });
        } else if re_skip.is_match(line) {
            suite_skipped += 1;
        } else if let Some(cap) = re_summary.captures(line) {
            total_failed = cap
                .get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            total_skipped = cap
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            total_passed = cap
                .get(3)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
        } else if let Some(cap) = re_duration.captures(line)
            && let Ok(s) = cap[1].parse::<f64>()
        {
            duration_ms = Some((s * 1000.0) as u64);
        }
    }

    if !current_suite.is_empty() {
        suites.push(flush_suite(
            &current_suite,
            suite_passed,
            suite_failed,
            suite_skipped,
            &suite_failures,
        ));
    }

    let total = total_passed + total_failed + total_skipped;
    let summary = TestSummary {
        total,
        passed: total_passed,
        failed: total_failed,
        skipped: total_skipped,
        duration_ms,
    };
    (summary, suites, failures)
}

// ─── pytest parser ────────────────────────────────────────────────────────────

fn parse_pytest(text: &str) -> (TestSummary, Vec<TestSuiteEntry>, Vec<TestFailure>) {
    let mut suite_map: HashMap<String, TestSuiteEntry> = HashMap::new();
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut total_passed = 0usize;
    let mut total_failed = 0usize;
    let mut total_skipped = 0usize;
    let mut duration_ms: Option<u64> = None;

    let re_test = Regex::new(r"^(.+?)::(\S+)\s+(PASSED|FAILED|ERROR|SKIPPED|XFAIL|XPASS)").unwrap();
    let re_summary =
        Regex::new(r"=+\s+(?:(\d+) failed[,\s])?(?:(\d+) passed[,\s])?(?:(\d+) skipped[,\s])?")
            .unwrap();
    let re_duration = Regex::new(r"in ([\d.]+)s").unwrap();

    for line in text.lines() {
        if let Some(cap) = re_test.captures(line) {
            let suite = cap[1].to_string();
            let test_name = cap[2].to_string();
            let status = &cap[3];
            let entry = suite_map
                .entry(suite.clone())
                .or_insert_with(|| TestSuiteEntry {
                    name: suite.clone(),
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    failed_tests: Vec::new(),
                });
            match status {
                "PASSED" | "XPASS" => entry.passed += 1,
                "FAILED" | "ERROR" => {
                    entry.failed += 1;
                    entry.failed_tests.push(test_name.clone());
                    failures.push(TestFailure {
                        name: test_name,
                        suite,
                        message: String::new(),
                    });
                }
                _ => entry.skipped += 1,
            }
        } else if let Some(cap) = re_summary.captures(line) {
            if let Some(m) = cap.get(1) {
                total_failed = m.as_str().parse().unwrap_or(0);
            }
            if let Some(m) = cap.get(2) {
                total_passed = m.as_str().parse().unwrap_or(0);
            }
            if let Some(m) = cap.get(3) {
                total_skipped = m.as_str().parse().unwrap_or(0);
            }
        } else if let Some(cap) = re_duration.captures(line)
            && let Ok(s) = cap[1].parse::<f64>()
        {
            duration_ms = Some((s * 1000.0) as u64);
        }
    }

    let suites: Vec<TestSuiteEntry> = suite_map.into_values().collect();
    let total = total_passed + total_failed + total_skipped;
    let summary = TestSummary {
        total,
        passed: total_passed,
        failed: total_failed,
        skipped: total_skipped,
        duration_ms,
    };
    (summary, suites, failures)
}

// ─── cargo test parser ────────────────────────────────────────────────────────

fn parse_cargo(text: &str) -> (TestSummary, Vec<TestSuiteEntry>, Vec<TestFailure>) {
    let mut suite_map: HashMap<String, TestSuiteEntry> = HashMap::new();
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut total_passed = 0usize;
    let mut total_failed = 0usize;
    let mut total_ignored = 0usize;
    let mut duration_ms: Option<u64> = None;
    let mut current_suite = "default".to_string();

    let re_running = Regex::new(r"running \d+ tests?(?:\s+\((.+?)\))?").unwrap();
    let re_test = Regex::new(r"^test (.+?) \.\.\. (ok|FAILED|ignored)").unwrap();
    let re_result =
        Regex::new(r"test result:.*?(\d+) passed; (\d+) failed; (\d+) ignored;.*?([\d.]+)s")
            .unwrap();

    for line in text.lines() {
        if let Some(cap) = re_running.captures(line) {
            current_suite = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "default".to_string());
        } else if let Some(cap) = re_test.captures(line) {
            let test_name = cap[1].to_string();
            let entry = suite_map
                .entry(current_suite.clone())
                .or_insert_with(|| TestSuiteEntry {
                    name: current_suite.clone(),
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    failed_tests: Vec::new(),
                });
            match &cap[2] {
                "ok" => entry.passed += 1,
                "FAILED" => {
                    entry.failed += 1;
                    entry.failed_tests.push(test_name.clone());
                    failures.push(TestFailure {
                        name: test_name,
                        suite: current_suite.clone(),
                        message: String::new(),
                    });
                }
                _ => entry.skipped += 1,
            }
        } else if let Some(cap) = re_result.captures(line) {
            total_passed += cap[1].parse::<usize>().unwrap_or(0);
            total_failed += cap[2].parse::<usize>().unwrap_or(0);
            total_ignored += cap[3].parse::<usize>().unwrap_or(0);
            if let Ok(s) = cap[4].parse::<f64>() {
                duration_ms = Some((s * 1000.0) as u64);
            }
        }
    }

    let suites: Vec<TestSuiteEntry> = suite_map.into_values().collect();
    let total = total_passed + total_failed + total_ignored;
    let summary = TestSummary {
        total,
        passed: total_passed,
        failed: total_failed,
        skipped: total_ignored,
        duration_ms,
    };
    (summary, suites, failures)
}

// ─── go test parser ───────────────────────────────────────────────────────────

fn parse_go_test(text: &str) -> (TestSummary, Vec<TestSuiteEntry>, Vec<TestFailure>) {
    let mut suite_map: HashMap<String, TestSuiteEntry> = HashMap::new();
    let mut failures: Vec<TestFailure> = Vec::new();
    let mut duration_ms: Option<u64> = None;
    let mut current_pkg = "main".to_string();

    let re_pkg = Regex::new(r"^(?:ok|FAIL)\s+(\S+)\s+([\d.]+)s").unwrap();
    let re_run = Regex::new(r"^=== RUN\s+(\S+)").unwrap();
    let re_test = Regex::new(r"^--- (PASS|FAIL): (\w+) \(([\d.]+)s\)").unwrap();

    for line in text.lines() {
        if let Some(cap) = re_pkg.captures(line) {
            current_pkg = cap[1].to_string();
            if let Ok(s) = cap[2].parse::<f64>() {
                duration_ms = Some((s * 1000.0) as u64);
            }
        } else if let Some(cap) = re_run.captures(line) {
            // Ensure the suite entry exists so the package appears even with no results yet
            suite_map
                .entry(current_pkg.clone())
                .or_insert_with(|| TestSuiteEntry {
                    name: current_pkg.clone(),
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    failed_tests: Vec::new(),
                });
            let _ = cap; // suppress unused warning
        } else if let Some(cap) = re_test.captures(line) {
            let status = &cap[1];
            let test_name = cap[2].to_string();
            let entry = suite_map
                .entry(current_pkg.clone())
                .or_insert_with(|| TestSuiteEntry {
                    name: current_pkg.clone(),
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    failed_tests: Vec::new(),
                });
            if status == "PASS" {
                entry.passed += 1;
            } else {
                entry.failed += 1;
                entry.failed_tests.push(test_name.clone());
                failures.push(TestFailure {
                    name: test_name,
                    suite: current_pkg.clone(),
                    message: String::new(),
                });
            }
        }
    }

    let suites: Vec<TestSuiteEntry> = suite_map.into_values().collect();
    let total_passed: usize = suites.iter().map(|s| s.passed).sum();
    let total_failed: usize = suites.iter().map(|s| s.failed).sum();
    let total_skipped: usize = suites.iter().map(|s| s.skipped).sum();
    let total = total_passed + total_failed + total_skipped;
    let summary = TestSummary {
        total,
        passed: total_passed,
        failed: total_failed,
        skipped: total_skipped,
        duration_ms,
    };
    (summary, suites, failures)
}

// ─── generic parser ───────────────────────────────────────────────────────────

fn parse_generic_test(text: &str) -> (TestSummary, Vec<TestSuiteEntry>, Vec<TestFailure>) {
    let mut passed = 0usize;
    let mut failed = 0usize;

    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("pass") || lower.contains(" ok ") {
            passed += 1;
        } else if lower.contains("fail") || lower.contains("error") {
            failed += 1;
        }
    }

    let summary = TestSummary {
        total: passed + failed,
        passed,
        failed,
        skipped: 0,
        duration_ms: None,
    };
    (summary, Vec::new(), Vec::new())
}
