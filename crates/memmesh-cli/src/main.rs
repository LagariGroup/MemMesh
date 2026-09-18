use clap::{Parser, Subcommand};
use memmesh_core::SearchResultItem;
use memmesh_server::{create_router, AppState};
use memmesh_storage::StorageEngine;
use std::io::{self, BufRead};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "memmesh",
    about = "MemMesh: Universal Agent Memory Fabric[cite: 2]"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MemMesh background HTTP/SSE daemon[cite: 2]
    Daemon {
        #[arg(long, default_value = "8740")]
        port: u16,
        #[arg(long, default_value = "./memmesh.db")]
        db_path: String,
        #[arg(long)]
        vault_path: Option<String>,
        #[arg(long)]
        dev: bool,
    },
    /// Run MCP in stdio mode for Claude Desktop / local processes[cite: 2]
    McpStdio {
        #[arg(long)]
        remote: Option<String>,
    },
    /// Query context for agent harness injection[cite: 2]
    Context {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value = "3")]
        limit: usize,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(long, default_value = "http://localhost:8740")]
        remote: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon {
            port,
            db_path,
            vault_path,
            dev: _,
        } => {
            let storage = Arc::new(StorageEngine::new(db_path, vault_path)?);
            let state = AppState {
                storage,
                auth_token: std::env::var("MEMMESH_AUTH_TOKEN").ok(),
            };

            let app = create_router(state);
            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            println!("MemMesh daemon listening on http://{}", addr);

            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app).await?;
        }
        Commands::McpStdio { remote: _ } => {
            let storage = Arc::new(StorageEngine::new_in_memory()?);
            let stdin = io::stdin();
            let mut stdout = io::stdout();

            for line in stdin.lock().lines() {
                let text = match line {
                    Ok(t) => t,
                    Err(_) => break,
                };

                if let Ok(req) = serde_json::from_str::<memmesh_server::mcp::JsonRpcRequest>(&text)
                {
                    let resp = memmesh_server::mcp::handle_mcp_request(req, storage.clone());
                    let out = serde_json::to_string(&resp)?;
                    use std::io::Write;
                    writeln!(stdout, "{}", out)?;
                    stdout.flush()?;
                }
            }
        }
        Commands::Context {
            project: _,
            query,
            limit,
            format: _,
            remote,
        } => {
            let q = query.unwrap_or_else(|| "project conventions and rules".to_string());
            let client = reqwest::Client::new();
            let res = client
                .post(format!("{}/v1/memories/search", remote))
                .json(&serde_json::json!({
                    "query": q,
                    "limit": limit
                }))
                .send()
                .await;

            match res {
                Ok(resp) => {
                    let data: serde_json::Value = resp.json().await.unwrap_or_default();
                    if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                        println!("### MemMesh Context Injection");
                        for item in results {
                            let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                            let content =
                                item.get("content").and_then(|c| c.as_str()).unwrap_or("");
                            println!("\n#### {}\n{}", title, content);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Failed to reach MemMesh daemon: {}", err);
                }
            }
        }
    }

    Ok(())
}
