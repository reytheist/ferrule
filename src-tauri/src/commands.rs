use mail_core::error::CoreError;
use mail_core::MailCore;

#[tauri::command]
pub fn health_check(core: tauri::State<'_, MailCore>) -> Result<String, CoreError> {
    core.db_health_check()?;
    Ok("ok".to_string())
}
