mod commands;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use mail_core::{CoreConfig, Host, MailCore};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) struct SplashState {
    started_at: Mutex<Option<Instant>>,
}

impl SplashState {
    fn new(started_at: Option<Instant>) -> Self {
        Self {
            started_at: Mutex::new(started_at),
        }
    }

    pub(crate) fn take_started_at(&self) -> Option<Instant> {
        self.started_at.lock().ok().and_then(|mut guard| guard.take())
    }
}

#[cfg(target_os = "macos")]
fn install_macos_app_menu(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{PredefinedMenuItem, SubmenuBuilder};

    let app_menu = SubmenuBuilder::new(app, "Ferrule")
        .item(&PredefinedMenuItem::about(app, Some("About Ferrule"), None)?)
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &edit_menu, &window_menu])
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            install_macos_app_menu(app)?;

            let db_path = app
                .path()
                .app_data_dir()
                .map(|dir| dir.join("ferrule.db"))
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

            // Start per-account async IMAP sync actors (with interruptible IDLE, fair-slice
            // offline action queue, smart pacing & auth circuit breaker) and the dedicated
            // durable SMTP submission worker.
            core.start_background_services();

            // Periodic refresh for enabled read-only personal .ics overlays (~14 min).
            let ics_core = core.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let mut interval = tokio::time::interval(Duration::from_secs(14 * 60));
                loop {
                    interval.tick().await;
                    if let Ok(overlays) = ics_core.list_ics_overlays().await {
                        for overlay in overlays.into_iter().filter(|o| o.enabled) {
                            let _ = ics_core.refresh_ics_overlay(overlay.id).await;
                        }
                    }
                }
            });

            // Lightweight system tray icon for quick open/quit.
            let show_item = MenuItemBuilder::with_id("show", "Show Ferrule").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(cfg!(target_os = "macos"))
                .tooltip("Ferrule")
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

            let first_run_splash = core.needs_first_run_setup().unwrap_or(false);
            let splash_started = if first_run_splash {
                match WebviewWindowBuilder::new(
                    app,
                    "splash",
                    WebviewUrl::App("splash.html".into()),
                )
                .title("Ferrule")
                .inner_size(460.0, 300.0)
                .decorations(false)
                .resizable(false)
                .center()
                .always_on_top(true)
                .visible(true)
                .build()
                {
                    Ok(_) => Some(Instant::now()),
                    Err(_) => {
                        if let Some(main) = app.get_webview_window("main") {
                            let _ = main.show();
                            let _ = main.set_focus();
                        }
                        None
                    }
                }
            } else {
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.show();
                    let _ = main.set_focus();
                }
                None
            };

            app.manage(SplashState::new(splash_started));
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::app_ready,
            commands::get_settings,
            commands::update_settings,
            commands::list_accounts,
            commands::add_account,
            commands::oauth_sign_in_account,
            commands::saved_oauth_clients,
            commands::update_account,
            commands::reauth_oauth_account,
            commands::remove_account,
            commands::sync_account,
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
            commands::download_attachment,
            commands::save_attachment,
            commands::list_signatures,
            commands::save_signature,
            commands::delete_signature,
            commands::tick_scheduler,
            commands::open_external_url,
            commands::list_calendar_events,
            commands::save_calendar_event,
            commands::quick_add_calendar_event,
            commands::delete_calendar_event,
            commands::rsvp_calendar_invite,
            commands::add_invite_to_calendar,
            commands::list_ics_overlays,
            commands::save_ics_overlay,
            commands::delete_ics_overlay,
            commands::refresh_ics_overlay,
            commands::list_overlay_busy_blocks,
            commands::find_availability_slots,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Ferrule");

    app.run(|_app_handle, _event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows,
            ..
        } = _event
        {
            if !has_visible_windows {
                if let Some(window) = _app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }
    });
}
