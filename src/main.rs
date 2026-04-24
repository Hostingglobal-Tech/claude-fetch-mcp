mod cache;
mod fetch;
mod mcp;
mod tools;

use anyhow::Result;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("CLAUDE_FETCH_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "claude-fetch-mcp starting");

    let ctx = tools::Context::new().await?;
    mcp::run_stdio(ctx).await
}
