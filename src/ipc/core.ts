// Single transport boundary: every Tauri command lives here, and nothing else
// in src/ may import @tauri-apps/*. Browser preview falls through to the
// stateful mock store so the UI stays fully interactive in Vite.

import { invoke } from "@tauri-apps/api/core";
import * as mock from "./mockStore";
import type {
  AccountId,
  AccountView,
  ActionOutcome,
  CreateDraftInput,
  DraftId,
  DraftView,
  JobId,
  JobView,
  MailboxCounts,
  MailAction,
  NewAccountInput,
  RemoteContent,
  SaveTemplateInput,
  Scope,
  SendMode,
  SendReceipt,
  TemplateId,
  TemplateView,
  ThreadId,
  ThreadPage,
  ThreadQuery,
  ThreadView,
  UndoOutcome,
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

export async function listAccounts(): Promise<AccountView[]> {
  if (!isTauriRuntime()) return mock.listAccounts();
  return invoke<AccountView[]>("list_accounts");
}

export async function addAccount(input: NewAccountInput): Promise<AccountView> {
  if (!isTauriRuntime()) return mock.addAccount(input);
  return invoke<AccountView>("add_account", { input });
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
