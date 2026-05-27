use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

mod dashboard;
mod security;
mod server;
mod startup;
mod update;

const DASHBOARD_PORT: u16 = 14123;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Parse CLI flags
    let root = args
        .windows(2)
        .find(|w| w[0] == "--root")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| ".".to_string());

    let list_tools = args.iter().any(|a| a == "--list-tools");
    if list_tools {
        eprintln!(
            "t0k3n-mcp v{} — {} tools registered:",
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

    let no_dashboard = args.iter().any(|a| a == "--no-dashboard");
    let port = args
        .windows(2)
        .find(|w| w[0] == "--dashboard-port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or(DASHBOARD_PORT);

    tracing::info!("Starting t0k3n-mcp with root: {}", root);

    // ── Dashboard ──────────────────────────────────────────────
    let dashboard = if no_dashboard {
        None
    } else {
        let state = dashboard::DashboardState::new(env!("CARGO_PKG_VERSION"));
        let state_clone = state.clone();
        tokio::spawn(async move { dashboard::run(state_clone, port).await });

        // Open browser (non-blocking, best-effort)
        let url = format!("http://127.0.0.1:{port}");
        if let Err(e) = open::that_detached(&url) {
            tracing::debug!("Could not open browser: {e}");
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
