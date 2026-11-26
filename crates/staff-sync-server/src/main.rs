use std::net::SocketAddr;
use std::path::PathBuf;

use staff_sync_server::{spawn_server, ServerConfig};

#[tokio::main]
async fn main() {
    let bind_raw = std::env::var("STAFF_SYNC_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let bind_addr: SocketAddr = bind_raw
        .parse()
        .unwrap_or_else(|e| panic!("invalid STAFF_SYNC_BIND '{bind_raw}': {e}"));

    let raw_db_path = std::env::var("STAFF_SYNC_DB")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("staff-sync.db"));
    let db_path = if raw_db_path.is_relative() {
        std::env::current_dir()
            .unwrap_or_else(|e| panic!("failed to resolve current directory: {e}"))
            .join(raw_db_path)
    } else {
        raw_db_path
    };

    let mut tokens: Vec<String> = Vec::new();
    if let Ok(tokens_file) = std::env::var("STAFF_SYNC_TOKENS_FILE") {
        let trimmed_path = tokens_file.trim();
        if !trimmed_path.is_empty() {
            let contents = std::fs::read_to_string(trimmed_path).unwrap_or_else(|e| {
                eprintln!("Failed to read STAFF_SYNC_TOKENS_FILE '{trimmed_path}': {e}");
                std::process::exit(1);
            });
            tokens.extend(
                contents
                    .split([',', '\n', '\r'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        }
    }
    let raw_tokens = std::env::var("STAFF_SYNC_TOKENS")
        .or_else(|_| std::env::var("STAFF_SYNC_TOKEN"))
        .unwrap_or_default();
    tokens.extend(
        raw_tokens
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    );

    if tokens.is_empty() {
        eprintln!("Set STAFF_SYNC_TOKEN, STAFF_SYNC_TOKENS, or STAFF_SYNC_TOKENS_FILE before starting.");
        std::process::exit(1);
    }

    let config = ServerConfig::with_tokens(tokens, Some(db_path.clone()));
    let server = spawn_server(bind_addr, config)
        .await
        .unwrap_or_else(|e| panic!("failed to start staff-sync-server: {e}"));

    println!(
        "Ferrule Staff Sync server listening on {} (db: {})",
        server.url(),
        db_path.display()
    );

    let _ = tokio::signal::ctrl_c().await;
    println!("Shutting down Staff Sync server...");
    server.shutdown().await;
}
