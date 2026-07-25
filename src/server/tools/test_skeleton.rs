use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::{rel_display, safe_path};

// ─── read_test_skeleton ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadTestSkeletonParams {
    #[schemars(
        description = "Root-relative path to a test file (*.test.ts, *_test.go, test_*.py, *Test.java, *_test.rs, etc.)."
    )]
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct TestItem {
    pub id: String,
    pub name: String,
    pub kind: String, // "describe" | "it" | "test" | "suite" | "case"
    pub level: usize, // nesting depth (0 = top-level)
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadTestSkeletonResult {
    pub path: String,
    pub framework: String,
    pub tests: Vec<TestItem>,
    pub token_count: usize,
}

pub fn read_test_skeleton(
    root: &Path,
    params: ReadTestSkeletonParams,
) -> anyhow::Result<ReadTestSkeletonResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let rel = rel_display(root, &file_path);
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let (framework, tests) = match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" => parse_jest_style(&content),
        "py" => parse_pytest_style(&content),
        "rs" => parse_rust_test(&content),
        "go" => parse_go_test_skeleton(&content),
        "java" | "kt" => parse_java_test(&content),
        "rb" => parse_rspec(&content),
        _ => parse_jest_style(&content), // best-effort fallback
    };

    let json = serde_json::to_string(&tests).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadTestSkeletonResult {
        path: rel,
        framework,
        tests,
        token_count,
    })
}

// ─── Jest / Vitest / Mocha (JS/TS) ───────────────────────────────────────────

fn parse_jest_style(content: &str) -> (String, Vec<TestItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    // Matches: describe("name", ...) | it("name") | test("name") | test.each | describe.each
    let re = Regex::new(
        r#"^(\s*)(describe(?:\.(?:each|skip|only))?|(?:it|test)(?:\.(?:each|skip|only|todo))?)\s*[(\`'"](.+?)[`'"]\s*[,)]"#
    ).unwrap();

    let mut depth_stack: Vec<(usize, usize)> = Vec::new(); // (indent_chars, start_line_0indexed)

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = re.captures(line) {
            let indent = cap[1].len();
            let keyword = cap[2].to_string();
            let name = cap[3].to_string();

            // Maintain depth stack by indentation
            while let Some(&(last_indent, _)) = depth_stack.last() {
                if indent <= last_indent {
                    depth_stack.pop();
                } else {
                    break;
                }
            }
            let level = depth_stack.len();

            let kind = if keyword.starts_with("describe") {
                "describe"
            } else {
                "test"
            }
            .to_string();

            // Find closing brace by counting braces from this line
            let start_line = i + 1;
            let end_line = find_block_end_lines(&lines, i);

            let id = format!("{}:{}-{}", kind, start_line, end_line);

            if kind == "describe" {
                depth_stack.push((indent, i));
            }

            items.push(TestItem {
                id,
                name,
                kind,
                level,
                start_line,
                end_line,
            });
        }
    }

    ("jest".to_string(), items)
}

// ─── pytest (Python) ─────────────────────────────────────────────────────────

fn parse_pytest_style(content: &str) -> (String, Vec<TestItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re_class = Regex::new(r"^class\s+(Test\w+)\s*[:(]").unwrap();
    let re_func = Regex::new(r"^(\s*)(?:async\s+)?def\s+(test_\w+)\s*\(").unwrap();

    let mut current_class: Option<(String, usize)> = None;

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = re_class.captures(line) {
            let name = cap[1].to_string();
            let start = i + 1;
            let end = find_python_indent_end(&lines, i);
            current_class = Some((name.clone(), i));
            items.push(TestItem {
                id: format!("suite:{}-{}", start, end),
                name,
                kind: "suite".to_string(),
                level: 0,
                start_line: start,
                end_line: end,
            });
        } else if let Some(cap) = re_func.captures(line) {
            let indent = cap[1].len();
            let name = cap[2].to_string();
            let level = if indent > 0 && current_class.is_some() {
                1
            } else {
                0
            };
            let start = i + 1;
            let end = find_python_indent_end(&lines, i);
            items.push(TestItem {
                id: format!("test:{}-{}", start, end),
                name,
                kind: "test".to_string(),
                level,
                start_line: start,
                end_line: end,
            });
        } else if !line.starts_with(' ') && !line.starts_with('\t') {
            current_class = None;
        }
    }

    ("pytest".to_string(), items)
}

// ─── Rust (#[test]) ───────────────────────────────────────────────────────────

fn parse_rust_test(content: &str) -> (String, Vec<TestItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re_mod = Regex::new(r"^(\s*)(?:pub\s+)?mod\s+(\w*test\w*)\s*\{").unwrap();
    let re_fn = Regex::new(r"^(\s*)(?:async\s+)?fn\s+(\w+)\s*\(").unwrap();
    let mut in_test_attr = false;
    let mut mod_level = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();

        if t == "#[test]" || t == "#[tokio::test]" || t.contains("#[test]") {
            in_test_attr = true;
            continue;
        }

        if let Some(cap) = re_mod.captures(line) {
            let name = cap[2].to_string();
            mod_level += 1;
            let start = i + 1;
            let end = find_block_end_lines(&lines, i);
            items.push(TestItem {
                id: format!("suite:{}-{}", start, end),
                name,
                kind: "suite".to_string(),
                level: mod_level.saturating_sub(1),
                start_line: start,
                end_line: end,
            });
            continue;
        }

        if in_test_attr {
            if let Some(cap) = re_fn.captures(line) {
                let name = cap[2].to_string();
                let start = i + 1;
                let end = find_block_end_lines(&lines, i);
                items.push(TestItem {
                    id: format!("test:{}-{}", start, end),
                    name,
                    kind: "test".to_string(),
                    level: mod_level,
                    start_line: start,
                    end_line: end,
                });
            }
            in_test_attr = false;
        }
    }

    ("cargo".to_string(), items)
}

// ─── Go (func TestXxx) ────────────────────────────────────────────────────────

fn parse_go_test_skeleton(content: &str) -> (String, Vec<TestItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re = Regex::new(r"^func\s+(Test\w+|Benchmark\w+|Example\w+)\s*\(").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = re.captures(line) {
            let name = cap[1].to_string();
            let kind = if name.starts_with("Benchmark") {
                "benchmark"
            } else if name.starts_with("Example") {
                "example"
            } else {
                "test"
            }
            .to_string();
            let start = i + 1;
            let end = find_block_end_lines(&lines, i);
            items.push(TestItem {
                id: format!("{}:{}-{}", kind, start, end),
                name,
                kind,
                level: 0,
                start_line: start,
                end_line: end,
            });
        }
    }

    ("go".to_string(), items)
}

// ─── Java / Kotlin (@Test) ────────────────────────────────────────────────────

fn parse_java_test(content: &str) -> (String, Vec<TestItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re_class = Regex::new(r"^(?:public\s+)?class\s+(\w+Test\w*)\s*").unwrap();
    let re_fn =
        Regex::new(r"^\s+(?:public\s+|protected\s+)?(?:void|[\w<>]+)\s+(\w+)\s*\(").unwrap();
    let mut in_test_ann = false;
    let mut class_level = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t == "@Test" || t.starts_with("@Test(") {
            in_test_ann = true;
            continue;
        }

        if let Some(cap) = re_class.captures(line) {
            class_level += 1;
            let name = cap[1].to_string();
            let start = i + 1;
            let end = find_block_end_lines(&lines, i);
            items.push(TestItem {
                id: format!("suite:{}-{}", start, end),
                name,
                kind: "suite".to_string(),
                level: class_level.saturating_sub(1),
                start_line: start,
                end_line: end,
            });
        } else if in_test_ann {
            if let Some(cap) = re_fn.captures(line) {
                let name = cap[1].to_string();
                let start = i + 1;
                let end = find_block_end_lines(&lines, i);
                items.push(TestItem {
                    id: format!("test:{}-{}", start, end),
                    name,
                    kind: "test".to_string(),
                    level: class_level,
                    start_line: start,
                    end_line: end,
                });
            }
            in_test_ann = false;
        }
    }

    ("junit".to_string(), items)
}

// ─── RSpec (Ruby) ─────────────────────────────────────────────────────────────

fn parse_rspec(content: &str) -> (String, Vec<TestItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re =
        Regex::new(r#"^(\s*)(describe|context|it|specify|subject)\s+["'`](.+?)["'`]"#).unwrap();
    let mut depth_stack: Vec<usize> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = re.captures(line) {
            let indent = cap[1].len();
            let keyword = &cap[2];
            let name = cap[3].to_string();

            while let Some(&last) = depth_stack.last() {
                if indent <= last {
                    depth_stack.pop();
                } else {
                    break;
                }
            }
            let level = depth_stack.len();
            let kind = if keyword == "describe" || keyword == "context" {
                "describe"
            } else {
                "it"
            }
            .to_string();
            let start = i + 1;
            let end = find_block_end_lines(&lines, i);
            let id = format!("{}:{}-{}", kind, start, end);

            if kind == "describe" {
                depth_stack.push(indent);
            }
            items.push(TestItem {
                id,
                name,
                kind,
                level,
                start_line: start,
                end_line: end,
            });
        }
    }

    ("rspec".to_string(), items)
}

// ─── Block-end helpers ────────────────────────────────────────────────────────

fn find_block_end_lines(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    let mut found = false;
    for (j, line) in lines[start..].iter().enumerate() {
        let opens = line.chars().filter(|&c| c == '{').count() as i32;
        let closes = line.chars().filter(|&c| c == '}').count() as i32;
        depth += opens - closes;
        if opens > 0 {
            found = true;
        }
        if found && depth <= 0 {
            return start + j + 1; // 1-indexed
        }
    }
    lines.len()
}

fn find_python_indent_end(lines: &[&str], start: usize) -> usize {
    let base_indent = lines[start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    for (j, line) in lines[start + 1..].iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent <= base_indent {
            return start + j + 1;
        }
    }
    lines.len()
}
