mod commands;

use std::time::Duration;

use mail_core::{CoreConfig, Host, MailCore};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

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

            // Forward core domain events to the Mosaic frontend.
            let mut rx = core.subscribe();
            let event_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    let _ = event_handle.emit("mail://event", &event);
                }
            });

            // Background scheduler tick so Undo Send, Send Later, Snooze, and
            // Follow-ups fire cleanly while the desktop app stays resident.
            let sched_core = core.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let _ = sched_core.tick_scheduler().await;
                }
            });

            // Lightweight system tray icon for quick open/quit.
            let show_item = MenuItemBuilder::with_id("show", "Show Galapaghost").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Galapaghost")
                .on_menu_event(|app_handle, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            let _ = tray.build(app)?;

            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::list_accounts,
            commands::add_account,
            commands::oauth_sign_in_account,
            commands::saved_oauth_clients,
            commands::update_account,
            commands::reauth_oauth_account,
            commands::remove_account,
            commands::sync_account,
            commands::seed_demo_if_empty,
            commands::list_threads,
            commands::mailbox_counts,
            commands::open_thread,
            commands::act_on_threads,
            commands::create_draft,
            commands::send_draft,
            commands::cancel_send,
            commands::snooze_thread,
            commands::unsnooze_thread,
            commands::set_follow_up,
            commands::dismiss_follow_up,
            commands::list_jobs,
            commands::list_templates,
            commands::save_template,
            commands::delete_template,
            commands::tick_scheduler,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Galapaghost");
}
