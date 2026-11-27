import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  actOnThreads,
  addAccount as addAccountCommand,
  addInviteToCalendar as addInviteToCalendarCommand,
  addVipSender as addVipSenderCommand,
  aiAskThread as aiAskThreadCommand,
  aiDraftReply as aiDraftReplyCommand,
  aiSummarizeThread as aiSummarizeThreadCommand,
  approveAgentDraft as approveAgentDraftCommand,
  cancelSend,
  createDraft,
  deleteCalendarEvent as deleteCalendarEventCommand,
  deleteIcsOverlay as deleteIcsOverlayCommand,
  deleteLabel as deleteLabelCommand,
  deleteRule as deleteRuleCommand,
  deleteSavedSearch as deleteSavedSearchCommand,
  deleteSignature as deleteSignatureCommand,
  deleteTemplate,
  dismissFollowUp,
  downloadAttachment as downloadAttachmentCommand,
  getAgentBridgeConfig,
  getAiConfig,
  getSettings,
  isTauriRuntime,
  labelThreads as labelThreadsCommand,
  listAccounts,
  listAgentActivity,
  listAgentPermissions,
  listCalendarEvents,
  listIcsOverlays,
  listJobs,
  listLabels,
  listOverlayBusyBlocks,
  listPendingAgentDrafts,
  listRules,
  listSavedSearches,
  listScreenerSenders,
  listSignatures,
  listTemplates,
  listThreads,
  listVipSenders,
  mailboxCounts,
  oauthSignInAccount as oauthSignInAccountCommand,
  onMailEvent,
  openThread as openThreadCommand,
  quickAddCalendarEvent as quickAddCalendarEventCommand,
  reauthOAuthAccount as reauthOAuthAccountCommand,
  refreshIcsOverlay as refreshIcsOverlayCommand,
  rejectAgentDraft as rejectAgentDraftCommand,
  removeAccount as removeAccountCommand,
  removeVipSender as removeVipSenderCommand,
  rsvpCalendarInvite as rsvpCalendarInviteCommand,
  runRuleNow as runRuleNowCommand,
  saveAgentBridgeConfig as saveAgentBridgeConfigCommand,
  saveAgentPermission as saveAgentPermissionCommand,
  saveAiConfig as saveAiConfigCommand,
  saveCalendarEvent as saveCalendarEventCommand,
  saveIcsOverlay as saveIcsOverlayCommand,
  saveLabel as saveLabelCommand,
  saveRule as saveRuleCommand,
  saveSavedSearch as saveSavedSearchCommand,
  saveSignature as saveSignatureCommand,
  saveTemplate as saveTemplateCommand,
  savedOAuthClients as savedOAuthClientsCommand,
  screenSender as screenSenderCommand,
  sendDraft,
  setFollowUp,
  signalAppReady,
  snoozeThread as snoozeThreadCommand,
  syncAccount,
  tickScheduler,
  undoAgentActivity as undoAgentActivityCommand,
  unsnoozeThread,
  updateAccount as updateAccountCommand,
  updateSettings as updateSettingsCommand,
} from "../../ipc/core";
import type {
  AccountId,
  AccountView,
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
  CalendarEventId,
  CalendarEventView,
  CalendarOccurrence,
  CreateDraftInput,
  DraftId,
  DraftView,
  IcsOverlayId,
  IcsOverlayView,
  InboxCategory,
  JobId,
  JobView,
  LabelId,
  LabelView,
  MailboxCounts,
  MailboxView,
  MailRuleView,
  MessageId,
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
  SignatureId,
  SignatureView,
  TemplateId,
  TemplateView,
  ThreadId,
  ThreadSummary,
  ThreadView,
  UnixMs,
  UpdateAccountInput,
  VipSenderView,
} from "../../types/mail";
import { formatDue } from "../ui/format";

export type Theme = "dark" | "light";
export type Layout = "split" | "classic";
export type TextSize = "small" | "medium" | "large";
export type Stage = "list" | "thread";
export type PopoverMode = "snooze" | "followUp" | "sendLater";
export type ActiveSection = "mail" | "calendar";

export interface Anchor {
  x: number;
  y: number;
}

export interface ToastData {
  id: number;
  message: string;
  durationMs: number;
  onUndo?: () => void | Promise<void>;
}

export interface PopoverState {
  mode: PopoverMode;
  anchor: Anchor;
  onPick: (until: UnixMs) => void | Promise<void>;
}

interface ToastOptions {
  durationMs?: number;
  onUndo?: () => void | Promise<void>;
}

const THEME_KEY = "ferrule.theme";
const LAYOUT_KEY = "ferrule.layout";
const CHROME_SIZE_KEY = "ferrule.chromeTextSize";
const READER_SIZE_KEY = "ferrule.readerTextSize";
const DEFAULT_UNDO_MS = 5000;

function readStringField(obj: object, key: string): string | null {
  if (!(key in obj)) return null;
  const descriptor = Object.getOwnPropertyDescriptor(obj, key);
  return typeof descriptor?.value === "string" ? descriptor.value : null;
}

function friendlyError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error !== null && typeof error === "object") {
    const kind = readStringField(error, "kind");
    const reason = readStringField(error, "reason");
    const field = readStringField(error, "field");
    if (kind === "authRequired") {
      return reason ?? "Sign-in rejected or credentials missing — check your email and App Password.";
    }
    if (kind === "keychainUnavailable") {
      return `OS keychain unavailable: ${reason ?? "unlock your keyring"}`;
    }
    if (kind === "network") {
      return `Connection failed: ${reason ?? "check server host, port, and internet"}`;
    }
    if (kind === "invalidInput") {
      return field && reason ? `${field}: ${reason}` : reason ?? "Invalid input";
    }
    if (reason) return reason;
    if (kind) return kind;
  }
  return String(error);
}

function loadTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(THEME_KEY);
    return stored === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

function loadLayout(): Layout {
  try {
    const stored = window.localStorage.getItem(LAYOUT_KEY);
    return stored === "classic" ? "classic" : "split";
  } catch {
    return "split";
  }
}

function loadTextSize(key: string): TextSize {
  try {
    const stored = window.localStorage.getItem(key);
    if (stored === "medium" || stored === "large") return stored;
    return "small";
  } catch {
    return "small";
  }
}

function centerAnchor(): Anchor {
  return {
    x: Math.max(12, window.innerWidth / 2 - 118),
    y: Math.max(12, window.innerHeight / 2 - 110),
  };
}

export interface MailApp {
  booting: boolean;
  error: string | null;
  accounts: AccountView[];
  savedOAuthClients: SavedOAuthClients;
  settings: AppSettings;
  counts: MailboxCounts | null;
  threads: ThreadSummary[];
  jobs: JobView[];
  templates: TemplateView[];
  signatures: SignatureView[];
  savedSearches: SavedSearchView[];
  screenerSenders: ScreenerSenderView[];
  vipSenders: VipSenderView[];
  labels: LabelView[];
  rules: MailRuleView[];
  screenerOpen: boolean;
  rulesOpen: boolean;
  scopeAccount: AccountId | null;
  view: MailboxView;
  selectedId: ThreadId | null;
  thread: ThreadView | null;
  theme: Theme;
  layout: Layout;
  chromeTextSize: TextSize;
  readerTextSize: TextSize;
  stage: Stage;
  toast: ToastData | null;
  commandOpen: boolean;
  composeOpen: boolean;
  addAccountOpen: boolean;
  templatesOpen: boolean;
  settingsOpen: boolean;
  onboardingOpen: boolean;
  tourOpen: boolean;
  tourStep: number;
  popover: PopoverState | null;
  replyText: string;
  replyNonce: number;
  expanded: ReadonlySet<MessageId>;
  remoteContent: RemoteContent;
  syncing: boolean;
  composeInsert: { nonce: number; body: string } | null;
  activeSection: ActiveSection;
  calendarOccurrences: CalendarOccurrence[];
  icsOverlays: IcsOverlayView[];
  overlayBusyBlocks: OverlayBusyBlock[];
  selectedCalendarEvent: CalendarEventView | null;
  agentPermissions: AgentAccountPermissionView[];
  agentBridgeConfig: AgentBridgeConfig;
  agentActivity: AgentActivityView[];
  pendingAgentDrafts: DraftView[];
  aiConfig: AiConfigView;
  agentModalOpen: boolean;

  toggleTheme(): void;
  setTheme(next: Theme): void;
  toggleLayout(): void;
  setLayout(next: Layout): void;
  setChromeTextSize(next: TextSize): void;
  setReaderTextSize(next: TextSize): void;
  cycleReaderTextSize(): void;
  openOnboarding(): void;
  completeOnboarding(): Promise<void>;
  startTour(): void;
  nextTourStep(): void;
  prevTourStep(): void;
  skipTour(): Promise<void>;
  completeTour(): Promise<void>;
  backToList(): void;
  selectSpace(id: AccountId | null): void;
  selectView(view: MailboxView): void;
  selectCategory(category: InboxCategory): void;
  openThread(id: ThreadId): void;
  archiveThread(id: ThreadId): void;
  trashThread(id: ThreadId): void;
  reportSpam(id: ThreadId): void;
  starThread(id: ThreadId): void;
  snooze(id: ThreadId, until: UnixMs): Promise<void>;
  followUp(id: ThreadId, dueAt: UnixMs): Promise<void>;
  toggleRemote(): void;
  syncNow(): Promise<void>;
  openCompose(): void;
  closeCompose(): void;
  openAddAccount(): void;
  closeAddAccount(): void;
  openTemplates(): void;
  closeTemplates(): void;
  openSettings(): void;
  closeSettings(): void;
  openScreener(): void;
  closeScreener(): void;
  openRules(): void;
  closeRules(): void;
  persistSavedSearch(input: SaveSavedSearchInput): Promise<void>;
  removeSavedSearch(id: SavedSearchId): Promise<void>;
  screenSender(address: string, displayName: string | null, decision: ScreenerDecision): Promise<void>;
  addVipSender(address: string, displayName: string | null): Promise<void>;
  removeVipSender(address: string): Promise<void>;
  persistLabel(input: SaveLabelInput): Promise<void>;
  removeLabel(id: LabelId): Promise<void>;
  labelThreads(labelId: LabelId, threadIds: ThreadId[], add: boolean): Promise<void>;
  persistRule(input: SaveMailRuleInput): Promise<void>;
  removeRule(id: RuleId): Promise<void>;
  runRuleNow(id: RuleId): Promise<void>;
  setDeveloperToolsEnabled(enabled: boolean): Promise<void>;
  openCommand(): void;
  closeCommand(): void;
  closeTopLayer(): void;
  openPopover(mode: PopoverMode, anchor: Anchor, onPick: (until: UnixMs) => void | Promise<void>): void;
  closePopover(): void;
  pickPopoverTime(unixMs: UnixMs): Promise<void>;
  requestSnooze(id: ThreadId, anchor: Anchor): void;
  requestFollowUp(id: ThreadId, anchor: Anchor): void;
  requestSendLater(anchor: Anchor, pick: (until: UnixMs) => void | Promise<void>): void;
  setReplyText(text: string): void;
  focusReply(): void;
  toggleExpand(id: MessageId): void;
  submitDraft(input: CreateDraftInput, mode: SendMode): Promise<void>;
  sendReply(mode: SendMode): Promise<void>;
  showToast(message: string, options?: ToastOptions): void;
  dismissToast(): void;
  insertTemplate(template: TemplateView): void;
  clearComposeInsert(): void;
  persistTemplate(input: SaveTemplateInput): Promise<void>;
  removeTemplate(id: TemplateId): Promise<void>;
  persistSignature(input: SaveSignatureInput): Promise<SignatureView>;
  removeSignature(id: SignatureId): Promise<void>;
  downloadAttachment(id: AttachmentId): Promise<AttachmentData>;
  addAccount(input: NewAccountInput): Promise<void>;
  oauthSignInAccount(input: OAuthSignInInput): Promise<AccountView>;
  updateAccount(input: UpdateAccountInput): Promise<AccountView>;
  reauthAccount(id: AccountId): Promise<void>;
  removeAccount(id: AccountId): Promise<void>;
  insertTextSnippet(bodyText: string): void;
  openCalendarSection(): void;
  loadCalendarRange(rangeStart: UnixMs, rangeEnd: UnixMs): Promise<void>;
  selectCalendarEvent(event: CalendarEventView | null): Promise<void>;
  persistCalendarEvent(input: SaveCalendarEventInput): Promise<CalendarEventView>;
  quickAddEvent(text: string): Promise<CalendarEventView>;
  removeCalendarEvent(id: CalendarEventId): Promise<void>;
  rsvpInvite(input: RsvpInviteInput): Promise<void>;
  addInviteToCalendar(input: AddInviteToCalendarInput): Promise<void>;
  persistIcsOverlay(input: SaveIcsOverlayInput): Promise<IcsOverlayView>;
  removeIcsOverlay(id: IcsOverlayId): Promise<void>;
  syncIcsOverlay(id: IcsOverlayId): Promise<void>;
  openAgentModal(): void;
  closeAgentModal(): void;
  saveAgentPermission(input: SaveAgentAccountPermissionInput): Promise<void>;
  saveAgentBridgeConfig(input: SaveAgentBridgeConfigInput): Promise<void>;
  undoAgentActivity(activityId: AgentActivityId): Promise<void>;
  approveAgentDraft(draftId: DraftId, mode: SendMode): Promise<void>;
  rejectAgentDraft(draftId: DraftId, reason: string): Promise<void>;
  saveAiConfig(input: SaveAiConfigInput): Promise<void>;
  aiSummarizeThread(threadId: ThreadId): Promise<AiSummaryResult>;
  aiAskThread(threadId: ThreadId, question: string, selectedText: string | null): Promise<AiAnswerResult>;
  aiDraftReply(threadId: ThreadId, instructions: string, stageForApproval: boolean): Promise<AiDraftResult>;
}

export function useMailApp(): MailApp {
  const [booting, setBooting] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<AccountView[]>([]);
  const [savedOAuthClients, setSavedOAuthClients] = useState<SavedOAuthClients>({
    gmailClientId: null,
    outlookClientId: null,
    outlookTenant: null,
    gmailBuiltIn: false,
    outlookBuiltIn: false,
  });
  const [settings, setSettings] = useState<AppSettings>({
    developerToolsEnabled: false,
    onboardingCompleted: false,
    tourCompleted: false,
  });
  const [counts, setCounts] = useState<MailboxCounts | null>(null);
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [jobs, setJobs] = useState<JobView[]>([]);
  const [templates, setTemplates] = useState<TemplateView[]>([]);
  const [signatures, setSignatures] = useState<SignatureView[]>([]);
  const [savedSearches, setSavedSearches] = useState<SavedSearchView[]>([]);
  const [screenerSenders, setScreenerSenders] = useState<ScreenerSenderView[]>([]);
  const [vipSenders, setVipSenders] = useState<VipSenderView[]>([]);
  const [labels, setLabels] = useState<LabelView[]>([]);
  const [rules, setRules] = useState<MailRuleView[]>([]);
  const [screenerOpen, setScreenerOpen] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const [scopeAccount, setScopeAccount] = useState<AccountId | null>(null);
  const [view, setView] = useState<MailboxView>({ kind: "inbox", category: "primary" });
  const [selectedId, setSelectedId] = useState<ThreadId | null>(null);
  const [thread, setThread] = useState<ThreadView | null>(null);
  const [remoteContent, setRemoteContent] = useState<RemoteContent>("blockAll");
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [layout, setLayout] = useState<Layout>(loadLayout);
  const [chromeTextSize, setChromeTextSize] = useState<TextSize>(() =>
    loadTextSize(CHROME_SIZE_KEY),
  );
  const [readerTextSize, setReaderTextSize] = useState<TextSize>(() =>
    loadTextSize(READER_SIZE_KEY),
  );
  const [stage, setStage] = useState<Stage>("list");
  const [toast, setToast] = useState<ToastData | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);
  const [composeOpen, setComposeOpen] = useState(false);
  const [addAccountOpen, setAddAccountOpen] = useState(false);
  const [templatesOpen, setTemplatesOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [onboardingOpen, setOnboardingOpen] = useState(false);
  const [tourOpen, setTourOpen] = useState(false);
  const [tourStep, setTourStep] = useState(0);
  const [popover, setPopover] = useState<PopoverState | null>(null);
  const [replyText, setReplyText] = useState("");
  const [replyNonce, setReplyNonce] = useState(0);
  const [expanded, setExpanded] = useState<ReadonlySet<MessageId>>(() => new Set());
  const [syncing, setSyncing] = useState(false);
  const [composeInsert, setComposeInsert] = useState<{ nonce: number; body: string } | null>(null);
  const [activeSection, setActiveSection] = useState<ActiveSection>("mail");
  const [calendarOccurrences, setCalendarOccurrences] = useState<CalendarOccurrence[]>([]);
  const [icsOverlays, setIcsOverlays] = useState<IcsOverlayView[]>([]);
  const [overlayBusyBlocks, setOverlayBusyBlocks] = useState<OverlayBusyBlock[]>([]);
  const [selectedCalendarEvent, setSelectedCalendarEvent] = useState<CalendarEventView | null>(
    null,
  );
  const [agentPermissions, setAgentPermissions] = useState<AgentAccountPermissionView[]>([]);
  const [agentBridgeConfig, setAgentBridgeConfig] = useState<AgentBridgeConfig>({
    mcpEnabled: false,
    ssePort: 48173,
    webhookEnabled: false,
    webhookUrl: "",
    hasWebhookSecret: false,
    ipcEndpoint: "",
    sseToken: "",
  });
  const [agentActivity, setAgentActivity] = useState<AgentActivityView[]>([]);
  const [pendingAgentDrafts, setPendingAgentDrafts] = useState<DraftView[]>([]);
  const [aiConfig, setAiConfig] = useState<AiConfigView>({
    enabled: false,
    baseUrl: "http://127.0.0.1:11434/v1",
    model: "llama3.2",
    hasApiKey: false,
  });
  const [agentModalOpen, setAgentModalOpen] = useState(false);

  const toastSeq = useRef(0);
  const insertSeq = useRef(0);
  const jobsSignature = useRef<string>("");
  const themeRef = useRef<Theme>(theme);
  const replyTextRef = useRef(replyText);
  const threadRef = useRef<ThreadView | null>(thread);
  const keyHandlerRef = useRef<(event: KeyboardEvent) => void>(() => undefined);
  const accountsRef = useRef<AccountView[]>(accounts);
  const syncInFlightRef = useRef(false);
  const lastAutoSyncAtRef = useRef(0);

  const scope: Scope = useMemo(
    () => (scopeAccount ? { kind: "account", accountId: scopeAccount } : { kind: "unified" }),
    [scopeAccount],
  );

  const showToast = useCallback((message: string, options?: ToastOptions) => {
    toastSeq.current += 1;
    setToast({
      id: toastSeq.current,
      message,
      durationMs: options?.durationMs ?? DEFAULT_UNDO_MS,
      onUndo: options?.onUndo,
    });
  }, []);

  const dismissToast = useCallback(() => setToast(null), []);

  const reload = useCallback(async (targetScope: Scope, targetView: MailboxView) => {
    const [nextCounts, page] = await Promise.all([
      mailboxCounts(targetScope),
      listThreads({ scope: targetScope, view: targetView, limit: 100, offset: 0 }),
    ]);
    setCounts(nextCounts);
    setThreads(page.items);
  }, []);

  const reloadCalendarMeta = useCallback(async () => {
    const nowMs = Date.now();
    const [overlays, upcoming] = await Promise.all([
      listIcsOverlays(),
      listCalendarEvents({
        rangeStart: nowMs - 7 * 86_400_000,
        rangeEnd: nowMs + 30 * 86_400_000,
      }),
    ]);
    setIcsOverlays(overlays);
    setCalendarOccurrences(upcoming);
  }, []);

  const reloadMeta = useCallback(async () => {
    const [
      nextAccounts,
      nextJobs,
      nextTemplates,
      nextSignatures,
      nextSavedSearches,
      nextScreener,
      nextVips,
      nextLabels,
      nextRules,
      nextClients,
      nextSettings,
      nextAgentPerms,
      nextBridgeCfg,
      nextAgentActivity,
      nextPendingDrafts,
      nextAiCfg,
    ] = await Promise.all([
      listAccounts(),
      listJobs(),
      listTemplates(),
      listSignatures(null),
      listSavedSearches(),
      listScreenerSenders(),
      listVipSenders(),
      listLabels(),
      listRules(),
      savedOAuthClientsCommand(),
      getSettings(),
      listAgentPermissions(),
      getAgentBridgeConfig(),
      listAgentActivity(50),
      listPendingAgentDrafts(),
      getAiConfig(),
    ]);
    setAccounts(nextAccounts);
    setJobs(nextJobs);
    setTemplates(nextTemplates);
    setSignatures(nextSignatures);
    setSavedSearches(nextSavedSearches);
    setScreenerSenders(nextScreener);
    setVipSenders(nextVips);
    setLabels(nextLabels);
    setRules(nextRules);
    setSavedOAuthClients(nextClients);
    setSettings(nextSettings);
    setAgentPermissions(nextAgentPerms);
    setAgentBridgeConfig(nextBridgeCfg);
    setAgentActivity(nextAgentActivity);
    setPendingAgentDrafts(nextPendingDrafts);
    setAiConfig(nextAiCfg);
    jobsSignature.current = nextJobs.map((j) => `${j.id}:${j.state}`).join("|");
    await reloadCalendarMeta();
    return { accounts: nextAccounts, settings: nextSettings };
  }, [reloadCalendarMeta]);

  // Theme & layout persistence + accent wiring.
  useEffect(() => {
    themeRef.current = theme;
    try {
      window.localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Preview environments may deny storage; theme still applies for the session.
    }
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    try {
      window.localStorage.setItem(LAYOUT_KEY, layout);
    } catch {
      // Ignore storage failures in restricted previews.
    }
  }, [layout]);

  useEffect(() => {
    try {
      window.localStorage.setItem(CHROME_SIZE_KEY, chromeTextSize);
    } catch {
      // Ignore storage failures in restricted previews.
    }
    document.documentElement.dataset.chromeSize = chromeTextSize;
  }, [chromeTextSize]);

  useEffect(() => {
    try {
      window.localStorage.setItem(READER_SIZE_KEY, readerTextSize);
    } catch {
      // Ignore storage failures in restricted previews.
    }
    document.documentElement.dataset.readerSize = readerTextSize;
  }, [readerTextSize]);

  // Keep refs fresh so popover and background event callbacks read current state.
  useEffect(() => {
    replyTextRef.current = replyText;
    threadRef.current = thread;
    accountsRef.current = accounts;
  });

  useEffect(() => {
    const active = scopeAccount ? accounts.find((a) => a.id === scopeAccount) : undefined;
    const accent = active ? `var(--space-${active.color})` : "var(--space-default)";
    document.documentElement.style.setProperty("--space-accent", accent);
  }, [scopeAccount, accounts]);

  // Boot: load meta, determine first-run Onboarding state, and signal desktop
  // readiness so the first-boot Launch Splash transitions smoothly to main.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const meta = await reloadMeta();
        if (!cancelled && !meta.settings.onboardingCompleted && meta.accounts.length === 0) {
          setOnboardingOpen(true);
        }
      } catch (err) {
        if (!cancelled) setError(friendlyError(err));
      } finally {
        if (!cancelled) {
          setBooting(false);
          void signalAppReady();
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [reloadMeta]);

  // Scope or view changed: drop the selection and reload the list.
  useEffect(() => {
    setSelectedId(null);
    setThread(null);
    void reload(scope, view).catch((err: unknown) => showToast(friendlyError(err)));
  }, [scope, view, reload, showToast]);

  // Fetch the open thread. Re-runs on theme flips because the preview
  // sanitizer bakes ink colors into srcdoc.
  useEffect(() => {
    if (selectedId === null) {
      setThread(null);
      return;
    }
    let cancelled = false;
    const requestTheme = theme;
    void openThreadCommand(selectedId, remoteContent)
      .then((next) => {
        if (cancelled || requestTheme !== themeRef.current) return;
        setThread(next);
        void reload(scope, view).catch(() => undefined);
        void reloadMeta().catch(() => undefined);
      })
      .catch((err: unknown) => {
        if (!cancelled) showToast(friendlyError(err));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, remoteContent, theme, reload, reloadMeta, scope, view, showToast]);

  // Scheduler heartbeat: wake snoozes, fire due sends, refresh on change.
  useEffect(() => {
    const timer = window.setInterval(() => {
      void (async () => {
        try {
          await tickScheduler();
          const nextJobs = await listJobs();
          const signature = nextJobs.map((j) => `${j.id}:${j.state}`).join("|");
          if (signature !== jobsSignature.current) {
            jobsSignature.current = signature;
            setJobs(nextJobs);
            await reload(scope, view);
          }
        } catch {
          // Transient scheduler errors are retried on the next tick.
        }
      })();
    }, 5000);
    return () => window.clearInterval(timer);
  }, [scope, view, reload]);

  // Live backend domain events from Tauri (mail://event): keeps thread list,
  // sync indicator, shared threads/drafts, calendar, and action items fresh as
  // automatic background sync and Staff Sync SSE run.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    let mailRefreshTimer: number | null = null;
    let calendarRefreshTimer: number | null = null;

    const scheduleMailRefresh = (): void => {
      if (mailRefreshTimer !== null) return;
      mailRefreshTimer = window.setTimeout(() => {
        mailRefreshTimer = null;
        if (disposed) return;
        void Promise.all([
          reload(scope, view),
          listAccounts().then((next) => {
            if (!disposed) setAccounts(next);
          }),
        ]).catch(() => undefined);
      }, 250);
    };

    const scheduleCalendarRefresh = (): void => {
      if (calendarRefreshTimer !== null) return;
      calendarRefreshTimer = window.setTimeout(() => {
        calendarRefreshTimer = null;
        if (disposed) return;
        void reloadCalendarMeta().catch(() => undefined);
      }, 250);
    };

    void onMailEvent((event) => {
      if (disposed) return;
      if (event.kind === "syncChanged") {
        setAccounts((prev) =>
          prev.map((acct) =>
            acct.id === event.accountId ? { ...acct, sync: event.state } : acct,
          ),
        );
        if (event.state.kind === "idle") {
          lastAutoSyncAtRef.current = Date.now();
          scheduleMailRefresh();
        }
      } else if (
        event.kind === "mailChanged" ||
        event.kind === "resync" ||
        event.kind === "agentActivityChanged" ||
        event.kind === "draftChanged"
      ) {
        scheduleMailRefresh();
        void Promise.all([listAgentActivity(50), listPendingAgentDrafts()])
          .then(([act, drafts]) => {
            if (!disposed) {
              setAgentActivity(act);
              setPendingAgentDrafts(drafts);
            }
          })
          .catch(() => undefined);
      } else if (event.kind === "calendarChanged") {
        scheduleCalendarRefresh();
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    return () => {
      disposed = true;
      if (mailRefreshTimer !== null) window.clearTimeout(mailRefreshTimer);
      if (calendarRefreshTimer !== null) window.clearTimeout(calendarRefreshTimer);
      unlisten?.();
    };
  }, [scope, view, reload, reloadCalendarMeta]);

  // Silent automatic background sync on window focus/visibility restore (if stale > 60s)
  // plus periodic 2-minute sync in browser preview mode (in Tauri desktop, Rust drives
  // the 2-minute loop and emits mail://event).
  useEffect(() => {
    const runSilentAutoSync = async (): Promise<void> => {
      if (syncInFlightRef.current) return;
      const targets = accountsRef.current.filter((a) => a.sync.kind !== "blocked");
      if (targets.length === 0) return;

      syncInFlightRef.current = true;
      lastAutoSyncAtRef.current = Date.now();
      try {
        await Promise.allSettled(targets.map((a) => syncAccount(a.id)));
        await reloadMeta();
        await reload(scope, view);
      } catch {
        // Silent background sync; errors are reflected in account sync status.
      } finally {
        syncInFlightRef.current = false;
      }
    };

    const onWake = (): void => {
      if (document.visibilityState === "hidden") return;
      if (Date.now() - lastAutoSyncAtRef.current < 60_000) return;
      void runSilentAutoSync();
    };

    window.addEventListener("focus", onWake);
    document.addEventListener("visibilitychange", onWake);

    const previewTimer = !isTauriRuntime()
      ? window.setInterval(() => {
          void runSilentAutoSync();
        }, 120_000)
      : null;

    return () => {
      window.removeEventListener("focus", onWake);
      document.removeEventListener("visibilitychange", onWake);
      if (previewTimer !== null) window.clearInterval(previewTimer);
    };
  }, [scope, view, reload, reloadMeta]);

  function openThread(id: ThreadId): void {
    setActiveSection("mail");
    setSelectedId(id);
    setExpanded(new Set());
    setReplyText("");
    setRemoteContent("blockAll");
    if (layout === "classic") setStage("thread");
  }

  function backToList(): void {
    setStage("list");
  }

  function selectSpace(id: AccountId | null): void {
    setActiveSection("mail");
    setScopeAccount(id);
  }

  function selectView(next: MailboxView): void {
    setActiveSection("mail");
    setView(next);
  }

  function selectCategory(category: InboxCategory): void {
    setActiveSection("mail");
    setView({ kind: "inbox", category });
  }

  function clearSelection(): void {
    setSelectedId(null);
    setThread(null);
    if (layout === "classic") setStage("list");
  }

  async function archiveThread(id: ThreadId): Promise<void> {
    try {
      const outcome = await actOnThreads({ kind: "archive" }, [id]);
      if (selectedId === id) clearSelection();
      await reload(scope, view);
      const undo = outcome.undoAction;
      if (undo) {
        showToast("Archived thread", {
          onUndo: async () => {
            await actOnThreads(undo, [id]);
            await reload(scope, view);
          },
        });
      }
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function trashThread(id: ThreadId): Promise<void> {
    try {
      await actOnThreads({ kind: "trash" }, [id]);
      if (selectedId === id) clearSelection();
      await reload(scope, view);
      await reloadMeta();
      showToast("Deleted (moved to Trash)", {
        onUndo: async () => {
          await actOnThreads({ kind: "unarchive" }, [id]);
          await reload(scope, view);
          await reloadMeta();
        },
      });
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function reportSpam(id: ThreadId): Promise<void> {
    try {
      await actOnThreads({ kind: "reportSpam" }, [id]);
      if (selectedId === id) clearSelection();
      await reload(scope, view);
      await reloadMeta();
      showToast("Reported as spam", {
        onUndo: async () => {
          await actOnThreads({ kind: "unarchive" }, [id]);
          await reload(scope, view);
          await reloadMeta();
        },
      });
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function starThread(id: ThreadId): Promise<void> {
    try {
      const current = threads.find((t) => t.id === id);
      const on = current ? !current.starred : true;
      await actOnThreads({ kind: "star", on }, [id]);
      setThread((prev) => (prev && prev.id === id ? { ...prev, starred: on } : prev));
      await reload(scope, view);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function snooze(id: ThreadId, until: UnixMs): Promise<void> {
    try {
      await snoozeThreadCommand(id, until);
      if (selectedId === id) clearSelection();
      await reload(scope, view);
      showToast(`Snoozed until ${formatDue(until)}`, {
        onUndo: async () => {
          await unsnoozeThread(id);
          await reload(scope, view);
        },
      });
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function followUp(id: ThreadId, dueAt: UnixMs): Promise<void> {
    try {
      const jobId = await setFollowUp(id, dueAt);
      setThread((prev) => (prev && prev.id === id ? { ...prev, followUpDue: dueAt } : prev));
      await reload(scope, view);
      await reloadMeta();
      showToast(`Follow-up set for ${formatDue(dueAt)}`, {
        onUndo: async () => {
          await dismissFollowUp(jobId);
          setThread((prev) =>
            prev && prev.id === id ? { ...prev, followUpDue: null } : prev,
          );
          await reload(scope, view);
          await reloadMeta();
        },
      });
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  function toggleRemote(): void {
    setRemoteContent((prev) =>
      prev === "blockAll" ? "allowImagesStripTrackers" : "blockAll",
    );
  }

  async function syncNow(): Promise<void> {
    if (syncing || syncInFlightRef.current) return;
    const targets = scopeAccount
      ? accounts.filter((a) => a.id === scopeAccount)
      : accounts;
    if (targets.length === 0) return;
    syncInFlightRef.current = true;
    lastAutoSyncAtRef.current = Date.now();
    setSyncing(true);
    try {
      const results = await Promise.allSettled(targets.map((a) => syncAccount(a.id)));
      await reloadMeta();
      await reload(scope, view);
      const succeeded = results.filter((r) => r.status === "fulfilled").length;
      const firstRejected = results.find(
        (r): r is PromiseRejectedResult => r.status === "rejected",
      );
      if (succeeded > 0 && !firstRejected) {
        showToast(succeeded === 1 ? "Sync complete" : `Synced ${succeeded} accounts`);
      } else if (succeeded > 0) {
        showToast(`Synced ${succeeded} of ${targets.length} accounts`);
      } else if (firstRejected) {
        showToast(friendlyError(firstRejected.reason));
      }
    } catch (err) {
      showToast(friendlyError(err));
    } finally {
      syncInFlightRef.current = false;
      setSyncing(false);
    }
  }

  async function undoSend(jobId: JobId): Promise<void> {
    try {
      const result = await cancelSend(jobId);
      await reload(scope, view);
      if (result === "tooLate") showToast("Too late — the message already sent");
      else showToast("Send cancelled");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function submitDraft(input: CreateDraftInput, mode: SendMode): Promise<void> {
    try {
      const draft = await createDraft(input);
      const receipt = await sendDraft(draft.id, mode);
      await reload(scope, view);
      await reloadMeta();
      if (mode.kind === "undoable") {
        showToast(`Sending in ${mode.undoSecs}s…`, {
          durationMs: mode.undoSecs * 1000,
          onUndo: async () => undoSend(receipt.jobId),
        });
      } else {
        showToast(`Scheduled to send ${formatDue(mode.sendAt)}`, {
          onUndo: async () => undoSend(receipt.jobId),
        });
      }
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function sendReply(mode: SendMode): Promise<void> {
    const activeThread = threadRef.current;
    const text = replyTextRef.current;
    if (!activeThread || !text.trim()) return;
    const account = accounts.find((a) => a.id === activeThread.accountId);
    if (!account) return;
    const last = activeThread.messages[activeThread.messages.length - 1];
    const input: CreateDraftInput = {
      accountId: account.id,
      replyToThread: activeThread.id,
      to: last ? [last.fromEmail] : [],
      cc: [],
      bcc: [],
      subject: /^re:/i.test(activeThread.subject)
        ? activeThread.subject
        : `Re: ${activeThread.subject}`,
      bodyText: text,
    };
    setReplyText("");
    await submitDraft(input, mode);
  }

  function openCompose(): void {
    setComposeOpen(true);
  }

  function closeCompose(): void {
    setComposeOpen(false);
    setComposeInsert(null);
  }

  function openAddAccount(): void {
    setAddAccountOpen(true);
  }

  function closeAddAccount(): void {
    setAddAccountOpen(false);
  }

  function openTemplates(): void {
    setTemplatesOpen(true);
  }

  function closeTemplates(): void {
    setTemplatesOpen(false);
  }

  function openSettings(): void {
    setSettingsOpen(true);
  }

  function closeSettings(): void {
    setSettingsOpen(false);
  }

  async function setDeveloperToolsEnabled(enabled: boolean): Promise<void> {
    try {
      const updated = await updateSettingsCommand({
        ...settings,
        developerToolsEnabled: enabled,
      });
      setSettings(updated);
      showToast(enabled ? "Developer Tools enabled" : "Developer Tools disabled");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  function openOnboarding(): void {
    setSettingsOpen(false);
    setCommandOpen(false);
    setTourOpen(false);
    setOnboardingOpen(true);
  }

  async function completeOnboarding(): Promise<void> {
    setOnboardingOpen(false);
    const shouldStartTour = !settings.tourCompleted;
    try {
      const updated = await updateSettingsCommand({
        ...settings,
        onboardingCompleted: true,
      });
      setSettings(updated);
    } catch (err) {
      showToast(friendlyError(err));
    }
    if (shouldStartTour) {
      setActiveSection("mail");
      setTourStep(0);
      setTourOpen(true);
    }
  }

  function startTour(): void {
    setSettingsOpen(false);
    setCommandOpen(false);
    setOnboardingOpen(false);
    setActiveSection("mail");
    setTourStep(0);
    setTourOpen(true);
  }

  function nextTourStep(): void {
    setTourStep((prev) => prev + 1);
  }

  function prevTourStep(): void {
    setTourStep((prev) => Math.max(0, prev - 1));
  }

  async function persistTourDone(): Promise<void> {
    setTourOpen(false);
    setTourStep(0);
    if (settings.tourCompleted) return;
    try {
      const updated = await updateSettingsCommand({
        ...settings,
        tourCompleted: true,
      });
      setSettings(updated);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function skipTour(): Promise<void> {
    await persistTourDone();
  }

  async function completeTour(): Promise<void> {
    await persistTourDone();
    showToast("Feature tour complete — replay anytime from Settings or Ctrl+K");
  }

  function openCommand(): void {
    setCommandOpen(true);
  }

  function closeCommand(): void {
    setCommandOpen(false);
  }

  function openPopover(
    mode: PopoverMode,
    anchor: Anchor,
    onPick: (until: UnixMs) => void | Promise<void>,
  ): void {
    setPopover({ mode, anchor, onPick });
  }

  function closePopover(): void {
    setPopover(null);
  }

  async function pickPopoverTime(unixMs: UnixMs): Promise<void> {
    const current = popover;
    setPopover(null);
    if (current) await current.onPick(unixMs);
  }

  function requestSnooze(id: ThreadId, anchor: Anchor): void {
    openPopover("snooze", anchor, (until) => snooze(id, until));
  }

  function requestFollowUp(id: ThreadId, anchor: Anchor): void {
    openPopover("followUp", anchor, (until) => followUp(id, until));
  }

  function requestSendLater(
    anchor: Anchor,
    pick: (until: UnixMs) => void | Promise<void>,
  ): void {
    openPopover("sendLater", anchor, pick);
  }

  function closeTopLayer(): void {
    if (tourOpen) {
      void skipTour();
      return;
    }
    if (popover !== null) {
      setPopover(null);
      return;
    }
    if (commandOpen) {
      setCommandOpen(false);
      return;
    }
    if (composeOpen) {
      closeCompose();
      return;
    }
    if (addAccountOpen) {
      closeAddAccount();
      return;
    }
    if (settingsOpen) {
      closeSettings();
      return;
    }
    if (templatesOpen) closeTemplates();
  }

  function focusReply(): void {
    setReplyNonce((n) => n + 1);
  }

  function toggleExpand(id: MessageId): void {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function insertTextSnippet(bodyText: string): void {
    if (composeOpen) {
      insertSeq.current += 1;
      setComposeInsert({ nonce: insertSeq.current, body: bodyText });
      return;
    }
    if (selectedId !== null) {
      setReplyText((prev) => (prev ? `${prev}\n\n${bodyText}` : bodyText));
      setReplyNonce((n) => n + 1);
      return;
    }
    showToast("Open a thread or start a compose to insert a snippet");
  }

  function insertTemplate(template: TemplateView): void {
    insertTextSnippet(template.bodyText);
  }

  function clearComposeInsert(): void {
    setComposeInsert(null);
  }

  async function persistTemplate(input: SaveTemplateInput): Promise<void> {
    try {
      await saveTemplateCommand(input);
      await reloadMeta();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeTemplate(id: TemplateId): Promise<void> {
    try {
      await deleteTemplate(id);
      await reloadMeta();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistSignature(input: SaveSignatureInput): Promise<SignatureView> {
    try {
      const saved = await saveSignatureCommand(input);
      await reloadMeta();
      showToast(`Saved signature “${saved.name}”`);
      return saved;
    } catch (err) {
      showToast(friendlyError(err));
      throw err;
    }
  }

  async function removeSignature(id: SignatureId): Promise<void> {
    try {
      await deleteSignatureCommand(id);
      await reloadMeta();
      showToast("Deleted signature");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function downloadAttachment(id: AttachmentId): Promise<AttachmentData> {
    try {
      return await downloadAttachmentCommand(id);
    } catch (err) {
      showToast(friendlyError(err));
      throw err;
    }
  }

  async function persistSavedSearch(input: SaveSavedSearchInput): Promise<void> {
    try {
      await saveSavedSearchCommand(input);
      await reloadMeta();
      showToast(`Saved search “${input.name}”`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeSavedSearch(id: SavedSearchId): Promise<void> {
    try {
      await deleteSavedSearchCommand(id);
      await reloadMeta();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function screenSender(
    address: string,
    displayName: string | null,
    decision: ScreenerDecision,
  ): Promise<void> {
    try {
      await screenSenderCommand(address, displayName, decision);
      await Promise.all([reloadMeta(), reload(scope, view)]);
      showToast(`Routed ${address} → ${decision}`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function addVipSender(address: string, displayName: string | null): Promise<void> {
    try {
      await addVipSenderCommand(address, displayName);
      await Promise.all([reloadMeta(), reload(scope, view)]);
      showToast(`Added ${address} to VIP`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeVipSender(address: string): Promise<void> {
    try {
      await removeVipSenderCommand(address);
      await Promise.all([reloadMeta(), reload(scope, view)]);
      showToast(`Removed ${address} from VIP`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistLabel(input: SaveLabelInput): Promise<void> {
    try {
      await saveLabelCommand(input);
      await reloadMeta();
      showToast(`Saved label “${input.name}”`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeLabel(id: LabelId): Promise<void> {
    try {
      await deleteLabelCommand(id);
      await Promise.all([reloadMeta(), reload(scope, view)]);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function labelThreads(
    labelId: LabelId,
    threadIds: ThreadId[],
    add: boolean,
  ): Promise<void> {
    try {
      await labelThreadsCommand(labelId, threadIds, add);
      await Promise.all([reloadMeta(), reload(scope, view)]);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistRule(input: SaveMailRuleInput): Promise<void> {
    try {
      await saveRuleCommand(input);
      await reloadMeta();
      showToast(`Saved rule “${input.name}”`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeRule(id: RuleId): Promise<void> {
    try {
      await deleteRuleCommand(id);
      await reloadMeta();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function runRuleNow(id: RuleId): Promise<void> {
    try {
      const matched = await runRuleNowCommand(id);
      await Promise.all([reloadMeta(), reload(scope, view)]);
      showToast(`Rule applied to ${matched} ${matched === 1 ? "thread" : "threads"}`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function addAccount(input: NewAccountInput): Promise<void> {
    try {
      const created = await addAccountCommand(input);
      const nextScope: Scope = { kind: "account", accountId: created.id };
      const nextView: MailboxView = { kind: "inbox", category: "primary" };
      setScopeAccount(created.id);
      setView(nextView);
      await reloadMeta();
      showToast(`Added ${created.displayName} — syncing…`);
      setSyncing(true);
      try {
        await syncAccount(created.id);
        await reloadMeta();
        await reload(nextScope, nextView);
        showToast(`Synced ${created.displayName}`);
      } catch (syncErr) {
        await reloadMeta();
        await reload(nextScope, nextView);
        showToast(`${created.displayName}: ${friendlyError(syncErr)}`, { durationMs: 8000 });
      } finally {
        setSyncing(false);
      }
    } catch (err) {
      showToast(friendlyError(err));
      throw err;
    }
  }

  async function oauthSignInAccount(input: OAuthSignInInput): Promise<AccountView> {
    try {
      const created = await oauthSignInAccountCommand(input);
      const nextScope: Scope = { kind: "account", accountId: created.id };
      const nextView: MailboxView = { kind: "inbox", category: "primary" };
      setScopeAccount(created.id);
      setView(nextView);
      await reloadMeta();
      showToast(`Connected ${created.email} — syncing…`);
      void (async () => {
        setSyncing(true);
        try {
          await syncAccount(created.id);
          await reloadMeta();
          await reload(nextScope, nextView);
          showToast(`Synced ${created.email}`);
        } catch (syncErr) {
          await reloadMeta();
          await reload(nextScope, nextView);
          showToast(`${created.email}: ${friendlyError(syncErr)}`, { durationMs: 8000 });
        } finally {
          setSyncing(false);
        }
      })();
      return created;
    } catch (err) {
      const msg = friendlyError(err);
      showToast(msg, { durationMs: 8000 });
      throw new Error(msg);
    }
  }

  async function updateAccount(input: UpdateAccountInput): Promise<AccountView> {
    try {
      const updated = await updateAccountCommand(input);
      await reloadMeta();
      await reload(scope, view);
      return updated;
    } catch (err) {
      const msg = friendlyError(err);
      showToast(msg);
      throw new Error(msg);
    }
  }

  async function reauthAccount(id: AccountId): Promise<void> {
    if (syncing) return;
    setSyncing(true);
    try {
      showToast("Opening browser to sign in…");
      await reauthOAuthAccountCommand(id);
      await reloadMeta();
      await reload(scope, view);
      showToast("Signed in and synced");
    } catch (err) {
      await reloadMeta();
      showToast(friendlyError(err), { durationMs: 8000 });
    } finally {
      setSyncing(false);
    }
  }

  async function removeAccount(id: AccountId): Promise<void> {
    try {
      await removeAccountCommand(id);
      if (scopeAccount === id) setScopeAccount(null);
      await reloadMeta();
      await reload(scope, view);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  function moveSelection(delta: number): void {
    if (threads.length === 0) return;
    const index = selectedId ? threads.findIndex((t) => t.id === selectedId) : -1;
    let next: number;
    if (index === -1) next = delta > 0 ? 0 : threads.length - 1;
    else next = Math.min(threads.length - 1, Math.max(0, index + delta));
    const target = threads[next];
    if (target) openThread(target.id);
  }

  function handleKeyDown(event: KeyboardEvent): void {
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === "k") {
      event.preventDefault();
      if (commandOpen) setCommandOpen(false);
      else setCommandOpen(true);
      return;
    }
    if (mod && event.key === ",") {
      event.preventDefault();
      setSettingsOpen((prev) => !prev);
      return;
    }
    if (event.key === "Escape") {
      closeTopLayer();
      return;
    }

    const target = event.target;
    const inField =
      target instanceof HTMLElement &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable);
    if (inField || mod || event.altKey) return;
    if (
      commandOpen ||
      composeOpen ||
      addAccountOpen ||
      templatesOpen ||
      settingsOpen ||
      onboardingOpen ||
      tourOpen ||
      popover !== null ||
      activeSection === "calendar"
    ) {
      return;
    }

    const key = event.key.toLowerCase();
    switch (key) {
      case "j":
        event.preventDefault();
        moveSelection(1);
        break;
      case "k":
        event.preventDefault();
        moveSelection(-1);
        break;
      case "e":
        if (selectedId) void archiveThread(selectedId);
        break;
      case "#":
      case "delete":
        if (selectedId) {
          event.preventDefault();
          void trashThread(selectedId);
        }
        break;
      case "!":
        if (selectedId) {
          event.preventDefault();
          void reportSpam(selectedId);
        }
        break;
      case "h":
        if (selectedId) requestSnooze(selectedId, centerAnchor());
        break;
      case "f":
        if (selectedId) requestFollowUp(selectedId, centerAnchor());
        break;
      case "s":
        if (selectedId) void starThread(selectedId);
        break;
      case "c":
        setComposeOpen(true);
        break;
      case "r":
        if (selectedId) focusReply();
        break;
      case "/":
        event.preventDefault();
        setCommandOpen(true);
        break;
      default:
        break;
    }
  }

  useEffect(() => {
    keyHandlerRef.current = handleKeyDown;
  });

  useEffect(() => {
    const listener = (event: KeyboardEvent) => keyHandlerRef.current(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  function openCalendarSection(): void {
    setActiveSection("calendar");
    setSelectedId(null);
    setThread(null);
    void reloadCalendarMeta().catch(() => undefined);
  }

  const loadCalendarRange = useCallback(async (rangeStart: UnixMs, rangeEnd: UnixMs) => {
    try {
      const [occs, blocks, overlays] = await Promise.all([
        listCalendarEvents({ rangeStart, rangeEnd }),
        listOverlayBusyBlocks({ rangeStart, rangeEnd }),
        listIcsOverlays(),
      ]);
      setCalendarOccurrences(occs);
      setOverlayBusyBlocks(blocks);
      setIcsOverlays(overlays);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }, [showToast]);

  async function selectCalendarEvent(event: CalendarEventView | null): Promise<void> {
    setSelectedCalendarEvent(event);
  }

  async function persistCalendarEvent(input: SaveCalendarEventInput): Promise<CalendarEventView> {
    const saved = await saveCalendarEventCommand(input);
    setSelectedCalendarEvent(saved);
    showToast(input.id ? "Calendar event updated" : "Calendar event created");
    return saved;
  }

  async function quickAddEvent(text: string): Promise<CalendarEventView> {
    try {
      const created = await quickAddCalendarEventCommand(text);
      showToast(`Added "${created.title}" to Calendar`);
      return created;
    } catch (err) {
      showToast(friendlyError(err));
      throw err;
    }
  }

  async function removeCalendarEvent(id: CalendarEventId): Promise<void> {
    try {
      await deleteCalendarEventCommand(id);
      if (selectedCalendarEvent?.id === id) {
        setSelectedCalendarEvent(null);
      }
      showToast("Calendar event deleted");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function rsvpInvite(input: RsvpInviteInput): Promise<void> {
    try {
      const updated = await rsvpCalendarInviteCommand(input);
      setThread((prev) =>
        prev
          ? {
              ...prev,
              messages: prev.messages.map((m) =>
                m.id === input.messageId
                  ? {
                      ...m,
                      invites: (m.invites ?? []).map((inv) =>
                        inv.uid === input.uid ? updated : inv,
                      ),
                    }
                  : m,
              ),
            }
          : prev,
      );
      showToast(`RSVP updated: ${input.status}`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function addInviteToCalendar(input: AddInviteToCalendarInput): Promise<void> {
    try {
      const ev = await addInviteToCalendarCommand(input);
      setThread((prev) =>
        prev
          ? {
              ...prev,
              messages: prev.messages.map((m) =>
                m.id === input.messageId
                  ? {
                      ...m,
                      invites: (m.invites ?? []).map((inv) =>
                        inv.uid === input.uid ? { ...inv, addedEventId: ev.id } : inv,
                      ),
                    }
                  : m,
              ),
            }
          : prev,
      );
      showToast(`Added "${ev.title}" to Calendar`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistIcsOverlay(input: SaveIcsOverlayInput): Promise<IcsOverlayView> {
    const saved = await saveIcsOverlayCommand(input);
    const next = await listIcsOverlays();
    setIcsOverlays(next);
    return saved;
  }

  async function removeIcsOverlay(id: IcsOverlayId): Promise<void> {
    try {
      await deleteIcsOverlayCommand(id);
      const next = await listIcsOverlays();
      setIcsOverlays(next);
      showToast("Personal .ics overlay removed");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function syncIcsOverlay(id: IcsOverlayId): Promise<void> {
    try {
      await refreshIcsOverlayCommand(id);
      const next = await listIcsOverlays();
      setIcsOverlays(next);
      showToast("Personal .ics overlay refreshed");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function saveAgentPermission(input: SaveAgentAccountPermissionInput): Promise<void> {
    try {
      await saveAgentPermissionCommand(input);
      setAgentPermissions(await listAgentPermissions());
      showToast("Agent permissions updated");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function saveAgentBridgeConfig(input: SaveAgentBridgeConfigInput): Promise<void> {
    try {
      const updated = await saveAgentBridgeConfigCommand(input);
      setAgentBridgeConfig(updated);
      showToast("Hermes MCP bridge settings saved");
    } catch (err) {
      showToast(friendlyError(err));
      throw err;
    }
  }

  async function undoAgentActivity(activityId: AgentActivityId): Promise<void> {
    try {
      await undoAgentActivityCommand(activityId);
      setAgentActivity(await listAgentActivity(50));
      await reload(scope, view);
      showToast("Agent action undone");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function approveAgentDraft(draftId: DraftId, mode: SendMode): Promise<void> {
    try {
      const receipt = await approveAgentDraftCommand(draftId, mode);
      setPendingAgentDrafts(await listPendingAgentDrafts());
      await reloadMeta();
      await reload(scope, view);
      showToast("Agent draft approved & queued", {
        durationMs: 7000,
        onUndo: async () => {
          await cancelSend(receipt.jobId);
          setPendingAgentDrafts(await listPendingAgentDrafts());
          await reloadMeta();
          showToast("Send cancelled: draft returned to Pending Approval");
        },
      });
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function rejectAgentDraft(draftId: DraftId, reason: string): Promise<void> {
    try {
      await rejectAgentDraftCommand(draftId, reason);
      setPendingAgentDrafts(await listPendingAgentDrafts());
      showToast("Agent draft rejected");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function saveAiConfig(input: SaveAiConfigInput): Promise<void> {
    try {
      const updated = await saveAiConfigCommand(input);
      setAiConfig(updated);
      showToast("In-pane AI settings saved");
    } catch (err) {
      showToast(friendlyError(err));
      throw err;
    }
  }

  async function aiSummarizeThread(threadId: ThreadId): Promise<AiSummaryResult> {
    return aiSummarizeThreadCommand(threadId);
  }

  async function aiAskThread(
    threadId: ThreadId,
    question: string,
    selectedText: string | null,
  ): Promise<AiAnswerResult> {
    return aiAskThreadCommand(threadId, question, selectedText);
  }

  async function aiDraftReply(
    threadId: ThreadId,
    instructions: string,
    stageForApproval: boolean,
  ): Promise<AiDraftResult> {
    const res = await aiDraftReplyCommand(threadId, instructions, stageForApproval);
    if (stageForApproval && res.stagedDraft) {
      setPendingAgentDrafts(await listPendingAgentDrafts());
      setAgentActivity(await listAgentActivity(50));
    }
    return res;
  }

  return {
    booting,
    error,
    accounts,
    savedOAuthClients,
    settings,
    counts,
    threads,
    jobs,
    templates,
    signatures,
    savedSearches,
    screenerSenders,
    vipSenders,
    labels,
    rules,
    screenerOpen,
    rulesOpen,
    scopeAccount,
    view,
    selectedId,
    thread,
    theme,
    layout,
    chromeTextSize,
    readerTextSize,
    stage,
    toast,
    commandOpen,
    composeOpen,
    addAccountOpen,
    templatesOpen,
    settingsOpen,
    onboardingOpen,
    tourOpen,
    tourStep,
    popover,
    replyText,
    replyNonce,
    expanded,
    remoteContent,
    syncing: syncing || accounts.some((a) => a.sync.kind === "syncing"),
    composeInsert,
    activeSection,
    calendarOccurrences,
    icsOverlays,
    overlayBusyBlocks,
    selectedCalendarEvent,
    agentPermissions,
    agentBridgeConfig,
    agentActivity,
    pendingAgentDrafts,
    aiConfig,
    agentModalOpen,
    toggleTheme: () => setTheme((prev) => (prev === "dark" ? "light" : "dark")),
    setTheme,
    toggleLayout: () => setLayout((prev) => (prev === "split" ? "classic" : "split")),
    setLayout,
    setChromeTextSize,
    setReaderTextSize,
    cycleReaderTextSize: () =>
      setReaderTextSize((prev) =>
        prev === "small" ? "medium" : prev === "medium" ? "large" : "small",
      ),
    openOnboarding,
    completeOnboarding,
    startTour,
    nextTourStep,
    prevTourStep,
    skipTour,
    completeTour,
    backToList,
    selectSpace,
    selectView,
    selectCategory,
    openThread,
    archiveThread,
    trashThread,
    reportSpam,
    starThread,
    snooze,
    followUp,
    toggleRemote,
    syncNow,
    openCompose,
    closeCompose,
    openAddAccount,
    closeAddAccount,
    openTemplates,
    closeTemplates,
    openSettings,
    closeSettings,
    openScreener: () => setScreenerOpen(true),
    closeScreener: () => setScreenerOpen(false),
    openRules: () => setRulesOpen(true),
    closeRules: () => setRulesOpen(false),
    persistSavedSearch,
    removeSavedSearch,
    screenSender,
    addVipSender,
    removeVipSender,
    persistLabel,
    removeLabel,
    labelThreads,
    persistRule,
    removeRule,
    runRuleNow,
    setDeveloperToolsEnabled,
    openCommand,
    closeCommand,
    closeTopLayer,
    openPopover,
    closePopover,
    pickPopoverTime,
    requestSnooze,
    requestFollowUp,
    requestSendLater,
    setReplyText,
    focusReply,
    toggleExpand,
    submitDraft,
    sendReply,
    showToast,
    dismissToast,
    insertTemplate,
    clearComposeInsert,
    persistTemplate,
    removeTemplate,
    persistSignature,
    removeSignature,
    downloadAttachment,
    addAccount,
    oauthSignInAccount,
    updateAccount,
    reauthAccount,
    removeAccount,
    insertTextSnippet,
    openCalendarSection,
    loadCalendarRange,
    selectCalendarEvent,
    persistCalendarEvent,
    quickAddEvent,
    removeCalendarEvent,
    rsvpInvite,
    addInviteToCalendar,
    persistIcsOverlay,
    removeIcsOverlay,
    syncIcsOverlay,
    openAgentModal: () => setAgentModalOpen(true),
    closeAgentModal: () => setAgentModalOpen(false),
    saveAgentPermission,
    saveAgentBridgeConfig,
    undoAgentActivity,
    approveAgentDraft,
    rejectAgentDraft,
    saveAiConfig,
    aiSummarizeThread,
    aiAskThread,
    aiDraftReply,
  };
}
