use ignore::WalkBuilder;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::security::safe_path;
use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadCodeSkeletonParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Include block-level constructs (if/for/etc) - default false")]
    pub include_blocks: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkeletonItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct ReadCodeSkeletonResult {
    pub language: String,
    pub skeleton: Vec<SkeletonItem>,
    pub token_count: usize,
}

pub fn read_code_skeleton(root: &Path, params: ReadCodeSkeletonParams) -> anyhow::Result<ReadCodeSkeletonResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let (language, skeleton) = parse_skeleton(&content, ext);
    let json = serde_json::to_string(&skeleton).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCodeSkeletonResult { language, skeleton, token_count })
}

fn parse_skeleton(content: &str, ext: &str) -> (String, Vec<SkeletonItem>) {
    // tree-sitter first, regex fallback
    if let Some((lang_name, items)) = try_parse_ts(content, ext) {
        if !items.is_empty() {
            return (lang_name.to_string(), items);
        }
    }
    let lang_name = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "h" => "cpp",
        "java" => "java",
        "rb" => "ruby",
        _ => "unknown",
    };
    let items = match ext {
        "rs" => parse_rust(content),
        "py" => parse_python(content),
        "js" | "jsx" | "ts" | "tsx" => parse_js_ts(content),
        "go" => parse_go(content),
        _ => parse_generic(content),
    };
    (lang_name.to_string(), items)
}

// ── tree-sitter ────────────────────────────────────────────────────────────

const RUST_QUERY: &str = "
(function_item name: (identifier) @name) @definition.function
(struct_item name: (type_identifier) @name) @definition.struct
(enum_item name: (type_identifier) @name) @definition.enum
(trait_item name: (type_identifier) @name) @definition.trait
(impl_item type: (_) @name) @definition.impl
(mod_item name: (identifier) @name) @definition.mod
(const_item name: (identifier) @name) @definition.const
";

const PYTHON_QUERY: &str = "
(function_definition name: (identifier) @name) @definition.function
(class_definition name: (identifier) @name) @definition.class
";

const JS_QUERY: &str = "
(function_declaration name: (identifier) @name) @definition.function
(generator_function_declaration name: (identifier) @name) @definition.function
(class_declaration name: (identifier) @name) @definition.class
(method_definition name: (property_identifier) @name) @definition.method
";

const TS_QUERY: &str = "
(function_declaration name: (identifier) @name) @definition.function
(class_declaration name: (identifier) @name) @definition.class
(method_definition name: (property_identifier) @name) @definition.method
(interface_declaration name: (type_identifier) @name) @definition.interface
(type_alias_declaration name: (type_identifier) @name) @definition.type
";

const GO_QUERY: &str = "
(function_declaration name: (identifier) @name) @definition.function
(method_declaration name: (field_identifier) @name) @definition.method
(type_declaration (type_spec name: (type_identifier) @name)) @definition.type
";

const CPP_QUERY: &str = "
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition.function
(function_definition declarator: (function_declarator declarator: (qualified_identifier name: (identifier) @name))) @definition.method
(class_specifier name: (type_identifier) @name) @definition.class
(struct_specifier name: (type_identifier) @name) @definition.struct
(enum_specifier name: (type_identifier) @name) @definition.enum
(namespace_definition name: (namespace_identifier) @name) @definition.namespace
";

const JAVA_QUERY: &str = "
(method_declaration name: (identifier) @name) @definition.method
(class_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(enum_declaration name: (identifier) @name) @definition.enum
(record_declaration name: (identifier) @name) @definition.record
(constructor_declaration name: (identifier) @name) @definition.constructor
";

const RUBY_QUERY: &str = "
(method name: (identifier) @name) @definition.method
(singleton_method name: (identifier) @name) @definition.method
(class name: (constant) @name) @definition.class
(module name: (constant) @name) @definition.module
";

const CSHARP_QUERY: &str = "
(class_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(struct_declaration name: (identifier) @name) @definition.struct
(enum_declaration name: (identifier) @name) @definition.enum
(record_declaration name: (identifier) @name) @definition.record
(method_declaration name: (identifier) @name) @definition.method
(constructor_declaration name: (identifier) @name) @definition.constructor
(namespace_declaration name: (identifier) @name) @definition.namespace
";

const PHP_QUERY: &str = "
(function_definition name: (name) @name) @definition.function
(method_declaration name: (name) @name) @definition.method
(class_declaration name: (name) @name) @definition.class
(interface_declaration name: (name) @name) @definition.interface
(trait_declaration name: (name) @name) @definition.trait
(namespace_definition name: (namespace_name) @name) @definition.namespace
";

const KOTLIN_QUERY: &str = "
(function_declaration (simple_identifier) @name) @definition.function
(class_declaration (type_identifier) @name) @definition.class
(object_declaration (type_identifier) @name) @definition.object
";

const SWIFT_QUERY: &str = "
(function_declaration name: (simple_identifier) @name) @definition.function
(class_declaration name: (type_identifier) @name) @definition.class
(struct_declaration name: (type_identifier) @name) @definition.struct
(enum_declaration name: (type_identifier) @name) @definition.enum
(protocol_declaration name: (type_identifier) @name) @definition.protocol
";

const LUA_QUERY: &str = "
(function_declaration name: (identifier) @name) @definition.function
(local_function name: (identifier) @name) @definition.function
";

pub fn ts_setup(ext: &str) -> Option<(tree_sitter::Language, &'static str, &'static str)> {
    match ext {
        "rs" => Some((tree_sitter_rust::LANGUAGE.into(), RUST_QUERY, "rust")),
        "py" => Some((tree_sitter_python::LANGUAGE.into(), PYTHON_QUERY, "python")),
        "js" | "jsx" => Some((tree_sitter_javascript::LANGUAGE.into(), JS_QUERY, "javascript")),
        "ts" => Some((tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), TS_QUERY, "typescript")),
        "tsx" => Some((tree_sitter_typescript::LANGUAGE_TSX.into(), TS_QUERY, "tsx")),
        "go" => Some((tree_sitter_go::LANGUAGE.into(), GO_QUERY, "go")),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "h" => Some((tree_sitter_cpp::LANGUAGE.into(), CPP_QUERY, "cpp")),
        "java" => Some((tree_sitter_java::LANGUAGE.into(), JAVA_QUERY, "java")),
        "rb" => Some((tree_sitter_ruby::LANGUAGE.into(), RUBY_QUERY, "ruby")),
        "cs" => Some((tree_sitter_c_sharp::LANGUAGE.into(), CSHARP_QUERY, "csharp")),
        "php" => Some((tree_sitter_php::LANGUAGE_PHP.into(), PHP_QUERY, "php")),
        "kt" | "kts" => Some((tree_sitter_kotlin_ng::LANGUAGE.into(), KOTLIN_QUERY, "kotlin")),
        "swift" => Some((tree_sitter_swift::LANGUAGE.into(), SWIFT_QUERY, "swift")),
        "lua" => Some((tree_sitter_lua::LANGUAGE.into(), LUA_QUERY, "lua")),
        _ => None,
    }
}

fn extract_node_signature(content: &str, node: &tree_sitter::Node, ext: &str) -> String {
    let node_text = &content[node.byte_range()];
    let delimiter = if ext == "py" { ':' } else { '{' };
    let sig = if let Some(pos) = node_text.find(delimiter) {
        &node_text[..pos]
    } else {
        node_text
    };
    // Normalize whitespace so multi-line signatures become a single line
    sig.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

fn try_parse_ts(content: &str, ext: &str) -> Option<(&'static str, Vec<SkeletonItem>)> {
    use streaming_iterator::StreamingIterator;

    let (lang, query_str, lang_name) = ts_setup(ext)?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang).ok()?;
    let tree = parser.parse(content, None)?;
    let root = tree.root_node();

    let query = tree_sitter::Query::new(&lang, query_str).ok()?;
    // Precompute to avoid borrow conflict with the match iterator
    let cap_names: Vec<String> = query.capture_names().iter().map(|s| s.to_string()).collect();

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut items = Vec::new();

    // tree-sitter 0.22+ uses StreamingIterator protocol (not std Iterator)
    let mut matches = cursor.matches(&query, root, content.as_bytes());
    loop {
        matches.advance();
        let mat = match matches.get() {
            Some(m) => m,
            None => break,
        };

        let mut def_node: Option<tree_sitter::Node> = None;
        let mut name_text: Option<String> = None;
        let mut kind = String::new();

        for cap in mat.captures {
            let cap_name = cap_names.get(cap.index as usize).map(|s| s.as_str()).unwrap_or("");
            if let Some(k) = cap_name.strip_prefix("definition.") {
                def_node = Some(cap.node);
                kind = k.to_string();
            } else if cap_name == "name" {
                name_text = Some(content[cap.node.byte_range()].to_string());
            }
        }

        if let Some(node) = def_node {
            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;
            let signature = extract_node_signature(content, &node, ext);
            let name = name_text.unwrap_or_else(|| kind.clone());
            items.push(SkeletonItem {
                id: format!("{}:{}-{}", kind, start_line, end_line),
                kind,
                name,
                signature,
                start_line,
                end_line,
            });
        }
    }

    Some((lang_name, items))
}

// ── regex fallbacks ────────────────────────────────────────────────────────

fn parse_rust(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let patterns = [
        (Regex::new(r"^(\s*)(pub\s+)?(async\s+)?fn\s+(\w+)").unwrap(), "function"),
        (Regex::new(r"^(\s*)(pub\s+)?struct\s+(\w+)").unwrap(), "struct"),
        (Regex::new(r"^(\s*)(pub\s+)?enum\s+(\w+)").unwrap(), "enum"),
        (Regex::new(r"^(\s*)(pub\s+)?trait\s+(\w+)").unwrap(), "trait"),
        (Regex::new(r"^(\s*)impl(\s*<[^>]*>)?\s+(\w+)").unwrap(), "impl"),
        (Regex::new(r"^(\s*)(pub\s+)?mod\s+(\w+)").unwrap(), "mod"),
    ];

    for (i, line) in lines.iter().enumerate() {
        for (re, kind) in &patterns {
            if let Some(cap) = re.captures(line) {
                let name = cap.get(cap.len() - 1).map(|m| m.as_str()).unwrap_or("").to_string();
                if name.is_empty() { continue; }
                let end_line = find_block_end(&lines, i);
                items.push(SkeletonItem {
                    id: format!("{}:{}-{}", kind, i + 1, end_line),
                    kind: kind.to_string(),
                    name,
                    signature: line.trim().to_string(),
                    start_line: i + 1,
                    end_line,
                });
                break;
            }
        }
    }
    items
}

fn parse_python(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let fn_re = Regex::new(r"^(\s*)(async\s+)?def\s+(\w+)\s*\(([^)]*)\)").unwrap();
    let class_re = Regex::new(r"^(\s*)class\s+(\w+)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            let name = cap.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_python_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("function:{}-{}", i + 1, end_line),
                kind: "function".to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        } else if let Some(cap) = class_re.captures(line) {
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_python_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("class:{}-{}", i + 1, end_line),
                kind: "class".to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        }
    }
    items
}

fn parse_js_ts(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let patterns = [
        (Regex::new(r"^(\s*)(export\s+)?(default\s+)?(async\s+)?function\s+(\w+)").unwrap(), "function"),
        (Regex::new(r"^(\s*)(export\s+)?(abstract\s+)?class\s+(\w+)").unwrap(), "class"),
        (Regex::new(r"^(\s*)(export\s+)?(const|let|var)\s+(\w+)\s*=\s*(async\s*)?\(").unwrap(), "arrow_fn"),
        (Regex::new(r"^(\s*)(export\s+)?interface\s+(\w+)").unwrap(), "interface"),
        (Regex::new(r"^(\s*)(export\s+)?type\s+(\w+)").unwrap(), "type"),
    ];

    for (i, line) in lines.iter().enumerate() {
        for (re, kind) in &patterns {
            if let Some(cap) = re.captures(line) {
                let name = cap.get(cap.len() - 1).map(|m| m.as_str()).unwrap_or("").to_string();
                if name.is_empty() || ["async", "function", "class", "interface", "type"].contains(&name.as_str()) {
                    continue;
                }
                let end_line = find_block_end(&lines, i);
                items.push(SkeletonItem {
                    id: format!("{}:{}-{}", kind, i + 1, end_line),
                    kind: kind.to_string(),
                    name,
                    signature: line.trim().to_string(),
                    start_line: i + 1,
                    end_line,
                });
                break;
            }
        }
    }
    items
}

fn parse_go(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let fn_re = Regex::new(r"^func\s+(\([\w\s*]+\)\s+)?(\w+)\s*\(").unwrap();
    let type_re = Regex::new(r"^type\s+(\w+)\s+(struct|interface)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_block_end(&lines, i);
            let kind = if cap.get(1).is_some() { "method" } else { "function" };
            items.push(SkeletonItem {
                id: format!("{}:{}-{}", kind, i + 1, end_line),
                kind: kind.to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        } else if let Some(cap) = type_re.captures(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let kind = cap.get(2).map(|m| m.as_str()).unwrap_or("struct");
            let end_line = find_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("{}:{}-{}", kind, i + 1, end_line),
                kind: kind.to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        }
    }
    items
}

fn parse_generic(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();
    let fn_re = Regex::new(r"(?:function|def|fn|func)\s+(\w+)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("function:{}-{}", i + 1, end_line),
                kind: "function".to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        }
    }
    items
}

// ── block-end helpers ──────────────────────────────────────────────────────

/// Finds the closing line of a `{...}` block, skipping strings and comments.
fn find_block_end(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    let mut in_block_comment = false;

    for (i, line) in lines[start..].iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let n = chars.len();
        let mut j = 0;

        while j < n {
            if in_block_comment {
                if j + 1 < n && chars[j] == '*' && chars[j + 1] == '/' {
                    in_block_comment = false;
                    j += 2;
                } else {
                    j += 1;
                }
                continue;
            }

            // Line comment – rest of line is safe to skip
            if j + 1 < n && chars[j] == '/' && chars[j + 1] == '/' {
                break;
            }

            // Block comment start
            if j + 1 < n && chars[j] == '/' && chars[j + 1] == '*' {
                in_block_comment = true;
                j += 2;
                continue;
            }

            // String literal (handles \" escapes)
            if chars[j] == '"' || chars[j] == '\'' {
                let quote = chars[j];
                j += 1;
                while j < n {
                    if chars[j] == '\\' {
                        j += 2;
                    } else if chars[j] == quote {
                        j += 1;
                        break;
                    } else {
                        j += 1;
                    }
                }
                continue;
            }

            match chars[j] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth <= 0 && i > 0 {
                        return start + i + 1;
                    }
                }
                _ => {}
            }
            j += 1;
        }
    }
    lines.len()
}

/// Finds the end of a Python indent-block, handling multiline signatures and comments.
fn find_python_block_end(lines: &[&str], start: usize) -> usize {
    let base_indent = lines[start].len() - lines[start].trim_start().len();
    // If the definition line already ends with ':', the signature is single-line.
    let mut past_header = lines[start].trim_end().ends_with(':');
    let mut found_body = false;

    for (i, line) in lines[start + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if !past_header {
            // Still inside multiline function/class signature
            if trimmed.ends_with(':') {
                past_header = true;
            }
            continue;
        }

        if !found_body {
            if indent > base_indent {
                found_body = true;
            }
        } else if indent <= base_indent {
            return start + i + 1;
        }
    }
    lines.len()
}

// ── read_code_body ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadCodeBodyParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "List of skeleton IDs from read_code_skeleton (e.g. 'function:10-25')")]
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CodeBodyItem {
    pub id: String,
    pub content: String,
}

pub struct ReadCodeBodyResult {
    pub items: Vec<CodeBodyItem>,
    pub token_count: usize,
}

pub fn read_code_body(root: &Path, params: ReadCodeBodyParams) -> anyhow::Result<ReadCodeBodyResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut items = Vec::new();
    for id in &params.ids {
        let parts: Vec<&str> = id.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            let range: Vec<&str> = parts[0].splitn(2, '-').collect();
            if range.len() == 2 {
                if let (Ok(start), Ok(end)) = (range[0].parse::<usize>(), range[1].parse::<usize>()) {
                    let start = start.saturating_sub(1);
                    let end = end.min(lines.len());
                    let body = lines[start..end].join("\n");
                    items.push(CodeBodyItem { id: id.clone(), content: body });
                    continue;
                }
            }
        }
        items.push(CodeBodyItem { id: id.clone(), content: format!("Error: invalid id '{}'", id) });
    }

    let json = serde_json::to_string(&items).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCodeBodyResult { items, token_count })
}

// ─── read_symbol_usages ───────────────────────────────────────────────────────

const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go",
    "cpp", "cc", "cxx", "hpp", "hh", "h", "java", "rb", "c",
    "cs", "php",
];

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadSymbolUsagesParams {
    #[schemars(description = "Symbol name to search for (function, struct, class, variable, etc.)")]
    pub symbol: String,
    #[schemars(description = "Restrict search to this file or directory (root-relative). Omit to search entire workspace.")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolUsage {
    pub path: String,
    pub line: usize,
    pub content: String,
    pub context: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadSymbolUsagesResult {
    pub symbol: String,
    pub usages: Vec<SymbolUsage>,
    pub total: usize,
    pub truncated: bool,
    pub token_count: usize,
}

pub fn read_symbol_usages(root: &Path, params: ReadSymbolUsagesParams) -> anyhow::Result<ReadSymbolUsagesResult> {
    if params.symbol.is_empty() {
        anyhow::bail!("symbol は空にできません");
    }

    let start = if let Some(ref p) = params.path {
        safe_path(root, p)?
    } else {
        root.to_path_buf()
    };

    let pattern = format!(r"\b{}\b", regex::escape(&params.symbol));
    let re = Regex::new(&pattern)
        .map_err(|e| anyhow::anyhow!("Invalid symbol '{}': {}", params.symbol, e))?;

    let mut usages: Vec<SymbolUsage> = Vec::new();
    const MAX_RESULTS: usize = 100;
    let mut truncated = false;

    'outer: for entry in WalkBuilder::new(&start)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() { continue; }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !CODE_EXTENSIONS.contains(&ext) { continue; }

        let Ok(content) = std::fs::read_to_string(path) else { continue };
        let lines: Vec<&str> = content.lines().collect();
        let rel = path.strip_prefix(root).unwrap_or(path)
            .to_string_lossy().replace('\\', "/");

        for (i, line) in lines.iter().enumerate() {
            if !re.is_match(line) { continue; }
            let mut context = Vec::new();
            if i > 0 { context.push(format!("{}: {}", i, lines[i - 1])); }
            if i + 1 < lines.len() { context.push(format!("{}: {}", i + 2, lines[i + 1])); }
            usages.push(SymbolUsage { path: rel.clone(), line: i + 1, content: line.to_string(), context });
            if usages.len() >= MAX_RESULTS {
                truncated = true;
                break 'outer;
            }
        }
    }

    let total = usages.len();
    let json = serde_json::to_string(&usages).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadSymbolUsagesResult { symbol: params.symbol, usages, total, truncated, token_count })
}

// ─── read_type_skeleton ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadTypeSkeletonParams {
    #[schemars(description = "Root-relative path to a TypeScript (.ts/.tsx) or Go (.go) file.")]
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct TypeItem {
    pub id: String,
    pub name: String,
    pub kind: String,    // "interface" | "type" | "enum" | "struct"
    pub fields: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadTypeSkeletonResult {
    pub path: String,
    pub language: String,
    pub types: Vec<TypeItem>,
    pub token_count: usize,
}

pub fn read_type_skeleton(root: &Path, params: ReadTypeSkeletonParams) -> anyhow::Result<ReadTypeSkeletonResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let rel = path.strip_prefix(root).unwrap_or(&path)
        .to_string_lossy().replace('\\', "/");

    let (language, types) = match ext {
        "ts" | "tsx" => parse_ts_types(&content),
        "go" => parse_go_types(&content),
        "rs" => parse_rust_types(&content),
        _ => anyhow::bail!("未対応の拡張子: {ext}。.ts / .tsx / .go / .rs のみ対応"),
    };

    let json = serde_json::to_string(&types).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadTypeSkeletonResult { path: rel, language, types, token_count })
}

fn parse_ts_types(content: &str) -> (String, Vec<TypeItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re = Regex::new(
        r"^(?:export\s+)?(?:declare\s+)?(interface|type|enum)\s+(\w+)"
    ).unwrap();
    let re_field_ts = Regex::new(r"^\s+(?:readonly\s+)?(\w+\??)\s*[?:]").unwrap();

    let mut i = 0;
    while i < lines.len() {
        if let Some(cap) = re.captures(lines[i]) {
            let kind = cap[1].to_string();
            let name = cap[2].to_string();
            let start_line = i + 1;

            // For `type Foo = ...` without braces (union, intersection, alias)
            if kind == "type" && !lines[i].contains('{') {
                let end_line = i + 1;
                let value = lines[i].splitn(2, '=').nth(1)
                    .unwrap_or("").trim().to_string();
                items.push(TypeItem {
                    id: format!("type:{}-{}", start_line, end_line),
                    name, kind, fields: vec![value], start_line, end_line,
                });
                i += 1;
                continue;
            }

            // Block type: find closing brace
            let mut depth: i32 = 0;
            let mut j = i;
            let mut fields = Vec::new();
            let mut found_open = false;

            while j < lines.len() {
                let l = lines[j].trim();
                let opens  = l.chars().filter(|&c| c == '{').count() as i32;
                let closes = l.chars().filter(|&c| c == '}').count() as i32;
                depth += opens - closes;
                if opens > 0 { found_open = true; }

                if found_open && depth == 1 && j > i {
                    if let Some(fc) = re_field_ts.captures(lines[j]) {
                        fields.push(fc[0].trim().trim_end_matches(|c| c == ',' || c == ';').to_string());
                    }
                }
                if found_open && depth <= 0 { break; }
                j += 1;
            }

            let end_line = (j + 1).min(lines.len());
            items.push(TypeItem {
                id: format!("{}:{}-{}", kind, start_line, end_line),
                name, kind, fields, start_line, end_line,
            });
            i = j + 1;
            continue;
        }
        i += 1;
    }

    ("typescript".to_string(), items)
}

fn parse_go_types(content: &str) -> (String, Vec<TypeItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re_type = Regex::new(r"^type\s+(\w+)\s+(struct|interface|\S+)").unwrap();
    let re_field = Regex::new(r"^\s+(\w+)\s+[\w\*\[\]]+").unwrap();

    let mut i = 0;
    while i < lines.len() {
        if let Some(cap) = re_type.captures(lines[i]) {
            let name = cap[1].to_string();
            let raw_kind = &cap[2];
            let kind = if raw_kind == "struct" { "struct" }
                else if raw_kind == "interface" { "interface" }
                else { "type" }.to_string();
            let start_line = i + 1;

            if !lines[i].contains('{') {
                // simple type alias
                items.push(TypeItem {
                    id: format!("type:{}-{}", start_line, start_line),
                    name, kind, fields: vec![raw_kind.to_string()], start_line, end_line: start_line,
                });
                i += 1;
                continue;
            }

            let mut depth: i32 = 0;
            let mut j = i;
            let mut fields = Vec::new();

            while j < lines.len() {
                let l = lines[j].trim();
                let opens  = l.chars().filter(|&c| c == '{').count() as i32;
                let closes = l.chars().filter(|&c| c == '}').count() as i32;
                depth += opens - closes;

                if depth == 1 && j > i {
                    if let Some(fc) = re_field.captures(lines[j]) {
                        fields.push(fc[0].trim().to_string());
                    }
                }
                if depth <= 0 && opens + closes > 0 { break; }
                j += 1;
            }

            let end_line = (j + 1).min(lines.len());
            items.push(TypeItem {
                id: format!("{}:{}-{}", kind, start_line, end_line),
                name, kind, fields, start_line, end_line,
            });
            i = j + 1;
            continue;
        }
        i += 1;
    }

    ("go".to_string(), items)
}

fn parse_rust_types(content: &str) -> (String, Vec<TypeItem>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let re = Regex::new(r"^(?:pub(?:\(\w+\))?\s+)?(?:pub\s+)?(struct|enum|trait|type)\s+(\w+)").unwrap();
    let re_field = Regex::new(r"^\s+(?:pub(?:\(\w+\))?\s+)?(\w+)\s*:").unwrap();

    let mut i = 0;
    while i < lines.len() {
        if let Some(cap) = re.captures(lines[i]) {
            let kind = cap[1].to_string();
            let name = cap[2].to_string();
            let start_line = i + 1;

            if !lines[i].contains('{') && kind == "type" {
                items.push(TypeItem {
                    id: format!("type:{}-{}", start_line, start_line),
                    name, kind, fields: Vec::new(), start_line, end_line: start_line,
                });
                i += 1;
                continue;
            }

            let mut depth: i32 = 0;
            let mut j = i;
            let mut fields = Vec::new();
            let mut found_open = false;

            while j < lines.len() {
                let l = lines[j].trim();
                let opens  = l.chars().filter(|&c| c == '{').count() as i32;
                let closes = l.chars().filter(|&c| c == '}').count() as i32;
                depth += opens - closes;
                if opens > 0 { found_open = true; }

                if found_open && depth == 1 && j > i {
                    if let Some(fc) = re_field.captures(lines[j]) {
                        fields.push(fc[0].trim().trim_end_matches(',').to_string());
                    }
                }
                if found_open && depth <= 0 { break; }
                j += 1;
            }

            let end_line = (j + 1).min(lines.len());
            items.push(TypeItem {
                id: format!("{}:{}-{}", kind, start_line, end_line),
                name, kind, fields, start_line, end_line,
            });
            i = j + 1;
            continue;
        }
        i += 1;
    }

    ("rust".to_string(), items)
}

// ─── read_call_graph ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadCallGraphParams {
    #[schemars(description = "Root-relative path to the code file.")]
    pub path: String,
    #[schemars(description = "Function ID from read_code_skeleton (e.g. 'function:34-42').")]
    pub function_id: String,
    #[schemars(description = "Cross-file trace depth (0=single file only, 1-5=follow calls across files). Default: 0.")]
    pub depth: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct ReadCallGraphResult {
    pub function: String,
    pub file: String,
    pub calls: Vec<String>,
    pub called_by_in_file: Vec<String>,
    pub token_count: usize,
    // cross-file fields (populated only when depth >= 1)
    pub cross_file_callees: Vec<serde_json::Value>,
    pub cross_file_callers: Vec<serde_json::Value>,
}

pub fn read_call_graph(root: &Path, params: ReadCallGraphParams) -> anyhow::Result<ReadCallGraphResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();
    let rel = file_path.strip_prefix(root).unwrap_or(&file_path)
        .to_string_lossy().replace('\\', "/");

    // Parse the function_id to get line range
    let parts: Vec<&str> = params.function_id.rsplitn(2, ':').collect();
    anyhow::ensure!(parts.len() == 2, "無効な function_id 形式: {}", params.function_id);
    let range: Vec<&str> = parts[0].splitn(2, '-').collect();
    anyhow::ensure!(range.len() == 2, "無効な function_id 形式: {}", params.function_id);
    let start: usize = range[0].parse().map_err(|_| anyhow::anyhow!("無効な開始行"))?;
    let end: usize   = range[1].parse().map_err(|_| anyhow::anyhow!("無効な終了行"))?;

    let from = start.saturating_sub(1);
    let to = end.min(lines.len());

    let fn_name = extract_fn_name(lines[from]);
    let body = lines[from..to].join("\n");
    let calls = extract_calls(&body, fn_name.as_deref());

    let called_by = if let Some(ref name) = fn_name {
        find_callers(&lines, name, from, to)
    } else {
        Vec::new()
    };

    // Cross-file extension when depth >= 1
    let depth = params.depth.unwrap_or(0).min(5);
    let (cross_file_callees, cross_file_callers) = if depth >= 1 {
        let callees = find_cross_file_definitions(root, &calls);
        let callers = if let Some(ref name) = fn_name {
            find_cross_file_callers(root, name, &rel)
        } else {
            Vec::new()
        };
        (callees, callers)
    } else {
        (Vec::new(), Vec::new())
    };

    let result_json = serde_json::json!({
        "calls": calls,
        "called_by_in_file": called_by,
        "cross_file_callees": cross_file_callees,
        "cross_file_callers": cross_file_callers,
    });
    let token_count = estimate_tokens(&result_json.to_string());

    Ok(ReadCallGraphResult {
        function: fn_name.unwrap_or_else(|| params.function_id.clone()),
        file: rel,
        calls,
        called_by_in_file: called_by,
        token_count,
        cross_file_callees,
        cross_file_callers,
    })
}

/// Find files that define any of the given function names (cross-file callees).
fn find_cross_file_definitions(root: &Path, fn_names: &[String]) -> Vec<serde_json::Value> {
    if fn_names.is_empty() { return Vec::new(); }
    let fn_re = Regex::new(
        r"(?:fn|func|def|function|class)\s+(\w+)\s*[(<]"
    ).unwrap();
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut found_names = std::collections::HashSet::new();

    for entry in WalkBuilder::new(root)
        .hidden(false).git_ignore(true).git_global(false).git_exclude(false)
        .build().flatten()
    {
        let path = entry.path();
        if path.is_dir() { continue; }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !CODE_EXTENSIONS.contains(&ext) { continue; }
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        let rel = path.strip_prefix(root).unwrap_or(path)
            .to_string_lossy().replace('\\', "/");

        for (i, line) in content.lines().enumerate() {
            if let Some(cap) = fn_re.captures(line) {
                let name = cap[1].to_string();
                if fn_names.contains(&name) && found_names.insert(name.clone()) {
                    results.push(serde_json::json!({
                        "function": name,
                        "file": rel,
                        "line": i + 1,
                    }));
                }
            }
        }
        if found_names.len() >= fn_names.len() { break; }
    }
    results
}

/// Find functions in other files that call `fn_name`.
fn find_cross_file_callers(root: &Path, fn_name: &str, self_file: &str) -> Vec<serde_json::Value> {
    let call_re = Regex::new(&format!(r"\b{}\s*\(", regex::escape(fn_name))).unwrap();
    let fn_re = Regex::new(r"(?:fn|func|def|function)\s+(\w+)\s*[(<]").unwrap();
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in WalkBuilder::new(root)
        .hidden(false).git_ignore(true).git_global(false).git_exclude(false)
        .build().flatten()
    {
        let path = entry.path();
        if path.is_dir() { continue; }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !CODE_EXTENSIONS.contains(&ext) { continue; }
        let rel = path.strip_prefix(root).unwrap_or(path)
            .to_string_lossy().replace('\\', "/");
        if rel == self_file { continue; } // skip the source file

        let Ok(content) = std::fs::read_to_string(path) else { continue };
        if !content.contains(fn_name) { continue; }

        let mut current_fn: Option<String> = None;
        for (i, line) in content.lines().enumerate() {
            if let Some(cap) = fn_re.captures(line) {
                current_fn = Some(cap[1].to_string());
            }
            if call_re.is_match(line) {
                if let Some(ref caller) = current_fn {
                    let key = format!("{}:{}", rel, caller);
                    if seen.insert(key) {
                        results.push(serde_json::json!({
                            "caller": caller,
                            "file": rel,
                            "line": i + 1,
                        }));
                    }
                }
            }
        }
        if results.len() >= 50 { break; }
    }
    results
}

fn extract_fn_name(line: &str) -> Option<String> {
    // Rust: pub fn foo( / async fn foo(
    let re_rust = Regex::new(r"fn\s+(\w+)\s*[<(]").unwrap();
    // JS/TS: function foo( / async function foo( / const foo = ( / foo(
    let re_js   = Regex::new(r"(?:function\s+|const\s+|let\s+|var\s+)(\w+)\s*[=(]").unwrap();
    // Python: def foo(
    let re_py   = Regex::new(r"def\s+(\w+)\s*\(").unwrap();
    // Go: func Foo(
    let re_go   = Regex::new(r"func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)\s*\(").unwrap();

    for re in &[re_rust, re_go, re_js, re_py] {
        if let Some(cap) = re.captures(line) {
            return Some(cap[1].to_string());
        }
    }
    None
}

fn extract_calls(body: &str, self_name: Option<&str>) -> Vec<String> {
    // Match: identifier( — but not control keywords
    let re = Regex::new(r"\b([a-zA-Z_]\w*)\s*\(").unwrap();
    let skip = &[
        "if", "while", "for", "match", "switch", "return", "typeof", "instanceof",
        "new", "await", "async", "yield", "delete", "void", "throw", "catch",
        "super", "assert", "print", "println", "eprintln", "format", "vec",
        "Some", "None", "Ok", "Err", "Box", "Arc", "Rc", "Mutex",
    ];

    let mut seen = std::collections::HashSet::new();
    let mut calls = Vec::new();

    for cap in re.captures_iter(body) {
        let name = cap[1].to_string();
        if skip.contains(&&*name) { continue; }
        if Some(name.as_str()) == self_name { continue; } // skip recursive self-call
        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) { continue; } // skip type constructors
        if seen.insert(name.clone()) {
            calls.push(name);
        }
    }

    calls
}

fn find_callers(lines: &[&str], fn_name: &str, fn_start: usize, fn_end: usize) -> Vec<String> {
    // Find other functions in the file that call fn_name
    let call_re = Regex::new(&format!(r"\b{}\s*\(", regex::escape(fn_name))).unwrap();
    let fn_re = Regex::new(
        r"(?:fn|func|def|function)\s+(\w+)\s*[(<]"
    ).unwrap();

    let mut current_fn: Option<String> = None;
    let mut current_fn_start = 0usize;
    let mut callers: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            current_fn = Some(cap[1].to_string());
            current_fn_start = i;
        }

        // Skip calls from within the function itself
        if i >= fn_start && i < fn_end { continue; }

        if call_re.is_match(line) {
            if let Some(ref caller) = current_fn {
                if current_fn_start < fn_start || current_fn_start >= fn_end {
                    callers.insert(caller.clone());
                }
            }
        }
    }

    callers.into_iter().collect()
}

// ─── read_interface_conformance ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadInterfaceConformanceParams {
    #[schemars(description = "Interface / trait / abstract class name to search for")]
    pub name: String,
    #[schemars(description = "Limit search to this file or directory (omit for whole workspace)")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConformanceEntry {
    pub type_name: String,
    pub file: String,
    pub line: usize,
    pub language: String,
}

#[derive(Debug, Serialize)]
pub struct ReadInterfaceConformanceResult {
    pub name: String,
    pub kind: String,
    pub implementations: Vec<ConformanceEntry>,
    pub total: usize,
    pub token_count: usize,
}

pub fn read_interface_conformance(
    root: &Path,
    params: ReadInterfaceConformanceParams,
) -> anyhow::Result<ReadInterfaceConformanceResult> {
    anyhow::ensure!(!params.name.is_empty(), "name は空にできません");

    let start = if let Some(ref p) = params.path {
        safe_path(root, p)?
    } else {
        root.to_path_buf()
    };

    let escaped = regex::escape(&params.name);

    // Per-language patterns: (ext, pattern, group_index_for_type_name, kind)
    struct Pattern {
        exts: &'static [&'static str],
        re_str: String,
    }

    let patterns = vec![
        // TypeScript: class Foo implements Bar
        Pattern {
            exts: &["ts", "tsx"],
            re_str: format!(r"(?:class|interface)\s+(\w+)(?:[^{{]*?)implements\s+(?:[^{{]*?)?{}", escaped),
        },
        // Rust: impl TraitName for TypeName
        Pattern {
            exts: &["rs"],
            re_str: format!(r"impl(?:<[^>]*>)?\s+{}\s+for\s+(\w+)", escaped),
        },
        // Java/Kotlin: class Foo implements Bar / extends Bar
        Pattern {
            exts: &["java", "kt", "kts"],
            re_str: format!(r"(?:class|object)\s+(\w+)(?:[^{{]*?)(?:implements|extends)\s+(?:[^{{]*?)?{}", escaped),
        },
        // Go: type assertion pattern — var _ Interface = (*Impl)(nil)
        Pattern {
            exts: &["go"],
            re_str: format!(r"var\s+_\s+{0}\s*=\s*(?:&?\s*(\w+)\{{|(?:\*)?(\w+)\(|(?:\*)?(\w+)\{{)", escaped),
        },
        // PHP: class Foo implements Bar
        Pattern {
            exts: &["php"],
            re_str: format!(r"class\s+(\w+)(?:[^{{]*?)implements\s+(?:[^{{]*?)?{}", escaped),
        },
        // C#: class Foo : Bar
        Pattern {
            exts: &["cs"],
            re_str: format!(r"(?:class|struct)\s+(\w+)(?:[^{{]*?):\s*(?:[^{{]*?)?{}", escaped),
        },
    ];

    let mut implementations = Vec::new();

    for pat in &patterns {
        let Ok(re) = Regex::new(&pat.re_str) else { continue };

        for entry in WalkBuilder::new(&start)
            .hidden(false).git_ignore(true).git_global(false).git_exclude(false)
            .build().flatten()
        {
            let path = entry.path();
            if path.is_dir() { continue; }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !pat.exts.contains(&ext) { continue; }

            let Ok(content) = std::fs::read_to_string(path) else { continue };
            if !content.contains(params.name.as_str()) { continue; }

            let rel = path.strip_prefix(root).unwrap_or(path)
                .to_string_lossy().replace('\\', "/");
            let language = match ext {
                "ts" | "tsx" => "typescript",
                "rs" => "rust",
                "java" => "java",
                "kt" | "kts" => "kotlin",
                "go" => "go",
                "php" => "php",
                "cs" => "c#",
                _ => ext,
            };

            for (line_idx, line) in content.lines().enumerate() {
                if let Some(cap) = re.captures(line) {
                    // Try groups 1, 2, 3 in order
                    let type_name = (1..=3)
                        .find_map(|g| cap.get(g).map(|m| m.as_str().trim().to_string()))
                        .unwrap_or_default();
                    if type_name.is_empty() { continue; }
                    implementations.push(ConformanceEntry {
                        type_name,
                        file: rel.clone(),
                        line: line_idx + 1,
                        language: language.to_string(),
                    });
                }
            }

            if implementations.len() >= 200 { break; }
        }
    }

    // Deduplicate by (type_name, file)
    let mut seen = std::collections::HashSet::new();
    implementations.retain(|e| seen.insert(format!("{}:{}", e.file, e.type_name)));
    implementations.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    let total = implementations.len();
    let kind = if params.name.starts_with(|c: char| c.is_uppercase()) { "interface" } else { "trait" };
    let json = serde_json::to_string(&implementations).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadInterfaceConformanceResult {
        name: params.name,
        kind: kind.to_string(),
        implementations,
        total,
        token_count,
    })
}
