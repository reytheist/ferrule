use mail_core::error::CoreError;
use mail_core::ids::{AccountId, DraftId, JobId, TemplateId, ThreadId, UnixMs};
use mail_core::types::{
    AccountView, ActionOutcome, CreateDraftInput, DraftView, JobView, MailAction, MailboxCounts,
    NewAccountInput, RemoteContent, SaveTemplateInput, Scope, SendMode, SendReceipt, TemplateView,
    ThreadPage, ThreadQuery, ThreadView, UndoOutcome,
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
pub async fn tick_scheduler(core: tauri::State<'_, MailCore>) -> Result<(), CoreError> {
    let _ = core.tick_scheduler().await?;
    Ok(())
}
