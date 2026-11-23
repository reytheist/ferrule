mod commands;

use mail_core::{CoreConfig, Host, MailCore};
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db_path = app
                .path()
                .app_data_dir()
                .map(|dir| dir.join("galapaghost.db"))
                .ok();
            let config = CoreConfig {
                db_path,
                ..CoreConfig::default()
            };
            let core = MailCore::open(config, Host::desktop())
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::health_check])
        .run(tauri::generate_context!())
        .expect("error while running Galapaghost");
}
