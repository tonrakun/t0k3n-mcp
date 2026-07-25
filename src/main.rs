use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

mod cli;
mod dashboard;
mod hook;
mod security;
mod server;
mod startup;
mod update;

const DASHBOARD_PORT: u16 = 14123;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // --version / -V: print and exit before any logging or server setup,
    // so install scripts can probe the binary without it blocking on stdio
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("t0k3n {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Subcommands run and exit before the MCP server starts.
    // No args (or flags only) keeps the legacy behavior: start the server.
    match args.get(1).map(String::as_str) {
        Some("upgrade") => return cli::upgrade().await,
        Some("setup") => return cli::setup(&args[2..]),
        Some("hook") => return hook::run(&args[2..]),
        Some("version") => {
            println!("t0k3n {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("help") | Some("--help") | Some("-h") => {
            cli::print_help();
            return Ok(());
        }
        Some(other) if !other.starts_with('-') => {
            eprintln!("Unknown command: {other}\n");
            cli::print_help();
            std::process::exit(2);
        }
        _ => {}
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Parse CLI flags
    let root_arg = args
        .windows(2)
        .find(|w| w[0] == "--root")
        .map(|w| w[1].clone());
    // root_configured tracks whether --root / T0K3N_ROOT was explicitly given. When false,
    // the server falls back to "." (the process cwd, often not the intended project) and
    // lets each tool call override the root via a `root` argument instead.
    let root_configured = root_arg.is_some()
        || std::env::var("T0K3N_ROOT")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
    let root = root_arg
        .or_else(|| std::env::var("T0K3N_ROOT").ok())
        .unwrap_or_else(|| ".".to_string());

    // Output format: compact (default, token-efficient text) or json (legacy)
    let format = args
        .windows(2)
        .find(|w| w[0] == "--format")
        .and_then(|w| server::tools::render::OutputFormat::parse(&w[1]))
        .unwrap_or(server::tools::render::OutputFormat::Compact);
    server::set_output_format(format);

    // read_type_diagnostics is opt-in (heavyweight: spawns cargo check / tsc / etc.).
    let diagnostics_enabled = args.iter().any(|a| a == "--enable-diagnostics")
        || std::env::var("T0K3N_ENABLE_DIAGNOSTICS")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);

    // Structured write tools are opt-in. Note this gates the *tools*, not the
    // machine: run_command still exposes a shell unless --disable-commands is given.
    let writes_enabled = args.iter().any(|a| a == "--enable-writes")
        || std::env::var("T0K3N_ENABLE_WRITES")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);

    // Shell execution is opt-out (on by default) so existing setups keep working.
    let commands_enabled = !(args.iter().any(|a| a == "--disable-commands")
        || std::env::var("T0K3N_DISABLE_COMMANDS")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false));

    // --tools <cat,cat,...> trims the registered roster. Every tool schema is
    // carried by the client on every request, so a narrower roster is itself a
    // token saving for focused sessions.
    let tool_categories = args
        .windows(2)
        .find(|w| w[0] == "--tools")
        .map(|w| {
            w[1].split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect::<Vec<String>>()
        })
        .or_else(|| {
            std::env::var("T0K3N_TOOLS").ok().map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
        })
        .filter(|v: &Vec<String>| !v.is_empty());

    if let Some(cats) = &tool_categories {
        let known = server::known_tool_categories();
        let unknown: Vec<&String> = cats
            .iter()
            .filter(|c| !known.contains(&c.as_str()))
            .collect();
        if !unknown.is_empty() {
            eprintln!(
                "Unknown --tools categor{}: {}\nAvailable: {}",
                if unknown.len() == 1 { "y" } else { "ies" },
                unknown
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                known.join(", ")
            );
            std::process::exit(2);
        }
    }

    // --list-tools answers "what does this invocation serve?", so it runs after the
    // roster and capability flags are parsed, not before: a listing that ignored
    // `--tools` reported 84 tools for a session that was about to serve 31.
    // REGISTERED_TOOLS stays the spine of the output — the catalog is what people
    // scan — with the reason next to anything this run will not register.
    if args.iter().any(|a| a == "--list-tools") {
        let config = server::ServerConfig {
            root: root.clone(),
            root_configured,
            diagnostics_enabled,
            writes_enabled,
            commands_enabled,
            tool_categories: tool_categories.clone(),
        };
        let listing: Vec<(&str, Option<String>)> = server::REGISTERED_TOOLS
            .iter()
            .map(|t| (*t, server::tool_exclusion_reason(t, &config)))
            .collect();
        let served = listing.iter().filter(|(_, reason)| reason.is_none()).count();
        eprintln!(
            "t0k3n v{} — {} tools declared, {served} served with these flags:",
            env!("CARGO_PKG_VERSION"),
            server::REGISTERED_TOOLS.len(),
        );
        for (tool, reason) in &listing {
            match reason {
                Some(note) => eprintln!("  {tool}  ({note})"),
                None => eprintln!("  {tool}"),
            }
        }
        return Ok(());
    }

    let refresh_parsers = args.iter().any(|a| a == "--refresh-parsers");
    if refresh_parsers {
        tracing::info!("--refresh-parsers: clearing parser cache");
        if let Err(e) = startup::clear_parser_cache() {
            tracing::warn!("Failed to clear parser cache: {}", e);
        }
    }

    let no_dashboard = args.iter().any(|a| a == "--no-dashboard");
    let open_browser = args.iter().any(|a| a == "--open-browser");
    let port = args
        .windows(2)
        .find(|w| w[0] == "--dashboard-port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or(DASHBOARD_PORT);

    tracing::info!(
        "Starting t0k3n with root: {} (configured: {})",
        root,
        root_configured
    );
    if !root_configured {
        tracing::warn!(
            "No --root / T0K3N_ROOT given — defaulting to the process working directory. \
             Tool calls may pass a `root` argument to override this per call."
        );
    }

    // ── Dashboard ──────────────────────────────────────────────
    let dashboard = if no_dashboard {
        None
    } else {
        let state = dashboard::DashboardState::new(env!("CARGO_PKG_VERSION"));
        let state_clone = state.clone();
        tokio::spawn(async move { dashboard::run(state_clone, port).await });

        if open_browser {
            let url = dashboard::dashboard_url(port, &state.token);
            if let Err(e) = open::that_detached(&url) {
                tracing::debug!("Could not open browser: {e}");
            }
        }

        Some(state)
    };

    // ── Update check ───────────────────────────────────────────
    // Opt-out: this is the server's only unsolicited outbound request, and
    // air-gapped or audited environments need to be able to switch it off.
    let no_update_check = args.iter().any(|a| a == "--no-update-check")
        || std::env::var("T0K3N_NO_UPDATE_CHECK")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
    if no_update_check {
        tracing::info!("Update check disabled (--no-update-check)");
    } else {
        update::spawn_update_check(dashboard.clone());
    }

    // ── Language detection ─────────────────────────────────────
    let root_path = std::path::Path::new(&root);
    let langs = startup::detect_languages(root_path, 10);
    if langs.is_empty() {
        tracing::info!("No source languages detected in workspace.");
    } else {
        let lang_list: Vec<String> = langs
            .iter()
            .map(|l| format!("{}({})", l.name, l.file_count))
            .collect();
        tracing::info!("Detected languages: {}", lang_list.join(", "));
    }

    // ── MCP server ─────────────────────────────────────────────
    let transport = stdio();
    let server = server::T0k3nServer::new(
        server::ServerConfig {
            root,
            root_configured,
            diagnostics_enabled,
            writes_enabled,
            commands_enabled,
            tool_categories,
        },
        dashboard,
    );
    let service = server.serve(transport).await.inspect_err(|e| {
        tracing::error!("Server error: {}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
