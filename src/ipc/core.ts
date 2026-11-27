// Single transport boundary: every Tauri command lives here, and nothing else
// in src/ may import @tauri-apps/*. Browser preview falls through to the
// stateful mock store so the UI stays fully interactive in Vite.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as mock from "./mockStore";
import type {
  AccountId,
  AccountView,
  ActionOutcome,
  AddInviteToCalendarInput,
  AgentAccountPermissionView,
  AgentActivityId,
  AgentActivityView,
  AgentBridgeConfig,
  AiAnswerResult,
  AiConfigView,
  AiDraftResult,
  AiSummaryResult,
  AppSettings,
  AttachmentData,
  AttachmentId,
  AvailabilityQuery,
  AvailabilitySlot,
  CalendarEventId,
  CalendarEventView,
  CalendarInviteView,
  CalendarOccurrence,
  CalendarRangeQuery,
  CoreEvent,
  CreateDraftInput,
  DraftId,
  DraftView,
  IcsOverlayId,
  IcsOverlayView,
  JobId,
  JobView,
  LabelId,
  LabelView,
  MailboxCounts,
  MailAction,
  MailRuleView,
  NewAccountInput,
  OAuthSignInInput,
  OverlayBusyBlock,
  RemoteContent,
  RsvpInviteInput,
  RuleId,
  SaveAgentAccountPermissionInput,
  SaveAgentBridgeConfigInput,
  SaveAiConfigInput,
  SaveCalendarEventInput,
  SaveIcsOverlayInput,
  SaveLabelInput,
  SaveMailRuleInput,
  SaveSavedSearchInput,
  SaveSignatureInput,
  SaveTemplateInput,
  SavedOAuthClients,
  SavedSearchId,
  SavedSearchView,
  Scope,
  ScreenerDecision,
  ScreenerSenderView,
  SendMode,
  SendReceipt,
  SignatureId,
  SignatureView,
  TemplateId,
  TemplateView,
  ThreadId,
  ThreadPage,
  ThreadQuery,
  ThreadView,
  UndoOutcome,
  UpdateAccountInput,
  VipSenderView,
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

export async function signalAppReady(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke<void>("app_ready");
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

export async function downloadAttachment(attachmentId: AttachmentId): Promise<AttachmentData> {
  if (!isTauriRuntime()) return mock.downloadAttachment(attachmentId);
  return invoke<AttachmentData>("download_attachment", { attachmentId });
}

export async function saveAttachment(
  attachmentId: AttachmentId,
  destPath: string,
): Promise<string> {
  if (!isTauriRuntime()) return destPath;
  return invoke<string>("save_attachment", { attachmentId, destPath });
}

export async function listSignatures(accountId: AccountId | null): Promise<SignatureView[]> {
  if (!isTauriRuntime()) return mock.listSignatures(accountId);
  return invoke<SignatureView[]>("list_signatures", { accountId });
}

export async function saveSignature(input: SaveSignatureInput): Promise<SignatureView> {
  if (!isTauriRuntime()) return mock.saveSignature(input);
  return invoke<SignatureView>("save_signature", { input });
}

export async function deleteSignature(signatureId: SignatureId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteSignature(signatureId);
  return invoke<void>("delete_signature", { signatureId });
}

// ── Phase 3: Saved Searches, Screener, VIP, Labels & Rules ──────────────────

export async function listSavedSearches(): Promise<SavedSearchView[]> {
  if (!isTauriRuntime()) return mock.listSavedSearches();
  return invoke<SavedSearchView[]>("list_saved_searches");
}

export async function saveSavedSearch(input: SaveSavedSearchInput): Promise<SavedSearchView> {
  if (!isTauriRuntime()) return mock.saveSavedSearch(input);
  return invoke<SavedSearchView>("save_saved_search", { input });
}

export async function deleteSavedSearch(savedSearchId: SavedSearchId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteSavedSearch(savedSearchId);
  return invoke<void>("delete_saved_search", { savedSearchId });
}

export async function listScreenerSenders(): Promise<ScreenerSenderView[]> {
  if (!isTauriRuntime()) return mock.listScreenerSenders();
  return invoke<ScreenerSenderView[]>("list_screener_senders");
}

export async function screenSender(
  address: string,
  displayName: string | null,
  decision: ScreenerDecision,
): Promise<number> {
  if (!isTauriRuntime()) return mock.screenSender(address, displayName, decision);
  return invoke<number>("screen_sender", { address, displayName, decision });
}

export async function listVipSenders(): Promise<VipSenderView[]> {
  if (!isTauriRuntime()) return mock.listVipSenders();
  return invoke<VipSenderView[]>("list_vip_senders");
}

export async function addVipSender(
  address: string,
  displayName: string | null,
): Promise<VipSenderView> {
  if (!isTauriRuntime()) return mock.addVipSender(address, displayName);
  return invoke<VipSenderView>("add_vip_sender", { address, displayName });
}

export async function removeVipSender(address: string): Promise<void> {
  if (!isTauriRuntime()) return mock.removeVipSender(address);
  return invoke<void>("remove_vip_sender", { address });
}

export async function listLabels(): Promise<LabelView[]> {
  if (!isTauriRuntime()) return mock.listLabels();
  return invoke<LabelView[]>("list_labels");
}

export async function saveLabel(input: SaveLabelInput): Promise<LabelView> {
  if (!isTauriRuntime()) return mock.saveLabel(input);
  return invoke<LabelView>("save_label", { input });
}

export async function deleteLabel(labelId: LabelId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteLabel(labelId);
  return invoke<void>("delete_label", { labelId });
}

export async function labelThreads(
  labelId: LabelId,
  threadIds: ThreadId[],
  add: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return mock.labelThreads(labelId, threadIds, add);
  return invoke<void>("label_threads", { labelId, threadIds, add });
}

export async function listRules(): Promise<MailRuleView[]> {
  if (!isTauriRuntime()) return mock.listRules();
  return invoke<MailRuleView[]>("list_rules");
}

export async function saveRule(input: SaveMailRuleInput): Promise<MailRuleView> {
  if (!isTauriRuntime()) return mock.saveRule(input);
  return invoke<MailRuleView>("save_rule", { input });
}

export async function deleteRule(ruleId: RuleId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteRule(ruleId);
  return invoke<void>("delete_rule", { ruleId });
}

export async function runRuleNow(ruleId: RuleId): Promise<number> {
  if (!isTauriRuntime()) return mock.runRuleNow(ruleId);
  return invoke<number>("run_rule_now", { ruleId });
}

export async function tickScheduler(): Promise<void> {
  if (!isTauriRuntime()) return mock.tickScheduler();
  return invoke<void>("tick_scheduler");
}

export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauriRuntime()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  return invoke<void>("open_external_url", { url });
}

// ── Personal Calendar ───────────────────────────────────────────────────────

export async function listCalendarEvents(
  query: CalendarRangeQuery,
): Promise<CalendarOccurrence[]> {
  if (!isTauriRuntime()) return mock.listCalendarEvents(query);
  return invoke<CalendarOccurrence[]>("list_calendar_events", { query });
}

export async function saveCalendarEvent(
  input: SaveCalendarEventInput,
): Promise<CalendarEventView> {
  if (!isTauriRuntime()) return mock.saveCalendarEvent(input);
  return invoke<CalendarEventView>("save_calendar_event", { input });
}

export async function quickAddCalendarEvent(text: string): Promise<CalendarEventView> {
  if (!isTauriRuntime()) return mock.quickAddCalendarEvent(text);
  return invoke<CalendarEventView>("quick_add_calendar_event", { text });
}

export async function deleteCalendarEvent(eventId: CalendarEventId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteCalendarEvent(eventId);
  return invoke<void>("delete_calendar_event", { eventId });
}

export async function rsvpCalendarInvite(input: RsvpInviteInput): Promise<CalendarInviteView> {
  if (!isTauriRuntime()) return mock.rsvpCalendarInvite(input);
  return invoke<CalendarInviteView>("rsvp_calendar_invite", { input });
}

export async function addInviteToCalendar(
  input: AddInviteToCalendarInput,
): Promise<CalendarEventView> {
  if (!isTauriRuntime()) return mock.addInviteToCalendar(input);
  return invoke<CalendarEventView>("add_invite_to_calendar", { input });
}

export async function listIcsOverlays(): Promise<IcsOverlayView[]> {
  if (!isTauriRuntime()) return mock.listIcsOverlays();
  return invoke<IcsOverlayView[]>("list_ics_overlays");
}

export async function saveIcsOverlay(input: SaveIcsOverlayInput): Promise<IcsOverlayView> {
  if (!isTauriRuntime()) return mock.saveIcsOverlay(input);
  return invoke<IcsOverlayView>("save_ics_overlay", { input });
}

export async function deleteIcsOverlay(overlayId: IcsOverlayId): Promise<void> {
  if (!isTauriRuntime()) return mock.deleteIcsOverlay(overlayId);
  return invoke<void>("delete_ics_overlay", { overlayId });
}

export async function refreshIcsOverlay(overlayId: IcsOverlayId): Promise<IcsOverlayView> {
  if (!isTauriRuntime()) return mock.refreshIcsOverlay(overlayId);
  return invoke<IcsOverlayView>("refresh_ics_overlay", { overlayId });
}

export async function listOverlayBusyBlocks(
  query: CalendarRangeQuery,
): Promise<OverlayBusyBlock[]> {
  if (!isTauriRuntime()) return mock.listOverlayBusyBlocks(query);
  return invoke<OverlayBusyBlock[]>("list_overlay_busy_blocks", { query });
}

export async function findAvailabilitySlots(
  query: AvailabilityQuery,
): Promise<AvailabilitySlot[]> {
  if (!isTauriRuntime()) return mock.findAvailabilitySlots(query);
  return invoke<AvailabilitySlot[]>("find_availability_slots", { query });
}

// ── Phase 4: Hermes MCP Bridge, Agent Activity, Pending Drafts & In-Pane AI ──

export async function listAgentPermissions(): Promise<AgentAccountPermissionView[]> {
  if (!isTauriRuntime()) return mock.listAgentPermissions();
  return invoke<AgentAccountPermissionView[]>("list_agent_permissions");
}

export async function saveAgentPermission(
  input: SaveAgentAccountPermissionInput,
): Promise<AgentAccountPermissionView> {
  if (!isTauriRuntime()) return mock.saveAgentPermission(input);
  return invoke<AgentAccountPermissionView>("save_agent_permission", { input });
}

export async function getAgentBridgeConfig(): Promise<AgentBridgeConfig> {
  if (!isTauriRuntime()) return mock.getAgentBridgeConfig();
  return invoke<AgentBridgeConfig>("get_agent_bridge_config");
}

export async function saveAgentBridgeConfig(
  input: SaveAgentBridgeConfigInput,
): Promise<AgentBridgeConfig> {
  if (!isTauriRuntime()) return mock.saveAgentBridgeConfig(input);
  return invoke<AgentBridgeConfig>("save_agent_bridge_config", { input });
}

export async function listAgentActivity(limit = 50): Promise<AgentActivityView[]> {
  if (!isTauriRuntime()) return mock.listAgentActivity(limit);
  return invoke<AgentActivityView[]>("list_agent_activity", { limit });
}

export async function undoAgentActivity(
  activityId: AgentActivityId,
): Promise<AgentActivityView> {
  if (!isTauriRuntime()) return mock.undoAgentActivity(activityId);
  return invoke<AgentActivityView>("undo_agent_activity", { activityId });
}

export async function listPendingAgentDrafts(): Promise<DraftView[]> {
  if (!isTauriRuntime()) return mock.listPendingAgentDrafts();
  return invoke<DraftView[]>("list_pending_agent_drafts");
}

export async function approveAgentDraft(
  draftId: DraftId,
  mode: SendMode,
): Promise<SendReceipt> {
  if (!isTauriRuntime()) return mock.approveAgentDraft(draftId, mode);
  return invoke<SendReceipt>("approve_agent_draft", { draftId, mode });
}

export async function rejectAgentDraft(draftId: DraftId, reason: string): Promise<void> {
  if (!isTauriRuntime()) return mock.rejectAgentDraft(draftId, reason);
  return invoke<void>("reject_agent_draft", { draftId, reason });
}

export async function getAiConfig(): Promise<AiConfigView> {
  if (!isTauriRuntime()) return mock.getAiConfig();
  return invoke<AiConfigView>("get_ai_config");
}

export async function saveAiConfig(input: SaveAiConfigInput): Promise<AiConfigView> {
  if (!isTauriRuntime()) return mock.saveAiConfig(input);
  return invoke<AiConfigView>("save_ai_config", { input });
}

export async function aiSummarizeThread(threadId: ThreadId): Promise<AiSummaryResult> {
  if (!isTauriRuntime()) return mock.aiSummarizeThread(threadId);
  return invoke<AiSummaryResult>("ai_summarize_thread", { threadId });
}

export async function aiAskThread(
  threadId: ThreadId,
  question: string,
  selectedText: string | null,
): Promise<AiAnswerResult> {
  if (!isTauriRuntime()) return mock.aiAskThread(threadId, question, selectedText);
  return invoke<AiAnswerResult>("ai_ask_thread", { threadId, question, selectedText });
}

export async function aiDraftReply(
  threadId: ThreadId,
  instructions: string,
  stageForApproval: boolean,
): Promise<AiDraftResult> {
  if (!isTauriRuntime()) return mock.aiDraftReply(threadId, instructions, stageForApproval);
  return invoke<AiDraftResult>("ai_draft_reply", {
    threadId,
    instructions,
    stageForApproval,
  });
}

export async function onMailEvent(handler: (event: CoreEvent) => void): Promise<() => void> {
  if (!isTauriRuntime()) return () => undefined;
  const unlisten = await listen<CoreEvent>("mail://event", (event) => {
    handler(event.payload);
  });
  return unlisten;
}
