use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

mod security;
mod server;
mod startup;
mod update;

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

    tracing::info!("Starting t0k3n-mcp with root: {}", root);

    // Non-blocking background update check
    update::spawn_update_check();

    // Detect workspace languages at startup
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

    let transport = stdio();
    let server = server::T0k3nServer::new(root);
    let service = server.serve(transport).await.inspect_err(|e| {
        tracing::error!("Server error: {}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
