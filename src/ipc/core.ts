// Single transport boundary: every Tauri command lives here, and nothing else
// in src/ may import @tauri-apps/*. Browser preview falls through to the
// stateful mock store so the UI stays fully interactive in Vite.

import { invoke } from "@tauri-apps/api/core";
import * as mock from "./mockStore";
import type {
  AccountId,
  AccountView,
  ActionOutcome,
  AddSharedCommentInput,
  AppSettings,
  ConnectStaffWorkspaceInput,
  ContactId,
  CreateDraftInput,
  DraftId,
  DraftView,
  JobId,
  JobView,
  MailboxCounts,
  MailAction,
  NewAccountInput,
  OAuthSignInInput,
  PinnedLinkId,
  PinnedLinkView,
  RemoteContent,
  SavePinnedLinkInput,
  SaveSharedTemplateInput,
  SaveShowContactInput,
  SaveTemplateInput,
  SavedOAuthClients,
  Scope,
  SendMode,
  SendReceipt,
  ShareThreadInput,
  SharedCommentView,
  SharedDraftView,
  SharedTemplateId,
  SharedTemplateView,
  SharedThreadId,
  SharedThreadSummary,
  SharedThreadView,
  ShowContactView,
  StaffMemberView,
  StaffWorkspaceView,
  TemplateId,
  TemplateView,
  ThreadId,
  ThreadPage,
  ThreadQuery,
  ThreadView,
  UndoOutcome,
  UpdateAccountInput,
  UpdateHandoffInput,
  UpdateSharedDraftInput,
} from "../types/mail";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function healthCheck(): Promise<string> {
  if (!isTauriRuntime()) {
    return "browser-preview";
  }
  return invoke<string>("health_check");
}

export async function getSettings(): Promise<AppSettings> {
  if (!isTauriRuntime()) return mock.getSettings();
  return invoke<AppSettings>("get_settings");
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  if (!isTauriRuntime()) return mock.updateSettings(settings);
  return invoke<AppSettings>("update_settings", { settings });
}

export async function listAccounts(): Promise<AccountView[]> {
  if (!isTauriRuntime()) return mock.listAccounts();
  return invoke<AccountView[]>("list_accounts");
}

export async function addAccount(input: NewAccountInput): Promise<AccountView> {
  if (!isTauriRuntime()) return mock.addAccount(input);
  return invoke<AccountView>("add_account", { input });
}

export async function oauthSignInAccount(input: OAuthSignInInput): Promise<AccountView> {
  if (!isTauriRuntime()) return mock.oauthSignInAccount(input);
  return invoke<AccountView>("oauth_sign_in_account", { input });
}

export async function savedOAuthClients(): Promise<SavedOAuthClients> {
  if (!isTauriRuntime()) return mock.savedOAuthClients();
  return invoke<SavedOAuthClients>("saved_oauth_clients");
}

export async function updateAccount(input: UpdateAccountInput): Promise<AccountView> {
  if (!isTauriRuntime()) return mock.updateAccount(input);
  return invoke<AccountView>("update_account", { input });
}

export async function reauthOAuthAccount(accountId: AccountId): Promise<AccountView> {
  if (!isTauriRuntime()) return mock.syncAccount(accountId);
  return invoke<AccountView>("reauth_oauth_account", { accountId });
}

export async function removeAccount(accountId: AccountId): Promise<void> {
  if (!isTauriRuntime()) return mock.removeAccount(accountId);
  return invoke<void>("remove_account", { accountId });
}

export async function syncAccount(accountId: AccountId): Promise<AccountView> {
  if (!isTauriRuntime()) return mock.syncAccount(accountId);
  return invoke<AccountView>("sync_account", { accountId });
}

export async function seedDemoIfEmpty(): Promise<void> {
  if (!isTauriRuntime()) return mock.seedDemoIfEmpty();
  return invoke<void>("seed_demo_if_empty");
}

export async function listThreads(query: ThreadQuery): Promise<ThreadPage> {
  if (!isTauriRuntime()) return mock.listThreads(query);
  return invoke<ThreadPage>("list_threads", { query });
}

export async function mailboxCounts(scope: Scope): Promise<MailboxCounts> {
  if (!isTauriRuntime()) return mock.mailboxCounts(scope);
  return invoke<MailboxCounts>("mailbox_counts", { scope });
}

export async function openThread(
  threadId: ThreadId,
  remoteContent: RemoteContent,
): Promise<ThreadView> {
  if (!isTauriRuntime()) return mock.openThread(threadId, remoteContent);
  return invoke<ThreadView>("open_thread", { threadId, remoteContent });
}

export async function actOnThreads(
  action: MailAction,
  threadIds: ThreadId[],
): Promise<ActionOutcome> {
  if (!isTauriRuntime()) return mock.actOnThreads(action, threadIds);
  return invoke<ActionOutcome>("act_on_threads", { action, threadIds });
}

export async function createDraft(input: CreateDraftInput): Promise<DraftView> {
  if (!isTauriRuntime()) return mock.createDraft(input);
  return invoke<DraftView>("create_draft", { input });
}

export async function sendDraft(draftId: DraftId, mode: SendMode): Promise<SendReceipt> {
  if (!isTauriRuntime()) return mock.sendDraft(draftId, mode);
  return invoke<SendReceipt>("send_draft", { draftId, mode });
}

export async function cancelSend(jobId: JobId): Promise<UndoOutcome> {
  if (!isTauriRuntime()) return mock.cancelSend(jobId);
  return invoke<UndoOutcome>("cancel_send", { jobId });
}

export async function snoozeThread(threadId: ThreadId, until: number): Promise<void> {
  if (!isTauriRuntime()) return mock.snoozeThread(threadId, until);
  return invoke<void>("snooze_thread", { threadId, until });
}

export async function unsnoozeThread(threadId: ThreadId): Promise<void> {
  if (!isTauriRuntime()) return mock.unsnoozeThread(threadId);
  return invoke<void>("unsnooze_thread", { threadId });
}

export async function setFollowUp(threadId: ThreadId, dueAt: number): Promise<JobId> {
  if (!isTauriRuntime()) return mock.setFollowUp(threadId, dueAt);
  return invoke<JobId>("set_follow_up", { threadId, dueAt });
}

export async function dismissFollowUp(jobId: JobId): Promise<void> {
  if (!isTauriRuntime()) return mock.dismissFollowUp(jobId);
  return invoke<void>("dismiss_follow_up", { jobId });
}

export async function listJobs(): Promise<JobView[]> {
  if (!isTauriRuntime()) return mock.listJobs();
  return invoke<JobView[]>("list_jobs");
}

export async function listTemplates(): Promise<TemplateView[]> {
  if (!isTauriRuntime()) return mock.listTemplates();
  return invoke<TemplateView[]>("list_templates");
}

export async function saveTemplate(input: SaveTemplateInput): Promise<TemplateView> {
  if (!isTauriRuntime()) return mock.saveTemplate(input);
  return invoke<TemplateView>("save_template", { input });
}

export async function deleteTemplate(templateId: TemplateId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteTemplate(templateId);
  return invoke<void>("delete_template", { templateId });
}

export async function tickScheduler(): Promise<void> {
  if (!isTauriRuntime()) return mock.tickScheduler();
  return invoke<void>("tick_scheduler");
}

// ── Phase B: Staff Sync & Email Collaboration ───────────────────────────────

export async function getStaffWorkspace(): Promise<StaffWorkspaceView> {
  if (!isTauriRuntime()) return mock.getStaffWorkspace();
  return invoke<StaffWorkspaceView>("get_staff_workspace");
}

export async function connectStaffWorkspace(
  input: ConnectStaffWorkspaceInput,
): Promise<StaffWorkspaceView> {
  if (!isTauriRuntime()) return mock.connectStaffWorkspace(input);
  return invoke<StaffWorkspaceView>("connect_staff_workspace", { input });
}

export async function disconnectStaffWorkspace(): Promise<void> {
  if (!isTauriRuntime()) return mock.disconnectStaffWorkspace();
  return invoke<void>("disconnect_staff_workspace");
}

export async function syncStaffWorkspace(): Promise<StaffWorkspaceView> {
  if (!isTauriRuntime()) return mock.syncStaffWorkspace();
  return invoke<StaffWorkspaceView>("sync_staff_workspace");
}

export async function listStaffDirectory(): Promise<StaffMemberView[]> {
  if (!isTauriRuntime()) return mock.listStaffDirectory();
  return invoke<StaffMemberView[]>("list_staff_directory");
}

export async function listSharedThreads(): Promise<SharedThreadSummary[]> {
  if (!isTauriRuntime()) return mock.listSharedThreads();
  return invoke<SharedThreadSummary[]>("list_shared_threads");
}

export async function openSharedThread(
  sharedThreadId: SharedThreadId,
): Promise<SharedThreadView> {
  if (!isTauriRuntime()) return mock.openSharedThread(sharedThreadId);
  return invoke<SharedThreadView>("open_shared_thread", { sharedThreadId });
}

export async function findSharedThreadForLocal(
  threadId: ThreadId,
): Promise<SharedThreadView | null> {
  if (!isTauriRuntime()) return mock.findSharedThreadForLocal(threadId);
  return invoke<SharedThreadView | null>("find_shared_thread_for_local", { threadId });
}

export async function shareThread(input: ShareThreadInput): Promise<SharedThreadView> {
  if (!isTauriRuntime()) return mock.shareThread(input);
  return invoke<SharedThreadView>("share_thread", { input });
}

export async function updateSharedHandoff(
  input: UpdateHandoffInput,
): Promise<SharedThreadView> {
  if (!isTauriRuntime()) return mock.updateSharedHandoff(input);
  return invoke<SharedThreadView>("update_shared_handoff", { input });
}

export async function addSharedComment(
  input: AddSharedCommentInput,
): Promise<SharedCommentView> {
  if (!isTauriRuntime()) return mock.addSharedComment(input);
  return invoke<SharedCommentView>("add_shared_comment", { input });
}

export async function updateSharedDraft(
  input: UpdateSharedDraftInput,
): Promise<SharedDraftView> {
  if (!isTauriRuntime()) return mock.updateSharedDraft(input);
  return invoke<SharedDraftView>("update_shared_draft", { input });
}

export async function listSharedTemplates(): Promise<SharedTemplateView[]> {
  if (!isTauriRuntime()) return mock.listSharedTemplates();
  return invoke<SharedTemplateView[]>("list_shared_templates");
}

export async function saveSharedTemplate(
  input: SaveSharedTemplateInput,
): Promise<SharedTemplateView> {
  if (!isTauriRuntime()) return mock.saveSharedTemplate(input);
  return invoke<SharedTemplateView>("save_shared_template", { input });
}

export async function deleteSharedTemplate(templateId: SharedTemplateId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteSharedTemplate(templateId);
  return invoke<void>("delete_shared_template", { templateId });
}

export async function listShowContacts(): Promise<ShowContactView[]> {
  if (!isTauriRuntime()) return mock.listShowContacts();
  return invoke<ShowContactView[]>("list_show_contacts");
}

export async function findContactByEmail(email: string): Promise<ShowContactView | null> {
  if (!isTauriRuntime()) return mock.findContactByEmail(email);
  return invoke<ShowContactView | null>("find_contact_by_email", { email });
}

export async function saveShowContact(
  input: SaveShowContactInput,
): Promise<ShowContactView> {
  if (!isTauriRuntime()) return mock.saveShowContact(input);
  return invoke<ShowContactView>("save_show_contact", { input });
}

export async function deleteShowContact(contactId: ContactId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteShowContact(contactId);
  return invoke<void>("delete_show_contact", { contactId });
}

export async function listPinnedLinks(): Promise<PinnedLinkView[]> {
  if (!isTauriRuntime()) return mock.listPinnedLinks();
  return invoke<PinnedLinkView[]>("list_pinned_links");
}

export async function savePinnedLink(input: SavePinnedLinkInput): Promise<PinnedLinkView> {
  if (!isTauriRuntime()) return mock.savePinnedLink(input);
  return invoke<PinnedLinkView>("save_pinned_link", { input });
}

export async function deletePinnedLink(linkId: PinnedLinkId): Promise<void> {
  if (!isTauriRuntime()) return mock.deletePinnedLink(linkId);
  return invoke<void>("delete_pinned_link", { linkId });
}

export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauriRuntime()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  return invoke<void>("open_external_url", { url });
}
