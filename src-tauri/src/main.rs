#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        if let Err(err) = rt.block_on(mail_core::mcp::run_stdio_bridge(None)) {
            eprintln!("ferrule mcp: {err}");
            std::process::exit(1);
        }
        return;
    }
    ferrule_lib::run();
}
