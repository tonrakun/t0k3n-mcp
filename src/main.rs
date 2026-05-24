use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

mod server;

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
    let root = args
        .windows(2)
        .find(|w| w[0] == "--root")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| ".".to_string());

    tracing::info!("Starting t0k3n-mcp with root: {}", root);

    let transport = stdio();
    let server = server::T0k3nServer::new(root);
    let service = server.serve(transport).await.inspect_err(|e| {
        tracing::error!("Server error: {}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
