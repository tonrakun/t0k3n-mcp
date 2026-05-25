use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadGitDiffParams {
    #[schemars(description = "Base ref to diff against (default: HEAD). Examples: HEAD, main, abc1234")]
    pub base: Option<String>,
    #[schemars(description = "Limit diff to this file path (relative to workspace root). Omit for all changes.")]
    pub path: Option<String>,
    #[schemars(description = "Return only --stat summary instead of full diff (default: false)")]
    pub stat_only: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadGitDiffResult {
    pub diff: String,
    pub token_count: usize,
}

pub fn read_git_diff(root: &Path, params: ReadGitDiffParams) -> Result<ReadGitDiffResult, String> {
    let base = params.base.as_deref().unwrap_or("HEAD");
    let stat_only = params.stat_only.unwrap_or(false);

    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.arg("diff");

    if stat_only {
        cmd.arg("--stat");
    } else {
        // Reduce context lines to keep output compact
        cmd.arg("--unified=2");
    }

    cmd.arg(base);

    if let Some(ref p) = params.path {
        cmd.arg("--").arg(p);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("git コマンド実行失敗: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff 失敗: {stderr}"));
    }

    let diff = String::from_utf8_lossy(&output.stdout).into_owned();
    let token_count = estimate_tokens(&diff);

    Ok(ReadGitDiffResult { diff, token_count })
}
