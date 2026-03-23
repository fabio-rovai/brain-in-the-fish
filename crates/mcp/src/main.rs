use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    eprintln!("Brain in the Fish MCP server starting (stdio transport)...");
    let server = brain_in_the_fish_core::server::EvalServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
