//! Thin console binary (`ferrule-mcp`) speaking MCP JSON-RPC 2.0 over stdio
//! and forwarding JSON-line requests over the user-only local named pipe (Windows)
//! or Unix domain socket (Linux/macOS) to the running Ferrule desktop app.

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    if let Err(err) = rt.block_on(mail_core::mcp::run_stdio_bridge(None)) {
        eprintln!("ferrule-mcp: {err}");
        std::process::exit(1);
    }
}
