//! `t0k3n hook` — a Claude Code PreToolUse hook that steers built-in
//! Read/Grep/Glob calls toward the equivalent t0k3n tools.
//!
//! Written as a subcommand of the binary the user already installed so the
//! generated settings.json has no runtime dependency (no node, no jq).
//!
//! Protocol: the hook JSON arrives on stdin; exit 0 with a
//! `hookSpecificOutput.permissionDecision` of "deny" to block the call, or exit
//! 0 printing nothing to let the normal permission flow proceed.

use std::io::Read;

use anyhow::Result;

/// Files at or below this many lines are cheap enough to read whole, so the
/// hook stays out of the way. Overridable per project.
const DEFAULT_MAX_LINES: usize = 200;

pub fn run(argv: &[String]) -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    // A hook that cannot parse its own input must never block the session.
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&input) else {
        return Ok(());
    };
    if let Some(reason) = decide(&payload, &max_lines(argv)) {
        println!(
            "{}",
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            })
        );
    }
    Ok(())
}

/// `--max-lines N` wins over `T0K3N_HOOK_MAX_LINES`, so a generated
/// settings.json is self-contained but a project can still override per shell.
fn max_lines(argv: &[String]) -> usize {
    argv.windows(2)
        .find(|w| w[0] == "--max-lines")
        .and_then(|w| w[1].parse().ok())
        .or_else(|| {
            std::env::var("T0K3N_HOOK_MAX_LINES")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(DEFAULT_MAX_LINES)
}

/// `Some(reason)` denies the call; `None` lets it through.
fn decide(payload: &serde_json::Value, max_lines: &usize) -> Option<String> {
    let tool = payload.get("tool_name")?.as_str()?;
    match tool {
        "Grep" => Some(
            "Use t0k3n instead of the built-in Grep: search_file (keyword/regex with context \
             in one file), semantic_search (meaning-based search across the workspace), or \
             read_symbol_usages (every reference to a symbol). They return only the matching \
             regions, not whole files."
                .to_string(),
        ),
        "Glob" => Some(
            "Use t0k3n instead of the built-in Glob: read_directory_tree (filtered, \
             .gitignore-aware tree) or project_digest (cached architecture warm-start). \
             semantic_search finds files by meaning when you do not know the path."
                .to_string(),
        ),
        "Read" => decide_read(payload, max_lines),
        _ => None,
    }
}

fn decide_read(payload: &serde_json::Value, max_lines: &usize) -> Option<String> {
    let path = payload.get("tool_input")?.get("file_path")?.as_str()?;
    // An explicit range is already a partial read — leave it alone.
    if payload["tool_input"].get("limit").is_some() {
        return None;
    }
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // Images, PDFs and other binaries are the built-in Read's job, not ours.
    let suggestion = suggestion_for(&ext)?;

    // Small files are cheaper to read whole than to outline-then-extract.
    let lines = std::fs::read_to_string(path).ok()?.lines().count();
    if lines <= *max_lines {
        return None;
    }
    Some(format!(
        "{path} is {lines} lines — reading it whole spends tokens on content you will not use. \
         {suggestion} (The threshold is {max_lines} lines; built-in Read with an explicit \
         offset/limit is still allowed.)"
    ))
}

/// The t0k3n route for a given extension, or `None` when the built-in Read is
/// genuinely the right tool.
fn suggestion_for(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go" | "c" | "h" | "cc" | "cpp" | "hpp"
        | "java" | "rb" | "cs" | "php" | "kt" | "kts" | "swift" | "lua" => {
            "Call read_code without ids for the skeleton, then read_code with just the ids you \
             need (zoom: skeleton/sketch/body)."
        }
        "md" | "markdown" => {
            "Call read_markdown_toc for the outline, then read_markdown_section for the \
             sections you need."
        }
        "json" | "yaml" | "yml" | "toml" => {
            "Call read_json_yaml_keys for the key structure, then read_json_yaml_value for the \
             subtree you need."
        }
        "ipynb" => "Call read_notebook, which strips output blobs and base64 images.",
        "css" | "scss" | "sass" | "less" => "Call read_css for selectors and rules on demand.",
        "log" | "txt" => {
            "Call read_log_tail (or read_file_outline) instead of pulling the whole file."
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "pdf" | "ico" | "zip" | "exe" => {
            return None;
        }
        _ => "Call read_file_outline first, then extract only the parts you need.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_and_glob_are_always_denied() {
        let grep = serde_json::json!({"tool_name": "Grep", "tool_input": {"pattern": "x"}});
        assert!(decide(&grep, &200).unwrap().contains("search_file"));
        let glob = serde_json::json!({"tool_name": "Glob", "tool_input": {"pattern": "**/*"}});
        assert!(decide(&glob, &200).unwrap().contains("read_directory_tree"));
    }

    #[test]
    fn unrelated_tools_pass_through() {
        let bash = serde_json::json!({"tool_name": "Bash", "tool_input": {"command": "ls"}});
        assert_eq!(decide(&bash, &200), None);
        assert_eq!(decide(&serde_json::json!({}), &200), None);
    }

    #[test]
    fn read_is_denied_only_above_the_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small.rs");
        std::fs::write(&small, "fn a() {}\n".repeat(10)).unwrap();
        let big = dir.path().join("big.rs");
        std::fs::write(&big, "fn a() {}\n".repeat(500)).unwrap();

        let payload = |p: &std::path::Path| serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": p.to_string_lossy()}});
        assert_eq!(decide(&payload(&small), &200), None);
        let reason = decide(&payload(&big), &200).unwrap();
        assert!(reason.contains("read_code"), "{reason}");
        assert!(reason.contains("500 lines"), "{reason}");
    }

    #[test]
    fn read_with_an_explicit_range_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.rs");
        std::fs::write(&big, "fn a() {}\n".repeat(500)).unwrap();
        let payload = serde_json::json!({
            "tool_name": "Read",
            "tool_input": {"file_path": big.to_string_lossy(), "offset": 1, "limit": 50},
        });
        assert_eq!(decide(&payload, &200), None);
    }

    #[test]
    fn binaries_and_missing_files_pass_through() {
        let img = serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": "a/b.png"}});
        assert_eq!(decide(&img, &200), None);
        let gone =
            serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": "nope-xyz.rs"}});
        assert_eq!(decide(&gone, &200), None);
    }

    #[test]
    fn suggestions_are_type_specific() {
        assert!(suggestion_for("md").unwrap().contains("read_markdown_toc"));
        assert!(
            suggestion_for("yaml")
                .unwrap()
                .contains("read_json_yaml_keys")
        );
        assert!(suggestion_for("ipynb").unwrap().contains("read_notebook"));
        assert!(suggestion_for("unknownext").unwrap().contains("outline"));
        assert_eq!(suggestion_for("png"), None);
    }
}
