//! read_test_coverage — maps a coverage report onto code symbols so the agent
//! can see which functions are untested (i.e. risky to change).
//!
//! Supports lcov (`lcov.info`, also what `cargo llvm-cov --lcov` emits),
//! coverage.py JSON (`coverage json`), and cobertura XML. Coverage reports are
//! commonly gitignored, so we probe an explicit list of conventional paths
//! rather than walking the tree. When nothing is found we return a non-error
//! note with a generation hint (safe for speculative calls).

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::code::{ReadCodeSkeletonParams, read_code_skeleton};
use super::fs::estimate_tokens;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadTestCoverageParams {
    #[schemars(description = "Restrict to this file or directory (root-relative). Omit for the whole report.")]
    pub path: Option<String>,
    #[schemars(description = "Only return symbols that are not fully covered (pct < 100).")]
    pub uncovered_only: Option<bool>,
    #[schemars(description = "Only return symbols whose coverage percentage is below this value.")]
    pub threshold: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SymbolCoverage {
    pub name: String,
    pub line: usize,
    pub covered: usize,
    pub total: usize,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct FileCoverage {
    pub path: String,
    pub pct: f64,
    pub symbols: Vec<SymbolCoverage>,
}

#[derive(Debug, Serialize)]
pub struct ReadTestCoverageResult {
    pub report_available: bool,
    pub format: Option<String>,
    pub overall_pct: Option<f64>,
    pub files: Vec<FileCoverage>,
    pub hint: Option<String>,
    pub token_count: usize,
}

/// Per-file raw line coverage parsed from a report.
struct RawCoverage {
    covered: HashSet<usize>,
    instrumented: HashSet<usize>,
}

const LCOV_CANDIDATES: &[&str] = &[
    "lcov.info",
    "coverage/lcov.info",
    "coverage/tmp/lcov.info",
    ".nyc_output/lcov.info",
    "target/llvm-cov/lcov.info",
];
const COVERAGEPY_CANDIDATES: &[&str] = &["coverage.json"];
const COBERTURA_CANDIDATES: &[&str] = &[
    "coverage.xml",
    "cobertura.xml",
    "cobertura-coverage.xml",
    "coverage/cobertura-coverage.xml",
];

fn pct(covered: usize, total: usize) -> f64 {
    if total == 0 {
        100.0
    } else {
        (covered as f64 / total as f64 * 1000.0).round() / 10.0
    }
}

pub fn read_test_coverage(
    root: &Path,
    params: ReadTestCoverageParams,
) -> anyhow::Result<ReadTestCoverageResult> {
    let detected = detect_report(root);
    let Some((format, raw)) = detected else {
        return Ok(ReadTestCoverageResult {
            report_available: false,
            format: None,
            overall_pct: None,
            files: Vec::new(),
            hint: Some(
                "No coverage report found. Generate one first, e.g. `cargo llvm-cov --lcov --output-path lcov.info` (Rust), `pytest --cov --cov-report=json` (Python), or an lcov/cobertura reporter for JS/TS."
                    .to_string(),
            ),
            token_count: 0,
        });
    };

    let scope = params.path.as_deref().map(normalize_rel);
    let uncovered_only = params.uncovered_only.unwrap_or(false);
    let threshold = params.threshold;

    let mut files: Vec<FileCoverage> = Vec::new();
    let mut total_covered = 0usize;
    let mut total_instrumented = 0usize;

    // Stable output: sort files by path.
    let mut report_paths: Vec<&String> = raw.keys().collect();
    report_paths.sort();

    for report_path in report_paths {
        let rel = to_root_relative(root, report_path);
        if let Some(scope) = &scope
            && !rel.replace('\\', "/").starts_with(scope)
        {
            continue;
        }

        let cov = &raw[report_path];
        let file_covered = cov.covered.len();
        let file_instrumented = cov.instrumented.len();
        total_covered += file_covered;
        total_instrumented += file_instrumented;

        let mut symbols = map_symbols(root, &rel, cov);

        if uncovered_only {
            symbols.retain(|s| s.pct < 100.0);
        }
        if let Some(t) = threshold {
            symbols.retain(|s| s.pct < t);
        }

        // When filters drop every symbol, skip the file entirely to keep output tight.
        if (uncovered_only || threshold.is_some()) && symbols.is_empty() {
            continue;
        }

        files.push(FileCoverage {
            path: rel,
            pct: pct(file_covered, file_instrumented),
            symbols,
        });
    }

    let overall_pct = Some(pct(total_covered, total_instrumented));
    let json = serde_json::to_string(&files).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadTestCoverageResult {
        report_available: true,
        format: Some(format),
        overall_pct,
        files,
        hint: None,
        token_count,
    })
}

/// Probe conventional report locations. Returns the first match as
/// (format-name, path -> raw coverage).
fn detect_report(root: &Path) -> Option<(String, HashMap<String, RawCoverage>)> {
    for cand in LCOV_CANDIDATES {
        if let Ok(text) = std::fs::read_to_string(root.join(cand)) {
            let parsed = parse_lcov(&text);
            if !parsed.is_empty() {
                return Some(("lcov".to_string(), parsed));
            }
        }
    }
    for cand in COVERAGEPY_CANDIDATES {
        if let Ok(text) = std::fs::read_to_string(root.join(cand))
            && let Some(parsed) = parse_coveragepy(&text)
            && !parsed.is_empty()
        {
            return Some(("coveragepy".to_string(), parsed));
        }
    }
    for cand in COBERTURA_CANDIDATES {
        if let Ok(text) = std::fs::read_to_string(root.join(cand)) {
            let parsed = parse_cobertura(&text);
            if !parsed.is_empty() {
                return Some(("cobertura".to_string(), parsed));
            }
        }
    }
    None
}

fn parse_lcov(text: &str) -> HashMap<String, RawCoverage> {
    let mut out: HashMap<String, RawCoverage> = HashMap::new();
    let mut current: Option<String> = None;
    let mut covered: HashSet<usize> = HashSet::new();
    let mut instrumented: HashSet<usize> = HashSet::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(file) = line.strip_prefix("SF:") {
            current = Some(file.to_string());
            covered.clear();
            instrumented.clear();
        } else if let Some(da) = line.strip_prefix("DA:") {
            // DA:<line>,<hits>[,<checksum>]
            let mut parts = da.split(',');
            if let (Some(ln), Some(hits)) = (parts.next(), parts.next())
                && let (Ok(ln), Ok(hits)) = (ln.parse::<usize>(), hits.parse::<i64>())
            {
                instrumented.insert(ln);
                if hits > 0 {
                    covered.insert(ln);
                }
            }
        } else if line == "end_of_record"
            && let Some(file) = current.take()
        {
            out.insert(
                file,
                RawCoverage {
                    covered: std::mem::take(&mut covered),
                    instrumented: std::mem::take(&mut instrumented),
                },
            );
        }
    }
    out
}

fn parse_coveragepy(text: &str) -> Option<HashMap<String, RawCoverage>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let files = v.get("files")?.as_object()?;
    let mut out = HashMap::new();
    for (path, data) in files {
        let executed: HashSet<usize> = data
            .get("executed_lines")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect())
            .unwrap_or_default();
        let missing: HashSet<usize> = data
            .get("missing_lines")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect())
            .unwrap_or_default();
        let mut instrumented = executed.clone();
        instrumented.extend(&missing);
        out.insert(
            path.clone(),
            RawCoverage { covered: executed, instrumented },
        );
    }
    Some(out)
}

fn parse_cobertura(text: &str) -> HashMap<String, RawCoverage> {
    let mut out: HashMap<String, RawCoverage> = HashMap::new();
    // Match either a `filename="..."` attribute or a `<line number=".." hits="..">`
    // element, in document order; lines belong to the most recent filename.
    let re = Regex::new(r#"filename="([^"]+)"|<line number="(\d+)" hits="(\d+)""#).unwrap();
    let mut current: Option<String> = None;
    for cap in re.captures_iter(text) {
        if let Some(file) = cap.get(1) {
            current = Some(file.as_str().to_string());
            out.entry(file.as_str().to_string()).or_insert_with(|| RawCoverage {
                covered: HashSet::new(),
                instrumented: HashSet::new(),
            });
        } else if let (Some(ln), Some(hits), Some(file)) = (cap.get(2), cap.get(3), &current)
            && let (Ok(ln), Ok(hits)) = (ln.as_str().parse::<usize>(), hits.as_str().parse::<i64>())
            && let Some(entry) = out.get_mut(file)
        {
            entry.instrumented.insert(ln);
            if hits > 0 {
                entry.covered.insert(ln);
            }
        }
    }
    out
}

/// Map line coverage onto the file's symbols via read_code_skeleton. If the file
/// can't be read or parsed (missing / unsupported), returns an empty symbol list
/// (the file-level pct is still reported by the caller).
fn map_symbols(root: &Path, rel: &str, cov: &RawCoverage) -> Vec<SymbolCoverage> {
    let Ok(skeleton) = read_code_skeleton(
        root,
        ReadCodeSkeletonParams { path: rel.to_string(), include_blocks: None },
    ) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    for item in skeleton.skeleton {
        let mut total = 0usize;
        let mut covered = 0usize;
        for ln in item.start_line..=item.end_line {
            if cov.instrumented.contains(&ln) {
                total += 1;
                if cov.covered.contains(&ln) {
                    covered += 1;
                }
            }
        }
        // Skip symbols with no instrumented lines (e.g. pure declarations).
        if total == 0 {
            continue;
        }
        symbols.push(SymbolCoverage {
            name: item.name,
            line: item.start_line,
            covered,
            total,
            pct: pct(covered, total),
        });
    }
    symbols
}

/// Normalize a user-supplied scope path to forward slashes without a leading `./`.
fn normalize_rel(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches("./").to_string()
}

/// Convert a report's file path (possibly absolute) to a root-relative path.
fn to_root_relative(root: &Path, report_path: &str) -> String {
    let normalized = report_path.replace('\\', "/");
    if let Ok(canon_root) = root.canonicalize() {
        let root_str = canon_root.to_string_lossy().replace('\\', "/");
        if let Some(stripped) = normalized.strip_prefix(&root_str) {
            return stripped.trim_start_matches('/').to_string();
        }
    }
    let root_str = root.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = normalized.strip_prefix(&root_str) {
        return stripped.trim_start_matches('/').to_string();
    }
    normalized.trim_start_matches("./").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn no_report_is_non_error() {
        let dir = tempfile::tempdir().unwrap();
        let r = read_test_coverage(dir.path(), ReadTestCoverageParams { path: None, uncovered_only: None, threshold: None }).unwrap();
        assert!(!r.report_available);
        assert!(r.hint.is_some());
    }

    #[test]
    fn parses_lcov_and_maps_symbols() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/lib.rs", "fn covered() {\n    let x = 1;\n    let y = 2;\n}\nfn uncovered() {\n    let z = 3;\n    let w = 4;\n}\n");
        // covered(): lines 2,3 hit. uncovered(): lines 6,7 not hit.
        write(
            dir.path(),
            "lcov.info",
            "SF:src/lib.rs\nDA:2,5\nDA:3,5\nDA:6,0\nDA:7,0\nend_of_record\n",
        );
        let r = read_test_coverage(dir.path(), ReadTestCoverageParams { path: None, uncovered_only: None, threshold: None }).unwrap();
        assert!(r.report_available);
        assert_eq!(r.format.as_deref(), Some("lcov"));
        assert_eq!(r.overall_pct, Some(50.0));
        let file = &r.files[0];
        let covered = file.symbols.iter().find(|s| s.name == "covered").unwrap();
        assert_eq!(covered.pct, 100.0);
        let uncovered = file.symbols.iter().find(|s| s.name == "uncovered").unwrap();
        assert_eq!(uncovered.pct, 0.0);
    }

    #[test]
    fn uncovered_only_filters() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/lib.rs", "fn covered() {\n    let x = 1;\n}\nfn bad() {\n    let z = 3;\n}\n");
        write(dir.path(), "lcov.info", "SF:src/lib.rs\nDA:2,5\nDA:5,0\nend_of_record\n");
        let r = read_test_coverage(dir.path(), ReadTestCoverageParams { path: None, uncovered_only: Some(true), threshold: None }).unwrap();
        let file = &r.files[0];
        assert!(file.symbols.iter().all(|s| s.pct < 100.0));
        assert!(file.symbols.iter().any(|s| s.name == "bad"));
    }

    #[test]
    fn parses_coveragepy_json() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "app.py", "def f():\n    return 1\n");
        write(
            dir.path(),
            "coverage.json",
            r#"{"files":{"app.py":{"executed_lines":[1,2],"missing_lines":[]}}}"#,
        );
        let r = read_test_coverage(dir.path(), ReadTestCoverageParams { path: None, uncovered_only: None, threshold: None }).unwrap();
        assert!(r.report_available);
        assert_eq!(r.format.as_deref(), Some("coveragepy"));
        assert_eq!(r.overall_pct, Some(100.0));
    }

    #[test]
    fn parses_cobertura_xml() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "coverage.xml",
            r#"<coverage><packages><package><classes><class filename="src/a.rs"><lines><line number="1" hits="2"/><line number="2" hits="0"/></lines></class></classes></package></packages></coverage>"#,
        );
        let parsed = parse_cobertura(&std::fs::read_to_string(dir.path().join("coverage.xml")).unwrap());
        let cov = parsed.get("src/a.rs").unwrap();
        assert_eq!(cov.instrumented.len(), 2);
        assert_eq!(cov.covered.len(), 1);
    }
}
