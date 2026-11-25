mod commands;

use std::time::Duration;

use mail_core::{CoreConfig, Host, MailCore};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

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

            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
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
            commands::get_staff_workspace,
            commands::connect_staff_workspace,
            commands::disconnect_staff_workspace,
            commands::sync_staff_workspace,
            commands::list_staff_directory,
            commands::list_shared_threads,
            commands::open_shared_thread,
            commands::find_shared_thread_for_local,
            commands::share_thread,
            commands::update_shared_handoff,
            commands::add_shared_comment,
            commands::update_shared_draft,
            commands::list_shared_templates,
            commands::save_shared_template,
            commands::delete_shared_template,
            commands::list_show_contacts,
            commands::find_contact_by_email,
            commands::save_show_contact,
            commands::delete_show_contact,
            commands::list_pinned_links,
            commands::save_pinned_link,
            commands::delete_pinned_link,
            commands::open_external_url,
            commands::list_calendar_events,
            commands::save_calendar_event,
            commands::quick_add_calendar_event,
            commands::delete_calendar_event,
            commands::get_meeting_doc,
            commands::update_meeting_doc,
            commands::promote_to_action_item,
            commands::toggle_action_item,
            commands::list_action_items,
            commands::save_action_item,
            commands::delete_action_item,
            commands::import_operations_workbook,
            commands::export_action_items_csv,
            commands::export_show_contacts_csv,
            commands::rsvp_calendar_invite,
            commands::add_invite_to_pigment_calendar,
            commands::list_ics_overlays,
            commands::save_ics_overlay,
            commands::delete_ics_overlay,
            commands::refresh_ics_overlay,
            commands::list_overlay_busy_blocks,
            commands::find_availability_slots,
            commands::get_discord_webhook_status,
            commands::set_discord_webhook,
            commands::format_meeting_recap,
            commands::post_meeting_recap_to_discord,
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
