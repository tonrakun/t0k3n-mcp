//! CLI subcommands: `upgrade` (self-update in place) and `setup` (.mcp.json generation).

use std::cmp::Ordering;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::update;
use crate::update::GITHUB_REPO;

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
    setup [dir] [--yes]   Interactive wizard that writes (or merges into) an MCP
                          config for this binary: config scope, server name, tool
                          roster, output format, dashboard and capability flags.
                          --yes (-y) skips every prompt and writes the defaults:
                          .mcp.json in dir with --root set to dir (default: current dir).
                          Non-TTY invocations are non-interactive unless
                          --interactive (-i) is passed to force the wizard.
    hook [--max-lines N]  PreToolUse hook for Claude Code: reads the hook JSON on
                          stdin and steers built-in Read/Grep/Glob calls to the
                          t0k3n equivalents. Whole-file Reads over N lines
                          (default 200) are denied with a pointer to the right
                          tool; smaller ones pass through. `setup` wires this up
                          for you — you rarely run it by hand.
    version               Print version and exit
    help                  Show this help

OPTIONS:
    --root <path>             Workspace root directory (default: .)
    --format <compact|json>   Tool output format (default: compact)
    --no-dashboard            Disable the web dashboard
    --open-browser            Open the dashboard in a browser on startup
    --dashboard-port <port>   Dashboard port (default: 14123)
    --list-tools              Print the tool catalog and exit. Honors the other flags:
                              anything this invocation will not serve is listed with
                              the reason (roster, capability gate, or build)
    --refresh-parsers         Clear the tree-sitter parser cache on startup
    --tools <categories>      Register only these help() categories or profiles,
                              comma-separated. Profile: core (file,git,text,debug)
                              (e.g. file,git,analysis). Every tool schema is carried by
                              the client on every request, so a narrower roster saves
                              tokens. help and debug_info are always kept, and an
                              explicit --enable-writes / --enable-diagnostics adds its
                              tools back even when the roster omits their category.
                              Also settable via T0K3N_TOOLS
    --no-update-check         Do not contact GitHub for a newer release on startup
                              (also via T0K3N_NO_UPDATE_CHECK=1)

CAPABILITIES:
    Reads are always on. The remaining capabilities are:

    --enable-diagnostics      Register read_type_diagnostics (opt-in, default off).
                              Heavyweight: spawns cargo check/tsc/pyright/go vet.
                              Also via T0K3N_ENABLE_DIAGNOSTICS=1
    --enable-writes           Register the structured write tools (create_file,
                              delete_symbol, insert_symbol, apply_edits, ...).
                              Opt-in, default off. Also via T0K3N_ENABLE_WRITES=1.
                              Wins over --tools: the write category is registered
                              even under a roster that leaves it out
    --disable-commands        Unregister run_command. Opt-out: shell execution is ON
                              by default. Also via T0K3N_DISABLE_COMMANDS=1

    NOTE: with run_command registered the server is NOT read-only — anything
    reachable from a shell is reachable. --enable-writes gates the structured write
    tools; use --disable-commands as well for a genuinely read-only server.

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
    let base = format!("https://github.com/{GITHUB_REPO}/releases/download/v{latest}");
    let url = format!("{base}/{artifact}");

    let client = reqwest::Client::builder()
        .user_agent(format!("t0k3n/{CURRENT_VERSION}"))
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    // Fetch the checksum manifest *before* the binary: a release we cannot verify
    // must not be written over the running executable.
    let sums_url = format!("{base}/SHA256SUMS.txt");
    let manifest = client
        .get(&sums_url)
        .send()
        .await?
        .error_for_status()
        .with_context(|| {
            format!(
                "could not download the checksum manifest ({sums_url}). \
                 Releases before v3.4.0 do not publish one; \
                 install manually from the releases page instead."
            )
        })?
        .text()
        .await?;
    let expected = expected_sha256(&manifest, &artifact).with_context(|| {
        format!(
            "SHA256SUMS.txt does not list {artifact} — refusing to install an unverified binary"
        )
    })?;

    println!("Downloading {url}");
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

    let actual = sha256_hex(&bytes);
    if actual != expected {
        bail!(
            "checksum mismatch for {artifact}\n  expected: {expected}\n  actual:   {actual}\n\
             The download was corrupted or tampered with; nothing was installed."
        );
    }
    println!(
        "Downloaded {:.1} MB — sha256 verified",
        bytes.len() as f64 / (1024.0 * 1024.0)
    );

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

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Look up `artifact`'s digest in a `sha256sum`-style manifest
/// (`<hex>  <filename>` per line, optionally with a `*` binary marker).
fn expected_sha256(manifest: &str, artifact: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let (digest, name) = line.split_once("  ").or_else(|| line.split_once(' '))?;
        let name = name.trim().trim_start_matches('*');
        let digest = digest.trim().to_ascii_lowercase();
        // Reject anything that is not a plausible sha256 so a stray prose line
        // in the manifest can never be mistaken for a valid digest.
        let valid = digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit());
        (name == artifact && valid).then_some(digest)
    })
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

/// Everything the wizard can decide. Defaults reproduce the pre-wizard,
/// non-interactive behaviour exactly, so `setup --yes` is unchanged.
struct SetupOptions {
    server_name: String,
    root: PathBuf,
    /// Where `.mcp.json` (or the user-scope config) is written.
    config_path: PathBuf,
    tools: Option<String>,
    json_format: bool,
    no_dashboard: bool,
    dashboard_port: Option<u16>,
    open_browser: bool,
    enable_writes: bool,
    enable_diagnostics: bool,
    disable_commands: bool,
    no_update_check: bool,
    /// How hard to push the agent off the client's built-in file tools.
    steering: Steering,
    /// `settings.json` to write the steering config into (scope-matched to
    /// `config_path`).
    settings_path: PathBuf,
    /// Line count above which the hook denies a whole-file built-in Read.
    hook_max_lines: usize,
}

/// The server's `instructions` already ask the agent to prefer t0k3n; these are
/// the levers that actually enforce it, written into `settings.json`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Steering {
    /// Nothing written — instructions and CLAUDE.md only.
    None,
    /// Grep/Glob denied outright, Read judged per file by the hook. The
    /// recommended setting: Read stays available for small files, which the
    /// client's Edit flow depends on.
    Recommended,
    /// No deny rules; the hook decides every Read/Grep/Glob call.
    HookOnly,
}

impl SetupOptions {
    fn defaults(root: PathBuf) -> Self {
        Self {
            server_name: "t0k3n".to_string(),
            config_path: root.join(".mcp.json"),
            settings_path: root.join(".claude").join("settings.json"),
            root,
            steering: Steering::None,
            hook_max_lines: 200,
            tools: None,
            json_format: false,
            no_dashboard: false,
            dashboard_port: None,
            open_browser: false,
            enable_writes: false,
            enable_diagnostics: false,
            disable_commands: false,
            no_update_check: false,
        }
    }

    /// Server `args` for `.mcp.json`. `--root` is always absolute: a relative
    /// path would silently depend on the client's working directory.
    fn args(&self) -> Vec<String> {
        let mut args = vec![
            "--root".to_string(),
            self.root.to_string_lossy().into_owned(),
        ];
        if let Some(tools) = &self.tools {
            args.push("--tools".to_string());
            args.push(tools.clone());
        }
        if self.json_format {
            args.push("--format".to_string());
            args.push("json".to_string());
        }
        if self.no_dashboard {
            args.push("--no-dashboard".to_string());
        } else {
            if let Some(port) = self.dashboard_port {
                args.push("--dashboard-port".to_string());
                args.push(port.to_string());
            }
            if self.open_browser {
                args.push("--open-browser".to_string());
            }
        }
        if self.enable_writes {
            args.push("--enable-writes".to_string());
        }
        if self.enable_diagnostics {
            args.push("--enable-diagnostics".to_string());
        }
        if self.disable_commands {
            args.push("--disable-commands".to_string());
        }
        if self.no_update_check {
            args.push("--no-update-check".to_string());
        }
        args
    }
}

/// Write (or merge into) an MCP config, registering this binary as an MCP
/// server. Interactive by default on a TTY; `--yes` keeps the old one-shot
/// behaviour (`.mcp.json` in `dir`, `--root <dir>`, no other flags).
pub fn setup(argv: &[String]) -> Result<()> {
    let non_interactive = argv
        .iter()
        .any(|a| matches!(a.as_str(), "-y" | "--yes" | "--non-interactive"));
    // --interactive forces the wizard when stdin is not a TTY (piped answers,
    // terminal wrappers). --yes still wins.
    let force_interactive = argv.iter().any(|a| a == "--interactive" || a == "-i");
    let dir = argv.iter().find(|a| !a.starts_with('-'));

    let target_dir = match dir {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir()?,
    };
    if !target_dir.is_dir() {
        bail!("not a directory: {}", target_dir.display());
    }
    let target_dir = std::path::absolute(&target_dir)
        .with_context(|| format!("could not resolve absolute path: {}", target_dir.display()))?;

    let mut opts = SetupOptions::defaults(target_dir);
    let interactive = !non_interactive && (force_interactive || std::io::stdin().is_terminal());
    if interactive && !run_wizard(&mut opts)? {
        println!("中止しました。設定は変更されていません。");
        return Ok(());
    }
    write_config(&opts)
}

fn write_config(opts: &SetupOptions) -> Result<()> {
    let exe = std::env::current_exe().context("could not locate the running executable")?;
    let config_path = &opts.config_path;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let mut root = if config_path.exists() {
        let text = std::fs::read_to_string(config_path)?;
        if text.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str::<serde_json::Value>(&text)
                .with_context(|| format!("{} is not valid JSON", config_path.display()))?
        }
    } else {
        serde_json::json!({})
    };

    let obj = root
        .as_object_mut()
        .context("config top level must be a JSON object")?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .context("\"mcpServers\" must be a JSON object")?;
    servers.insert(
        opts.server_name.clone(),
        serde_json::json!({
            "command": exe.to_string_lossy(),
            "args": opts.args(),
        }),
    );

    std::fs::write(
        config_path,
        format!("{}\n", serde_json::to_string_pretty(&root)?),
    )?;
    println!("MCP config written: {}", config_path.display());

    if opts.steering != Steering::None {
        write_steering_settings(opts, &exe)?;
        println!("Tool steering written: {}", opts.settings_path.display());
    }
    Ok(())
}

// ── built-in tool steering ───────────────────────────────────────────────────

/// Tool names denied outright under `Steering::Recommended`. `Read` is
/// deliberately absent: the client's Edit flow requires a prior Read, so
/// denying it wholesale breaks editing. The hook handles Read case by case.
const DENIED_BUILTINS: &[&str] = &["Grep", "Glob"];

/// Shell-quote the executable path for the hook `command` string, which is run
/// through a shell and so would otherwise split on spaces.
fn hook_command(exe: &Path, max_lines: usize) -> String {
    let path = exe.to_string_lossy();
    let path = if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path.into_owned()
    };
    format!("{path} hook --max-lines {max_lines}")
}

/// Merge the deny rules and the PreToolUse hook into `settings.json`, leaving
/// every other key — and every unrelated hook — untouched.
fn write_steering_settings(opts: &SetupOptions, exe: &Path) -> Result<()> {
    let path = &opts.settings_path;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let mut settings = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        if text.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str::<serde_json::Value>(&text)
                .with_context(|| format!("{} is not valid JSON", path.display()))?
        }
    } else {
        serde_json::json!({})
    };
    let obj = settings
        .as_object_mut()
        .context("settings.json top level must be a JSON object")?;

    if opts.steering == Steering::Recommended {
        let deny = obj
            .entry("permissions")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .context("\"permissions\" must be a JSON object")?
            .entry("deny")
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .context("\"permissions.deny\" must be a JSON array")?;
        for tool in DENIED_BUILTINS {
            let rule = serde_json::Value::from(*tool);
            if !deny.contains(&rule) {
                deny.push(rule);
            }
        }
    }

    let matcher = match opts.steering {
        // Grep/Glob are already denied above; the hook only judges Read.
        Steering::Recommended => "Read",
        _ => "Read|Grep|Glob",
    };
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("\"hooks\" must be a JSON object")?
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("\"hooks.PreToolUse\" must be a JSON array")?;
    let entry = serde_json::json!({
        "matcher": matcher,
        "hooks": [{
            "type": "command",
            "command": hook_command(exe, opts.hook_max_lines),
        }],
    });
    // Re-running setup replaces our own entry instead of stacking duplicates.
    match hooks.iter().position(is_t0k3n_hook) {
        Some(i) => hooks[i] = entry,
        None => hooks.push(entry),
    }

    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&settings)?),
    )?;
    Ok(())
}

/// Recognise a PreToolUse entry this command previously wrote.
fn is_t0k3n_hook(entry: &serde_json::Value) -> bool {
    entry["hooks"].as_array().is_some_and(|handlers| {
        handlers.iter().any(|h| {
            h["command"]
                .as_str()
                .is_some_and(|c| c.contains("t0k3n") && c.contains(" hook"))
        })
    })
}

// ── setup wizard ─────────────────────────────────────────────────────────────

/// Read one line from stdin. `Ok(None)` on EOF, which the callers treat as
/// "keep the default" so a piped-but-TTY-ish session cannot hang.
fn read_line() -> Result<Option<String>> {
    let mut buf = String::new();
    if std::io::stdin().read_line(&mut buf)? == 0 {
        return Ok(None);
    }
    Ok(Some(buf.trim().to_string()))
}

fn ask(prompt: &str, default: &str) -> Result<String> {
    print!("{prompt} [{default}]: ");
    std::io::stdout().flush()?;
    match read_line()? {
        Some(s) if !s.is_empty() => Ok(s),
        _ => Ok(default.to_string()),
    }
}

fn ask_yes_no(prompt: &str, default: bool) -> Result<bool> {
    let hint = if default { "Y/n" } else { "y/N" };
    loop {
        print!("{prompt} [{hint}]: ");
        std::io::stdout().flush()?;
        let Some(answer) = read_line()? else {
            return Ok(default);
        };
        match answer.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("  y か n で答えてください。"),
        }
    }
}

/// Numbered single choice. Returns the index into `choices`.
fn ask_choice(prompt: &str, choices: &[(&str, &str)], default: usize) -> Result<usize> {
    println!("{prompt}");
    for (i, (label, note)) in choices.iter().enumerate() {
        let mark = if i == default { "*" } else { " " };
        if note.is_empty() {
            println!("  {mark}{}) {label}", i + 1);
        } else {
            println!("  {mark}{}) {label} — {note}", i + 1);
        }
    }
    loop {
        print!("  番号を選択 [{}]: ", default + 1);
        std::io::stdout().flush()?;
        let Some(answer) = read_line()? else {
            return Ok(default);
        };
        if answer.is_empty() {
            return Ok(default);
        }
        match answer.parse::<usize>() {
            Ok(n) if n >= 1 && n <= choices.len() => return Ok(n - 1),
            _ => println!("  1〜{} の番号を入力してください。", choices.len()),
        }
    }
}

/// Drive the interactive setup. `Ok(false)` means the user declined to write.
fn run_wizard(opts: &mut SetupOptions) -> Result<bool> {
    println!("t0k3n setup ウィザード (v{CURRENT_VERSION})");
    println!("Enter で [ ] 内のデフォルトを採用します。\n");

    // 1. workspace root
    let root = ask(
        "ワークスペースのルートディレクトリ",
        &opts.root.to_string_lossy(),
    )?;
    let root = std::path::absolute(PathBuf::from(&root))
        .with_context(|| format!("could not resolve absolute path: {root}"))?;
    if !root.is_dir() {
        bail!("not a directory: {}", root.display());
    }
    opts.root = root;

    // 2. config scope
    let user_config = dirs::home_dir().map(|h| h.join(".claude.json"));
    let mut scopes: Vec<(String, String, PathBuf)> = vec![(
        "プロジェクト".to_string(),
        opts.root.join(".mcp.json").display().to_string(),
        opts.root.join(".mcp.json"),
    )];
    if let Some(path) = &user_config {
        scopes.push((
            "ユーザー全体".to_string(),
            path.display().to_string(),
            path.clone(),
        ));
    }
    let labels: Vec<(&str, &str)> = scopes
        .iter()
        .map(|(l, n, _)| (l.as_str(), n.as_str()))
        .collect();
    let scope = ask_choice("\n設定の書き込み先:", &labels, 0)?;
    opts.config_path = scopes[scope].2.clone();

    // 3. server name — lets several roots coexist in one user-scope config
    opts.server_name = ask("\nMCP サーバー名", &opts.server_name)?;

    // 4. tool roster — the single biggest lever on schema token cost
    let categories = crate::server::known_tool_categories();
    let roster = ask_choice(
        "\n登録するツールのロスター (スキーマはリクエスト毎に送られるため、絞るほど節約になります):",
        &[
            ("すべて", "全カテゴリ、最大のトークンコスト"),
            (
                "core プロファイル",
                "file,git,text,debug — デフォルト比 約58%削減",
            ),
            ("カテゴリを自分で選ぶ", ""),
        ],
        0,
    )?;
    opts.tools = match roster {
        1 => Some("core".to_string()),
        2 => {
            println!("  利用可能なカテゴリ: {}", categories.join(", "));
            loop {
                let picked = ask("  カテゴリをカンマ区切りで入力", "file,git,text,debug")?;
                let picked: Vec<String> = picked
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                let unknown: Vec<&String> = picked
                    .iter()
                    .filter(|c| !categories.contains(&c.as_str()) && *c != "core")
                    .collect();
                if picked.is_empty() {
                    println!("  1つ以上指定してください。");
                } else if unknown.is_empty() {
                    break Some(picked.join(","));
                } else {
                    println!(
                        "  未知のカテゴリ: {}",
                        unknown
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        _ => None,
    };

    // 5. output format
    opts.json_format = ask_choice(
        "\nツール出力フォーマット:",
        &[
            ("compact", "トークン効率の良いテキスト（推奨）"),
            ("json", "従来の JSON"),
        ],
        0,
    )? == 1;

    // 6. dashboard
    opts.no_dashboard = !ask_yes_no("\nWeb ダッシュボードを有効にしますか", true)?;
    if !opts.no_dashboard {
        let port = ask("  ダッシュボードのポート", "14123")?;
        let port: u16 = port
            .parse()
            .context("ポートは 1〜65535 の数値で指定してください")?;
        opts.dashboard_port = (port != 14123).then_some(port);
        opts.open_browser = ask_yes_no("  起動時にブラウザを開きますか", false)?;
    }

    // 7. capabilities
    println!("\n機能の有効化 (読み取り系は常に有効):");
    opts.enable_writes = ask_yes_no(
        "  構造化書き込みツール (create_file / insert_symbol など) を有効にしますか",
        false,
    )?;
    opts.enable_diagnostics = ask_yes_no(
        "  read_type_diagnostics を有効にしますか (cargo check/tsc などを起動する重い機能)",
        false,
    )?;
    opts.disable_commands = !ask_yes_no("  run_command (シェル実行) を有効のままにしますか", true)?;
    opts.no_update_check = !ask_yes_no("  起動時に GitHub の更新確認を行いますか", true)?;

    // 8. steering — the server's instructions are only a request; these are the
    //    settings.json levers that actually enforce the preference.
    let steering = ask_choice(
        "\nクライアントのビルトインツール (Read/Grep/Glob) の抑制:",
        &[
            (
                "推奨",
                "Grep/Glob は deny、Read は大きいファイルのみフックで拒否",
            ),
            ("フックのみ", "deny せず、Read/Grep/Glob をすべてフック判定"),
            ("しない", "settings.json を変更しない"),
        ],
        0,
    )?;
    opts.steering = match steering {
        0 => Steering::Recommended,
        1 => Steering::HookOnly,
        _ => Steering::None,
    };
    if opts.steering != Steering::None {
        // Keep the steering scope aligned with where the server was registered.
        opts.settings_path = if Some(&opts.config_path) == user_config.as_ref() {
            dirs::home_dir()
                .context("could not locate the home directory")?
                .join(".claude")
                .join("settings.json")
        } else {
            opts.root.join(".claude").join("settings.json")
        };
        let lines = ask(
            "  Read を拒否する行数のしきい値 (これ以下は素通し)",
            &opts.hook_max_lines.to_string(),
        )?;
        opts.hook_max_lines = lines
            .parse()
            .context("しきい値は行数の数値で指定してください")?;
    }

    // 9. preview + confirm
    println!("\n── 書き込み内容 ──────────────────────────────");
    println!("{}", opts.config_path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                opts.server_name.clone(): {
                    "command": std::env::current_exe()?.to_string_lossy(),
                    "args": opts.args(),
                }
            }
        }))?
    );
    if opts.config_path.exists() {
        println!("(既存の設定にマージされます)");
    }
    if opts.steering != Steering::None {
        println!("\n{}", opts.settings_path.display());
        if opts.steering == Steering::Recommended {
            println!("  permissions.deny  += {}", DENIED_BUILTINS.join(", "));
        }
        println!(
            "  hooks.PreToolUse  += matcher \"{}\" → {}",
            if opts.steering == Steering::Recommended {
                "Read"
            } else {
                "Read|Grep|Glob"
            },
            hook_command(&std::env::current_exe()?, opts.hook_max_lines)
        );
    }
    println!("──────────────────────────────────────────────");
    ask_yes_no("この内容で書き込みますか", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_creates_and_merges_config() {
        let dir = tempfile::tempdir().unwrap();
        let dir_str = dir.path().to_str().unwrap();

        let argv = |d: &str| vec![d.to_string(), "--yes".to_string()];

        setup(&argv(dir_str)).unwrap();
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
        setup(&argv(dir_str)).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(json["mcpServers"]["other"]["command"], "x");
        assert!(json["mcpServers"]["t0k3n"]["command"].is_string());
    }

    #[test]
    fn test_setup_options_default_args_are_root_only() {
        let opts = SetupOptions::defaults(PathBuf::from("/w"));
        assert_eq!(opts.args(), vec!["--root", "/w"]);
    }

    #[test]
    fn test_setup_options_args_reflect_wizard_choices() {
        let mut opts = SetupOptions::defaults(PathBuf::from("/w"));
        opts.tools = Some("core".to_string());
        opts.json_format = true;
        opts.dashboard_port = Some(9000);
        opts.open_browser = true;
        opts.enable_writes = true;
        opts.enable_diagnostics = true;
        opts.disable_commands = true;
        opts.no_update_check = true;
        assert_eq!(
            opts.args(),
            vec![
                "--root",
                "/w",
                "--tools",
                "core",
                "--format",
                "json",
                "--dashboard-port",
                "9000",
                "--open-browser",
                "--enable-writes",
                "--enable-diagnostics",
                "--disable-commands",
                "--no-update-check",
            ]
        );
    }

    #[test]
    fn test_setup_options_no_dashboard_drops_port_and_browser() {
        let mut opts = SetupOptions::defaults(PathBuf::from("/w"));
        opts.no_dashboard = true;
        opts.dashboard_port = Some(9000);
        opts.open_browser = true;
        assert_eq!(opts.args(), vec!["--root", "/w", "--no-dashboard"]);
    }

    #[test]
    fn test_setup_honors_server_name_and_config_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut opts = SetupOptions::defaults(dir.path().to_path_buf());
        opts.server_name = "t0k3n-web".to_string();
        opts.config_path = dir.path().join("nested").join("custom.json");
        opts.tools = Some("file,git".to_string());
        write_config(&opts).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&opts.config_path).unwrap()).unwrap();
        assert!(json["mcpServers"]["t0k3n-web"]["command"].is_string());
        assert_eq!(json["mcpServers"]["t0k3n-web"]["args"][2], "--tools");
        assert_eq!(json["mcpServers"]["t0k3n-web"]["args"][3], "file,git");
    }

    #[test]
    fn test_steering_writes_deny_rules_and_hook() {
        let dir = tempfile::tempdir().unwrap();
        let mut opts = SetupOptions::defaults(dir.path().to_path_buf());
        opts.steering = Steering::Recommended;
        opts.hook_max_lines = 120;
        write_config(&opts).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&opts.settings_path).unwrap()).unwrap();
        assert_eq!(
            json["permissions"]["deny"],
            serde_json::json!(["Grep", "Glob"])
        );
        let entry = &json["hooks"]["PreToolUse"][0];
        assert_eq!(entry["matcher"], "Read");
        let cmd = entry["hooks"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("hook --max-lines 120"), "{cmd}");
    }

    #[test]
    fn test_steering_hook_only_skips_deny_rules() {
        let dir = tempfile::tempdir().unwrap();
        let mut opts = SetupOptions::defaults(dir.path().to_path_buf());
        opts.steering = Steering::HookOnly;
        write_config(&opts).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&opts.settings_path).unwrap()).unwrap();
        assert!(json.get("permissions").is_none());
        assert_eq!(json["hooks"]["PreToolUse"][0]["matcher"], "Read|Grep|Glob");
    }

    #[test]
    fn test_steering_merges_and_does_not_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let mut opts = SetupOptions::defaults(dir.path().to_path_buf());
        opts.steering = Steering::Recommended;
        std::fs::create_dir_all(opts.settings_path.parent().unwrap()).unwrap();
        std::fs::write(
            &opts.settings_path,
            r#"{
              "model": "opus",
              "permissions": { "deny": ["Grep"], "allow": ["Bash(ls:*)"] },
              "hooks": { "PreToolUse": [
                { "matcher": "Bash", "hooks": [{ "type": "command", "command": "other.sh" }] }
              ] }
            }"#,
        )
        .unwrap();

        write_config(&opts).unwrap();
        write_config(&opts).unwrap(); // re-running setup must be idempotent

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&opts.settings_path).unwrap()).unwrap();
        assert_eq!(json["model"], "opus", "unrelated keys survive");
        assert_eq!(
            json["permissions"]["allow"],
            serde_json::json!(["Bash(ls:*)"])
        );
        assert_eq!(
            json["permissions"]["deny"],
            serde_json::json!(["Grep", "Glob"])
        );
        let hooks = json["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(hooks.len(), 2, "our entry is replaced, the Bash one kept");
        assert_eq!(hooks[0]["matcher"], "Bash");
        assert_eq!(hooks[1]["matcher"], "Read");
    }

    #[test]
    fn test_steering_none_leaves_settings_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let opts = SetupOptions::defaults(dir.path().to_path_buf());
        write_config(&opts).unwrap();
        assert!(!opts.settings_path.exists());
    }

    #[test]
    fn test_hook_command_quotes_paths_with_spaces() {
        let cmd = hook_command(Path::new("C:\\Program Files\\t0k3n.exe"), 200);
        assert_eq!(cmd, "\"C:\\Program Files\\t0k3n.exe\" hook --max-lines 200");
        assert!(is_t0k3n_hook(&serde_json::json!({
            "hooks": [{ "type": "command", "command": cmd }]
        })));
        assert!(!is_t0k3n_hook(&serde_json::json!({
            "hooks": [{ "type": "command", "command": "other.sh" }]
        })));
    }

    #[test]
    fn test_expected_sha256_parses_manifest() {
        let digest = "a".repeat(64);
        let manifest = format!(
            "{digest}  t0k3n-linux-x86_64\n{}  t0k3n-windows-x86_64.exe\n",
            "b".repeat(64)
        );
        assert_eq!(
            expected_sha256(&manifest, "t0k3n-linux-x86_64"),
            Some(digest)
        );
        assert_eq!(expected_sha256(&manifest, "t0k3n-macos-aarch64"), None);
    }

    #[test]
    fn test_expected_sha256_rejects_non_digest_lines() {
        // A prose line naming the artifact must never be accepted as a digest.
        let manifest = "see the release notes for t0k3n-linux-x86_64\n";
        assert_eq!(expected_sha256(manifest, "t0k3n-linux-x86_64"), None);
    }

    #[test]
    fn test_sha256_hex_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_artifact_name_matches_release_naming() {
        let name = artifact_name().unwrap();
        assert!(name.starts_with("t0k3n-"));
        #[cfg(windows)]
        assert!(name.ends_with(".exe"));
    }
}
