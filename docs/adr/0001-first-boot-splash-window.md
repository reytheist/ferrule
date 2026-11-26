# First-boot-only frameless Launch Splash window

Start the `main` Tauri window hidden in `tauri.conf.json` and have `src-tauri/src/lib.rs` check `AppSettings::onboarding_completed` and `account_count` immediately after opening `MailCore`. Only when setup has not been completed and no accounts exist does Ferrule spawn a dedicated frameless `splash` window (held for a ~2.4s minimum dwell until the main webview signals ready), then close it and reveal `main` on Step 1 of Onboarding; on all subsequent launches, `splash` is never created and `main` is shown immediately.

## Considered Options

- **In-window React boot screen**: Simpler to wire, but flashes the full framed desktop window chrome before content mounts and gives no native splash feel on first run.
- **Always-on splash window**: Adds unnecessary startup latency (~1.2s) to every daily launch for a low-memory desktop email client that otherwise opens in under 100ms.
