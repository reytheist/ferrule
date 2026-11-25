use std::net::SocketAddr;
use std::path::PathBuf;

use staff_sync_server::{spawn_server, ServerConfig};

#[tokio::main]
async fn main() {
    let bind_raw = std::env::var("STAFF_SYNC_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let bind_addr: SocketAddr = bind_raw
        .parse()
        .unwrap_or_else(|e| panic!("invalid STAFF_SYNC_BIND '{bind_raw}': {e}"));

    let db_path = std::env::var("STAFF_SYNC_DB")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("staff-sync.db")));

    let raw_tokens = std::env::var("STAFF_SYNC_TOKENS")
        .or_else(|_| std::env::var("STAFF_SYNC_TOKEN"))
        .unwrap_or_default();
    let tokens: Vec<String> = raw_tokens
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if tokens.is_empty() {
        eprintln!("Set STAFF_SYNC_TOKEN or STAFF_SYNC_TOKENS (comma-separated) before starting.");
        std::process::exit(1);
    }

    let config = ServerConfig::with_tokens(tokens, db_path.clone());
    let server = spawn_server(bind_addr, config)
        .await
        .unwrap_or_else(|e| panic!("failed to start staff-sync-server: {e}"));

    println!(
        "Ferrule Staff Sync server listening on {} (db: {})",
        server.url(),
        db_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ":memory:".to_string())
    );

    let _ = tokio::signal::ctrl_c().await;
    println!("Shutting down Staff Sync server...");
    server.shutdown().await;
}
