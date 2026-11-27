use std::path::PathBuf;
use std::time::Duration;

use mail_core::calendar::{
    AddInviteToCalendarInput, AvailabilityQuery, AvailabilitySlot, CalendarEventView,
    CalendarInviteView, CalendarOccurrence, CalendarRangeQuery, IcsOverlayView, OverlayBusyBlock,
    RsvpInviteInput, SaveCalendarEventInput, SaveIcsOverlayInput,
};
use mail_core::error::CoreError;
use mail_core::ids::{
    AccountId, AttachmentId, CalendarEventId, DraftId, IcsOverlayId, JobId, SignatureId,
    TemplateId, ThreadId, UnixMs,
};
use mail_core::types::{
    AccountView, ActionOutcome, AppSettings, AttachmentData, CreateDraftInput, DraftView, JobView,
    MailAction, MailboxCounts, NewAccountInput, OAuthSignInInput, RemoteContent,
    SaveSignatureInput, SaveTemplateInput, SavedOAuthClients, Scope, SendMode, SendReceipt,
    SignatureView, TemplateView, ThreadPage, ThreadQuery, ThreadView, UndoOutcome,
    UpdateAccountInput,
};
use mail_core::MailCore;
use tauri::Manager;

use crate::SplashState;

const MIN_SPLASH_DWELL: Duration = Duration::from_millis(2400);

#[tauri::command]
pub fn health_check(core: tauri::State<'_, MailCore>) -> Result<String, CoreError> {
    core.db_health_check()?;
    Ok("ok".to_string())
}

#[tauri::command]
pub async fn app_ready(
    app: tauri::AppHandle,
    splash: tauri::State<'_, SplashState>,
) -> Result<(), CoreError> {
    if let Some(started) = splash.take_started_at() {
        let elapsed = started.elapsed();
        if let Some(remaining) = MIN_SPLASH_DWELL.checked_sub(elapsed) {
            tokio::time::sleep(remaining).await;
        }
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.show();
            let _ = main.set_focus();
        }
        if let Some(splash_window) = app.get_webview_window("splash") {
            let _ = splash_window.close();
        }
    } else if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
    }
    Ok(())
}

#[tauri::command]
pub async fn list_accounts(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<AccountView>, CoreError> {
    core.list_accounts().await
}

#[tauri::command]
pub async fn add_account(
    core: tauri::State<'_, MailCore>,
    input: NewAccountInput,
) -> Result<AccountView, CoreError> {
    core.add_account(input).await
}

#[tauri::command]
pub async fn oauth_sign_in_account(
    core: tauri::State<'_, MailCore>,
    input: OAuthSignInInput,
) -> Result<AccountView, CoreError> {
    core.oauth_sign_in_account(input).await
}

#[tauri::command]
pub async fn saved_oauth_clients(
    core: tauri::State<'_, MailCore>,
) -> Result<SavedOAuthClients, CoreError> {
    core.saved_oauth_clients().await
}

#[tauri::command]
pub async fn update_account(
    core: tauri::State<'_, MailCore>,
    input: UpdateAccountInput,
) -> Result<AccountView, CoreError> {
    core.update_account(input).await
}

#[tauri::command]
pub async fn reauth_oauth_account(
    core: tauri::State<'_, MailCore>,
    account_id: AccountId,
) -> Result<AccountView, CoreError> {
    core.reauth_oauth_account(account_id).await
}

#[tauri::command]
pub async fn remove_account(
    core: tauri::State<'_, MailCore>,
    account_id: AccountId,
) -> Result<(), CoreError> {
    core.remove_account(account_id).await
}

#[tauri::command]
pub async fn sync_account(
    core: tauri::State<'_, MailCore>,
    account_id: AccountId,
) -> Result<AccountView, CoreError> {
    core.sync_account(account_id).await
}

#[tauri::command]
pub async fn list_threads(
    core: tauri::State<'_, MailCore>,
    query: ThreadQuery,
) -> Result<ThreadPage, CoreError> {
    core.list_threads(query).await
}

#[tauri::command]
pub async fn mailbox_counts(
    core: tauri::State<'_, MailCore>,
    scope: Scope,
) -> Result<MailboxCounts, CoreError> {
    core.mailbox_counts(scope).await
}

#[tauri::command]
pub async fn open_thread(
    core: tauri::State<'_, MailCore>,
    thread_id: ThreadId,
    remote_content: RemoteContent,
) -> Result<ThreadView, CoreError> {
    core.open_thread(thread_id, remote_content).await
}

#[tauri::command]
pub async fn act_on_threads(
    core: tauri::State<'_, MailCore>,
    action: MailAction,
    thread_ids: Vec<ThreadId>,
) -> Result<ActionOutcome, CoreError> {
    core.act_on_threads(action, thread_ids).await
}

#[tauri::command]
pub async fn create_draft(
    core: tauri::State<'_, MailCore>,
    input: CreateDraftInput,
) -> Result<DraftView, CoreError> {
    core.create_draft(input).await
}

#[tauri::command]
pub async fn send_draft(
    core: tauri::State<'_, MailCore>,
    draft_id: DraftId,
    mode: SendMode,
) -> Result<SendReceipt, CoreError> {
    core.send_draft(draft_id, mode).await
}

#[tauri::command]
pub async fn cancel_send(
    core: tauri::State<'_, MailCore>,
    job_id: JobId,
) -> Result<UndoOutcome, CoreError> {
    core.cancel_send(job_id).await
}

#[tauri::command]
pub async fn snooze_thread(
    core: tauri::State<'_, MailCore>,
    thread_id: ThreadId,
    until: UnixMs,
) -> Result<(), CoreError> {
    let _ = core.snooze_thread(thread_id, until).await?;
    Ok(())
}

#[tauri::command]
pub async fn unsnooze_thread(
    core: tauri::State<'_, MailCore>,
    thread_id: ThreadId,
) -> Result<(), CoreError> {
    core.unsnooze_thread(thread_id).await
}

#[tauri::command]
pub async fn set_follow_up(
    core: tauri::State<'_, MailCore>,
    thread_id: ThreadId,
    due_at: UnixMs,
) -> Result<JobId, CoreError> {
    let job = core.set_follow_up(thread_id, due_at).await?;
    Ok(job.id)
}

#[tauri::command]
pub async fn dismiss_follow_up(
    core: tauri::State<'_, MailCore>,
    job_id: JobId,
) -> Result<(), CoreError> {
    core.dismiss_follow_up(job_id).await
}

#[tauri::command]
pub async fn list_jobs(core: tauri::State<'_, MailCore>) -> Result<Vec<JobView>, CoreError> {
    core.list_jobs().await
}

#[tauri::command]
pub async fn list_templates(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<TemplateView>, CoreError> {
    core.list_templates().await
}

#[tauri::command]
pub async fn save_template(
    core: tauri::State<'_, MailCore>,
    input: SaveTemplateInput,
) -> Result<TemplateView, CoreError> {
    core.save_template(input).await
}

#[tauri::command]
pub async fn delete_template(
    core: tauri::State<'_, MailCore>,
    template_id: TemplateId,
) -> Result<(), CoreError> {
    core.delete_template(template_id).await
}

#[tauri::command]
pub async fn download_attachment(
    core: tauri::State<'_, MailCore>,
    attachment_id: AttachmentId,
) -> Result<AttachmentData, CoreError> {
    core.download_attachment(attachment_id).await
}

#[tauri::command]
pub async fn save_attachment(
    core: tauri::State<'_, MailCore>,
    attachment_id: AttachmentId,
    dest_path: String,
) -> Result<String, CoreError> {
    core.save_attachment(attachment_id, PathBuf::from(dest_path))
        .await
}

#[tauri::command]
pub async fn list_signatures(
    core: tauri::State<'_, MailCore>,
    account_id: Option<AccountId>,
) -> Result<Vec<SignatureView>, CoreError> {
    core.list_signatures(account_id).await
}

#[tauri::command]
pub async fn save_signature(
    core: tauri::State<'_, MailCore>,
    input: SaveSignatureInput,
) -> Result<SignatureView, CoreError> {
    core.save_signature(input).await
}

#[tauri::command]
pub async fn delete_signature(
    core: tauri::State<'_, MailCore>,
    signature_id: SignatureId,
) -> Result<(), CoreError> {
    core.delete_signature(signature_id).await
}

#[tauri::command]
pub async fn get_settings(core: tauri::State<'_, MailCore>) -> Result<AppSettings, CoreError> {
    core.get_settings().await
}

#[tauri::command]
pub async fn update_settings(
    core: tauri::State<'_, MailCore>,
    settings: AppSettings,
) -> Result<AppSettings, CoreError> {
    core.update_settings(settings).await
}

#[tauri::command]
pub async fn tick_scheduler(core: tauri::State<'_, MailCore>) -> Result<(), CoreError> {
    let _ = core.tick_scheduler().await?;
    Ok(())
}

#[tauri::command]
pub fn open_external_url(
    core: tauri::State<'_, MailCore>,
    url: String,
) -> Result<(), CoreError> {
    core.open_external_url(&url)
}

// ---------------------------------------------------------------- Personal Calendar

#[tauri::command]
pub async fn list_calendar_events(
    core: tauri::State<'_, MailCore>,
    query: CalendarRangeQuery,
) -> Result<Vec<CalendarOccurrence>, CoreError> {
    core.list_calendar_events(query).await
}

#[tauri::command]
pub async fn save_calendar_event(
    core: tauri::State<'_, MailCore>,
    input: SaveCalendarEventInput,
) -> Result<CalendarEventView, CoreError> {
    core.save_calendar_event(input).await
}

#[tauri::command]
pub async fn quick_add_calendar_event(
    core: tauri::State<'_, MailCore>,
    text: String,
) -> Result<CalendarEventView, CoreError> {
    core.quick_add_calendar_event(text).await
}

#[tauri::command]
pub async fn delete_calendar_event(
    core: tauri::State<'_, MailCore>,
    event_id: CalendarEventId,
) -> Result<(), CoreError> {
    core.delete_calendar_event(event_id).await
}

#[tauri::command]
pub async fn rsvp_calendar_invite(
    core: tauri::State<'_, MailCore>,
    input: RsvpInviteInput,
) -> Result<CalendarInviteView, CoreError> {
    core.rsvp_calendar_invite(input).await
}

#[tauri::command]
pub async fn add_invite_to_calendar(
    core: tauri::State<'_, MailCore>,
    input: AddInviteToCalendarInput,
) -> Result<CalendarEventView, CoreError> {
    core.add_invite_to_calendar(input).await
}

#[tauri::command]
pub async fn list_ics_overlays(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<IcsOverlayView>, CoreError> {
    core.list_ics_overlays().await
}

#[tauri::command]
pub async fn save_ics_overlay(
    core: tauri::State<'_, MailCore>,
    input: SaveIcsOverlayInput,
) -> Result<IcsOverlayView, CoreError> {
    core.save_ics_overlay(input).await
}

#[tauri::command]
pub async fn delete_ics_overlay(
    core: tauri::State<'_, MailCore>,
    overlay_id: IcsOverlayId,
) -> Result<(), CoreError> {
    core.delete_ics_overlay(overlay_id).await
}

#[tauri::command]
pub async fn refresh_ics_overlay(
    core: tauri::State<'_, MailCore>,
    overlay_id: IcsOverlayId,
) -> Result<IcsOverlayView, CoreError> {
    core.refresh_ics_overlay(overlay_id).await
}

#[tauri::command]
pub async fn list_overlay_busy_blocks(
    core: tauri::State<'_, MailCore>,
    query: CalendarRangeQuery,
) -> Result<Vec<OverlayBusyBlock>, CoreError> {
    core.list_overlay_busy_blocks(query).await
}

#[tauri::command]
pub async fn find_availability_slots(
    core: tauri::State<'_, MailCore>,
    query: AvailabilityQuery,
) -> Result<Vec<AvailabilitySlot>, CoreError> {
    core.find_availability_slots(query).await
}
