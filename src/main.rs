use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

mod cli;
mod dashboard;
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
        Some("setup") => return cli::setup(args.get(2).map(String::as_str)),
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
    let root = args
        .windows(2)
        .find(|w| w[0] == "--root")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| ".".to_string());

    let list_tools = args.iter().any(|a| a == "--list-tools");
    if list_tools {
        eprintln!(
            "t0k3n v{} — {} tools registered:",
            env!("CARGO_PKG_VERSION"),
            server::REGISTERED_TOOLS.len()
        );
        for tool in server::REGISTERED_TOOLS {
            eprintln!("  {tool}");
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

    // Output format: compact (default, token-efficient text) or json (legacy)
    let format = args
        .windows(2)
        .find(|w| w[0] == "--format")
        .and_then(|w| server::tools::render::OutputFormat::parse(&w[1]))
        .unwrap_or(server::tools::render::OutputFormat::Compact);
    server::set_output_format(format);

    let no_dashboard = args.iter().any(|a| a == "--no-dashboard");
    let open_browser = args.iter().any(|a| a == "--open-browser");
    let port = args
        .windows(2)
        .find(|w| w[0] == "--dashboard-port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or(DASHBOARD_PORT);

    tracing::info!("Starting t0k3n with root: {}", root);

    // ── Dashboard ──────────────────────────────────────────────
    let dashboard = if no_dashboard {
        None
    } else {
        let state = dashboard::DashboardState::new(env!("CARGO_PKG_VERSION"));
        let state_clone = state.clone();
        tokio::spawn(async move { dashboard::run(state_clone, port).await });

        if open_browser {
            let url = format!("http://127.0.0.1:{port}");
            if let Err(e) = open::that_detached(&url) {
                tracing::debug!("Could not open browser: {e}");
            }
        }

        Some(state)
    };

    // ── Update check ───────────────────────────────────────────
    update::spawn_update_check(dashboard.clone());

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
    let server = server::T0k3nServer::new(root, dashboard);
    let service = server.serve(transport).await.inspect_err(|e| {
        tracing::error!("Server error: {}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
