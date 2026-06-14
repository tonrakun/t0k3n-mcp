//! CLI subcommands: `upgrade` (self-update in place) and `setup` (.mcp.json generation).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::update;

const GITHUB_REPO: &str = "tonrakun/t0k3n-mcp";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Legacy binary name kept in sync so pre-rename `.mcp.json` configs keep working.
#[cfg(windows)]
const LEGACY_BIN_NAME: &str = "t0k3n-mcp.exe";
#[cfg(not(windows))]
const LEGACY_BIN_NAME: &str = "t0k3n-mcp";

pub fn print_help() {
    println!(
        "t0k3n {CURRENT_VERSION} — Token-saving MCP server for AI coding tools

USAGE:
    t0k3n [OPTIONS]       Start the MCP server (stdio transport)
    t0k3n <COMMAND>

COMMANDS:
    upgrade               Download the latest release and replace this binary in place
    setup [dir]           Write or merge .mcp.json pointing at this binary with --root set to dir (default: current dir)
    version               Print version and exit
    help                  Show this help

OPTIONS:
    --root <path>             Workspace root directory (default: .)
    --format <compact|json>   Tool output format (default: compact)
    --no-dashboard            Disable the web dashboard
    --open-browser            Open the dashboard in a browser on startup
    --dashboard-port <port>   Dashboard port (default: 14123)
    --list-tools              Print all registered tool names and exit
    --refresh-parsers         Clear the tree-sitter parser cache on startup
    --enable-diagnostics      Register the opt-in read_type_diagnostics tool
                              (heavyweight: spawns cargo check/tsc/pyright/go vet;
                              also enablable via T0K3N_ENABLE_DIAGNOSTICS=1)
    --enable-writes           Register opt-in write tools (create_file, delete_symbol,
                              insert_symbol, apply_edits). Read-only by default;
                              also enablable via T0K3N_ENABLE_WRITES=1
    --version, -V             Print version and exit"
    );
}

// ── upgrade ──────────────────────────────────────────────────────────────────

pub async fn upgrade() -> Result<()> {
    println!("t0k3n upgrade — current: v{CURRENT_VERSION}");

    let latest = update::fetch_latest_version()
        .await
        .context("could not query the latest release from GitHub")?;

    match update::compare_semver(CURRENT_VERSION, &latest) {
        Ordering::Equal => {
            println!("Already up to date (v{CURRENT_VERSION}). Nothing to do.");
            return Ok(());
        }
        Ordering::Greater => {
            println!(
                "Running v{CURRENT_VERSION}, ahead of the latest release (v{latest}). Nothing to do."
            );
            return Ok(());
        }
        Ordering::Less => println!("Latest release: v{latest}"),
    }

    let artifact = artifact_name()?;
    let url =
        format!("https://github.com/{GITHUB_REPO}/releases/download/v{latest}/{artifact}");
    println!("Downloading {url}");

    let client = reqwest::Client::builder()
        .user_agent(format!("t0k3n/{CURRENT_VERSION}"))
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let bytes = client
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .context("download failed")?
        .bytes()
        .await?;
    if bytes.len() < 1024 * 1024 {
        bail!(
            "downloaded file is too small ({} bytes) — not a valid binary",
            bytes.len()
        );
    }
    println!("Downloaded {:.1} MB", bytes.len() as f64 / (1024.0 * 1024.0));

    let exe = std::env::current_exe().context("could not locate the running executable")?;
    replace_binary(&exe, &bytes)?;

    if refresh_legacy_alias(&exe, &bytes)? {
        println!("Refreshed legacy {LEGACY_BIN_NAME} alongside the new binary");
    }

    println!("Upgrade complete: v{CURRENT_VERSION} -> v{latest}");
    println!("Restart Claude Code (or your MCP client) to load the new binary.");
    Ok(())
}

/// Release artifact name for the running platform (matches .github/workflows/release.yml).
fn artifact_name() -> Result<String> {
    let os = match std::env::consts::OS {
        os @ ("windows" | "linux" | "macos") => os,
        other => bail!("unsupported OS: {other}"),
    };
    let arch = match std::env::consts::ARCH {
        arch @ ("x86_64" | "aarch64") => arch,
        other => bail!("unsupported architecture: {other}"),
    };
    let ext = if cfg!(windows) { ".exe" } else { "" };
    Ok(format!("t0k3n-{os}-{arch}{ext}"))
}

fn replace_binary(target: &Path, bytes: &[u8]) -> Result<()> {
    let dir = target
        .parent()
        .context("executable has no parent directory")?;
    clean_old_binaries(dir);

    let tmp = target.with_extension("new");
    std::fs::write(&tmp, bytes).with_context(|| format!("could not write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }

    #[cfg(windows)]
    {
        // Windows locks a running exe against deletion but allows renaming it,
        // so move the current binary aside instead of overwriting it.
        let old = unique_old_path(target);
        std::fs::rename(target, &old).context("could not move the existing binary aside")?;
        if let Err(e) = std::fs::rename(&tmp, target) {
            let _ = std::fs::rename(&old, target);
            return Err(anyhow::Error::from(e).context("could not move the new binary into place"));
        }
        // Usually still locked while running; cleaned up on the next upgrade.
        let _ = std::fs::remove_file(&old);
    }
    #[cfg(not(windows))]
    {
        // rename(2) atomically replaces the binary even while a server is running.
        std::fs::rename(&tmp, target).context("could not move the new binary into place")?;
    }
    Ok(())
}

#[cfg(windows)]
fn unique_old_path(target: &Path) -> PathBuf {
    let base = target.with_extension("old");
    if !base.exists() {
        return base;
    }
    target.with_extension(format!("old-{}", std::process::id()))
}

/// Best-effort removal of `t0k3n*.old*` leftovers from previous upgrades.
fn clean_old_binaries(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("t0k3n") && name.contains(".old") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// If a legacy-named binary sits next to the current one, replace it too so
/// existing `.mcp.json` configs that still point at it keep working.
fn refresh_legacy_alias(exe: &Path, bytes: &[u8]) -> Result<bool> {
    if exe.file_name().is_some_and(|n| n == LEGACY_BIN_NAME) {
        return Ok(false);
    }
    let Some(dir) = exe.parent() else {
        return Ok(false);
    };
    let legacy = dir.join(LEGACY_BIN_NAME);
    if !legacy.exists() {
        return Ok(false);
    }
    replace_binary(&legacy, bytes)?;
    Ok(true)
}

// ── setup ────────────────────────────────────────────────────────────────────

/// Write (or merge into) `.mcp.json` in `dir`, registering this binary as the
/// `t0k3n` MCP server.
pub fn setup(dir: Option<&str>) -> Result<()> {
    let target_dir = match dir {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir()?,
    };
    if !target_dir.is_dir() {
        bail!("not a directory: {}", target_dir.display());
    }
    // --root is mandatory for the server; a relative path would silently depend
    // on the client's working directory, so always pin the absolute path.
    let target_dir = std::path::absolute(&target_dir)
        .with_context(|| format!("could not resolve absolute path: {}", target_dir.display()))?;
    let config_path = target_dir.join(".mcp.json");
    let exe = std::env::current_exe().context("could not locate the running executable")?;

    let mut root = if config_path.exists() {
        let text = std::fs::read_to_string(&config_path)?;
        serde_json::from_str::<serde_json::Value>(&text)
            .with_context(|| format!("{} is not valid JSON", config_path.display()))?
    } else {
        serde_json::json!({})
    };

    let obj = root
        .as_object_mut()
        .context(".mcp.json top level must be a JSON object")?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .context("\"mcpServers\" must be a JSON object")?;
    servers.insert(
        "t0k3n".to_string(),
        serde_json::json!({
            "command": exe.to_string_lossy(),
            "args": ["--root", target_dir.to_string_lossy()],
        }),
    );

    std::fs::write(
        &config_path,
        format!("{}\n", serde_json::to_string_pretty(&root)?),
    )?;
    println!("MCP config written: {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_creates_and_merges_config() {
        let dir = tempfile::tempdir().unwrap();
        let dir_str = dir.path().to_str().unwrap();

        setup(Some(dir_str)).unwrap();
        let config_path = dir.path().join(".mcp.json");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(json["mcpServers"]["t0k3n"]["command"].is_string());
        let expected_root = std::path::absolute(dir.path()).unwrap();
        assert_eq!(
            json["mcpServers"]["t0k3n"]["args"],
            serde_json::json!(["--root", expected_root.to_string_lossy()])
        );

        // Merging keeps other servers intact
        std::fs::write(
            &config_path,
            r#"{ "mcpServers": { "other": { "command": "x" } } }"#,
        )
        .unwrap();
        setup(Some(dir_str)).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(json["mcpServers"]["other"]["command"], "x");
        assert!(json["mcpServers"]["t0k3n"]["command"].is_string());
    }

    #[test]
    fn test_artifact_name_matches_release_naming() {
        let name = artifact_name().unwrap();
        assert!(name.starts_with("t0k3n-"));
        #[cfg(windows)]
        assert!(name.ends_with(".exe"));
    }
}
