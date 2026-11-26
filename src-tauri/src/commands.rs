use std::time::Duration;

use mail_core::calendar::{
    ActionItemStatus, ActionItemView, AddInviteToCalendarInput, AvailabilityQuery,
    AvailabilitySlot, CalendarEventView, CalendarInviteView, CalendarOccurrence,
    CalendarRangeQuery, DiscordWebhookStatus, IcsOverlayView, MeetingDocView, OverlayBusyBlock,
    PromoteActionItemInput, RsvpInviteInput, SaveActionItemInput, SaveCalendarEventInput,
    SaveIcsOverlayInput, UpdateMeetingDocInput,
};
use mail_core::workbook::{ImportWorkbookInput, WorkbookImportReport};
use mail_core::error::CoreError;
use mail_core::ids::{
    AccountId, ActionItemId, CalendarEventId, ContactId, DraftId, IcsOverlayId, JobId,
    PinnedLinkId, SharedTemplateId, SharedThreadId, TemplateId, ThreadId, UnixMs,
};
use mail_core::staff::{
    AddSharedCommentInput, ConnectStaffWorkspaceInput, PinnedLinkView, SavePinnedLinkInput,
    SaveSharedTemplateInput, SaveShowContactInput, ShareThreadInput, SharedCommentView,
    SharedDraftView, SharedTemplateView, SharedThreadSummary, SharedThreadView, ShowContactView,
    StaffMemberView, StaffWorkspaceView, UpdateHandoffInput, UpdateSharedDraftInput,
};
use mail_core::types::{
    AccountView, ActionOutcome, AppSettings, CreateDraftInput, DraftView, JobView, MailAction,
    MailboxCounts, NewAccountInput, OAuthSignInInput, RemoteContent, SaveTemplateInput,
    SavedOAuthClients, Scope, SendMode, SendReceipt, TemplateView, ThreadPage, ThreadQuery,
    ThreadView, UndoOutcome, UpdateAccountInput,
};
use mail_core::MailCore;
use tauri::Manager;

use crate::updater::{
    self, InstallUpdateInput, InstallUpdateOutcome, UpdateCheckResult,
};
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

// ---------------------------------------------------------------- Phase B: Staff Collaboration

#[tauri::command]
pub async fn get_staff_workspace(
    core: tauri::State<'_, MailCore>,
) -> Result<StaffWorkspaceView, CoreError> {
    core.get_staff_workspace().await
}

#[tauri::command]
pub async fn connect_staff_workspace(
    core: tauri::State<'_, MailCore>,
    input: ConnectStaffWorkspaceInput,
) -> Result<StaffWorkspaceView, CoreError> {
    core.connect_staff_workspace(input).await
}

#[tauri::command]
pub async fn disconnect_staff_workspace(
    core: tauri::State<'_, MailCore>,
) -> Result<(), CoreError> {
    core.disconnect_staff_workspace().await
}

#[tauri::command]
pub async fn sync_staff_workspace(
    core: tauri::State<'_, MailCore>,
) -> Result<StaffWorkspaceView, CoreError> {
    core.sync_staff_workspace().await
}

#[tauri::command]
pub async fn pending_staff_outbox_count(
    core: tauri::State<'_, MailCore>,
) -> Result<u32, CoreError> {
    Ok(core.pending_staff_outbox_count().await?.min(u32::MAX as usize) as u32)
}

#[tauri::command]
pub async fn list_staff_directory(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<StaffMemberView>, CoreError> {
    core.list_staff_directory().await
}

#[tauri::command]
pub async fn list_shared_threads(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<SharedThreadSummary>, CoreError> {
    core.list_shared_threads().await
}

#[tauri::command]
pub async fn open_shared_thread(
    core: tauri::State<'_, MailCore>,
    shared_thread_id: SharedThreadId,
) -> Result<SharedThreadView, CoreError> {
    core.open_shared_thread(shared_thread_id).await
}

#[tauri::command]
pub async fn find_shared_thread_for_local(
    core: tauri::State<'_, MailCore>,
    thread_id: ThreadId,
) -> Result<Option<SharedThreadView>, CoreError> {
    core.find_shared_thread_for_local(thread_id).await
}

#[tauri::command]
pub async fn share_thread(
    core: tauri::State<'_, MailCore>,
    input: ShareThreadInput,
) -> Result<SharedThreadView, CoreError> {
    core.share_thread(input).await
}

#[tauri::command]
pub async fn update_shared_handoff(
    core: tauri::State<'_, MailCore>,
    input: UpdateHandoffInput,
) -> Result<SharedThreadView, CoreError> {
    core.update_shared_handoff(input).await
}

#[tauri::command]
pub async fn add_shared_comment(
    core: tauri::State<'_, MailCore>,
    input: AddSharedCommentInput,
) -> Result<SharedCommentView, CoreError> {
    core.add_shared_comment(input).await
}

#[tauri::command]
pub async fn update_shared_draft(
    core: tauri::State<'_, MailCore>,
    input: UpdateSharedDraftInput,
) -> Result<SharedDraftView, CoreError> {
    core.update_shared_draft(input).await
}

#[tauri::command]
pub async fn list_shared_templates(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<SharedTemplateView>, CoreError> {
    core.list_shared_templates().await
}

#[tauri::command]
pub async fn save_shared_template(
    core: tauri::State<'_, MailCore>,
    input: SaveSharedTemplateInput,
) -> Result<SharedTemplateView, CoreError> {
    core.save_shared_template(input).await
}

#[tauri::command]
pub async fn delete_shared_template(
    core: tauri::State<'_, MailCore>,
    template_id: SharedTemplateId,
) -> Result<(), CoreError> {
    core.delete_shared_template(template_id).await
}

#[tauri::command]
pub async fn list_show_contacts(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<ShowContactView>, CoreError> {
    core.list_show_contacts().await
}

#[tauri::command]
pub async fn find_contact_by_email(
    core: tauri::State<'_, MailCore>,
    email: String,
) -> Result<Option<ShowContactView>, CoreError> {
    core.find_contact_by_email(email).await
}

#[tauri::command]
pub async fn save_show_contact(
    core: tauri::State<'_, MailCore>,
    input: SaveShowContactInput,
) -> Result<ShowContactView, CoreError> {
    core.save_show_contact(input).await
}

#[tauri::command]
pub async fn delete_show_contact(
    core: tauri::State<'_, MailCore>,
    contact_id: ContactId,
) -> Result<(), CoreError> {
    core.delete_show_contact(contact_id).await
}

#[tauri::command]
pub async fn list_pinned_links(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<PinnedLinkView>, CoreError> {
    core.list_pinned_links().await
}

#[tauri::command]
pub async fn save_pinned_link(
    core: tauri::State<'_, MailCore>,
    input: SavePinnedLinkInput,
) -> Result<PinnedLinkView, CoreError> {
    core.save_pinned_link(input).await
}

#[tauri::command]
pub async fn delete_pinned_link(
    core: tauri::State<'_, MailCore>,
    link_id: PinnedLinkId,
) -> Result<(), CoreError> {
    core.delete_pinned_link(link_id).await
}

#[tauri::command]
pub fn open_external_url(
    core: tauri::State<'_, MailCore>,
    url: String,
) -> Result<(), CoreError> {
    core.open_external_url(&url)
}

// ---------------------------------------------------------------- Phase C: Calendar & Meetings

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
pub async fn get_meeting_doc(
    core: tauri::State<'_, MailCore>,
    event_id: CalendarEventId,
) -> Result<MeetingDocView, CoreError> {
    core.get_meeting_doc(event_id).await
}

#[tauri::command]
pub async fn update_meeting_doc(
    core: tauri::State<'_, MailCore>,
    input: UpdateMeetingDocInput,
) -> Result<MeetingDocView, CoreError> {
    core.update_meeting_doc(input).await
}

#[tauri::command]
pub async fn promote_to_action_item(
    core: tauri::State<'_, MailCore>,
    input: PromoteActionItemInput,
) -> Result<ActionItemView, CoreError> {
    core.promote_to_action_item(input).await
}

#[tauri::command]
pub async fn toggle_action_item(
    core: tauri::State<'_, MailCore>,
    action_item_id: ActionItemId,
    status: ActionItemStatus,
) -> Result<ActionItemView, CoreError> {
    core.toggle_action_item(action_item_id, status).await
}

#[tauri::command]
pub async fn list_action_items(
    core: tauri::State<'_, MailCore>,
) -> Result<Vec<ActionItemView>, CoreError> {
    core.list_action_items().await
}

// ---------------------------------------------------------------- Phase D: Action Items & Workbook

#[tauri::command]
pub async fn save_action_item(
    core: tauri::State<'_, MailCore>,
    input: SaveActionItemInput,
) -> Result<ActionItemView, CoreError> {
    core.save_action_item(input).await
}

#[tauri::command]
pub async fn delete_action_item(
    core: tauri::State<'_, MailCore>,
    action_item_id: ActionItemId,
) -> Result<(), CoreError> {
    core.delete_action_item(action_item_id).await
}

#[tauri::command]
pub async fn import_operations_workbook(
    core: tauri::State<'_, MailCore>,
    input: ImportWorkbookInput,
) -> Result<WorkbookImportReport, CoreError> {
    core.import_operations_workbook(input).await
}

#[tauri::command]
pub async fn export_action_items_csv(
    core: tauri::State<'_, MailCore>,
) -> Result<String, CoreError> {
    core.export_action_items_csv().await
}

#[tauri::command]
pub async fn export_show_contacts_csv(
    core: tauri::State<'_, MailCore>,
) -> Result<String, CoreError> {
    core.export_show_contacts_csv().await
}

#[tauri::command]
pub async fn rsvp_calendar_invite(
    core: tauri::State<'_, MailCore>,
    input: RsvpInviteInput,
) -> Result<CalendarInviteView, CoreError> {
    core.rsvp_calendar_invite(input).await
}

#[tauri::command]
pub async fn add_invite_to_pigment_calendar(
    core: tauri::State<'_, MailCore>,
    input: AddInviteToCalendarInput,
) -> Result<CalendarEventView, CoreError> {
    core.add_invite_to_pigment_calendar(input).await
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

#[tauri::command]
pub async fn get_discord_webhook_status(
    core: tauri::State<'_, MailCore>,
) -> Result<DiscordWebhookStatus, CoreError> {
    core.get_discord_webhook_status().await
}

#[tauri::command]
pub async fn set_discord_webhook(
    core: tauri::State<'_, MailCore>,
    url: Option<String>,
) -> Result<DiscordWebhookStatus, CoreError> {
    core.set_discord_webhook(url).await
}

#[tauri::command]
pub async fn format_meeting_recap(
    core: tauri::State<'_, MailCore>,
    event_id: CalendarEventId,
) -> Result<String, CoreError> {
    core.format_meeting_recap(event_id).await
}

#[tauri::command]
pub async fn post_meeting_recap_to_discord(
    core: tauri::State<'_, MailCore>,
    event_id: CalendarEventId,
    custom_markdown: Option<String>,
) -> Result<(), CoreError> {
    core.post_meeting_recap_to_discord(event_id, custom_markdown)
        .await
}

// ---------------------------------------------------------------- In-App Updates

#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateCheckResult, CoreError> {
    updater::check_for_updates().await
}

#[tauri::command]
pub async fn download_and_install_update(
    app: tauri::AppHandle,
    input: InstallUpdateInput,
) -> Result<InstallUpdateOutcome, CoreError> {
    updater::download_and_install_update(&app, input).await
}
