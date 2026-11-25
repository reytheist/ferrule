use mail_core::error::CoreError;
use mail_core::ids::{
    AccountId, ContactId, DraftId, JobId, PinnedLinkId, SharedTemplateId, SharedThreadId,
    TemplateId, ThreadId, UnixMs,
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

#[tauri::command]
pub fn health_check(core: tauri::State<'_, MailCore>) -> Result<String, CoreError> {
    core.db_health_check()?;
    Ok("ok".to_string())
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
pub async fn seed_demo_if_empty(core: tauri::State<'_, MailCore>) -> Result<(), CoreError> {
    let _ = core.seed_demo_if_empty().await?;
    Ok(())
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
