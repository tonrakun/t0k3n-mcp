use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::Path;

/// Maps file extensions to language names.
fn ext_to_lang(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "py" | "pyw" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("javascript-jsx"),
        "ts" => Some("typescript"),
        "tsx" => Some("typescript-tsx"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hh" | "hpp" => Some("cpp"),
        "cs" => Some("c-sharp"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "swift" => Some("swift"),
        "kt" | "kts" => Some("kotlin"),
        "scala" => Some("scala"),
        "lua" => Some("lua"),
        "sh" | "bash" => Some("bash"),
        "ps1" | "psm1" => Some("powershell"),
        "json" => Some("json"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" | "sass" => Some("scss"),
        "md" | "markdown" => Some("markdown"),
        "sql" => Some("sql"),
        "zig" => Some("zig"),
        "ex" | "exs" => Some("elixir"),
        "erl" | "hrl" => Some("erlang"),
        "elm" => Some("elm"),
        "clj" | "cljs" => Some("clojure"),
        "hs" => Some("haskell"),
        "ml" | "mli" => Some("ocaml"),
        "r" => Some("r"),
        "dart" => Some("dart"),
        _ => None,
    }
}

#[derive(Debug)]
pub struct DetectedLanguage {
    pub name: &'static str,
    pub file_count: usize,
}

/// Scan workspace and detect the top languages by file count.
pub fn detect_languages(root: &Path, max_languages: usize) -> Vec<DetectedLanguage> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();

    let walker = WalkBuilder::new(root)
        .max_depth(Some(5))
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && let Some(lang) = ext_to_lang(&ext.to_lowercase())
        {
            *counts.entry(lang).or_insert(0) += 1;
        }
    }

    // Also check manifest files for additional context
    boost_from_manifests(root, &mut counts);

    let mut langs: Vec<DetectedLanguage> = counts
        .into_iter()
        .map(|(name, file_count)| DetectedLanguage { name, file_count })
        .collect();

    langs.sort_by_key(|l| std::cmp::Reverse(l.file_count));
    langs.truncate(max_languages);
    langs
}

fn boost_from_manifests(root: &Path, counts: &mut HashMap<&'static str, usize>) {
    let manifests = [
        ("Cargo.toml", "rust"),
        ("package.json", "javascript"),
        ("go.mod", "go"),
        ("pyproject.toml", "python"),
        ("setup.py", "python"),
        ("requirements.txt", "python"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("Gemfile", "ruby"),
        ("composer.json", "php"),
        ("pubspec.yaml", "dart"),
        ("mix.exs", "elixir"),
    ];

    for (manifest, lang) in &manifests {
        if root.join(manifest).exists() {
            *counts.entry(lang).or_insert(0) += 10; // boost
        }
    }
}

/// Clear the parser cache directory.
pub fn clear_parser_cache() -> anyhow::Result<()> {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".cache"))
        .join("t0k3n-mcp")
        .join("parsers");

    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)?;
        tracing::info!("Parser cache cleared: {:?}", cache_dir);
    } else {
        tracing::info!("Parser cache directory does not exist, nothing to clear.");
    }
    Ok(())
}
