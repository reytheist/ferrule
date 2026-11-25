import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  actOnThreads,
  addAccount as addAccountCommand,
  addInviteToPigmentCalendar as addInviteToPigmentCalendarCommand,
  addSharedComment as addSharedCommentCommand,
  cancelSend,
  connectStaffWorkspace as connectStaffWorkspaceCommand,
  createDraft,
  deleteActionItem as deleteActionItemCommand,
  deleteCalendarEvent as deleteCalendarEventCommand,
  deleteIcsOverlay as deleteIcsOverlayCommand,
  deletePinnedLink as deletePinnedLinkCommand,
  deleteSharedTemplate as deleteSharedTemplateCommand,
  deleteShowContact as deleteShowContactCommand,
  deleteTemplate,
  disconnectStaffWorkspace as disconnectStaffWorkspaceCommand,
  dismissFollowUp,
  exportActionItemsCsv,
  exportShowContactsCsv,
  findSharedThreadForLocal,
  formatMeetingRecap as formatMeetingRecapCommand,
  getDiscordWebhookStatus,
  getMeetingDoc as getMeetingDocCommand,
  getSettings,
  getStaffWorkspace,
  importOperationsWorkbook as importOperationsWorkbookCommand,
  listAccounts,
  listActionItems,
  listCalendarEvents,
  listIcsOverlays,
  listJobs,
  listOverlayBusyBlocks,
  listPinnedLinks,
  listSharedTemplates,
  listSharedThreads,
  listShowContacts,
  listStaffDirectory,
  listTemplates,
  listThreads,
  mailboxCounts,
  oauthSignInAccount as oauthSignInAccountCommand,
  openExternalUrl,
  openSharedThread as openSharedThreadCommand,
  openThread as openThreadCommand,
  postMeetingRecapToDiscord as postMeetingRecapToDiscordCommand,
  promoteToActionItem as promoteToActionItemCommand,
  quickAddCalendarEvent as quickAddCalendarEventCommand,
  reauthOAuthAccount as reauthOAuthAccountCommand,
  refreshIcsOverlay as refreshIcsOverlayCommand,
  removeAccount as removeAccountCommand,
  rsvpCalendarInvite as rsvpCalendarInviteCommand,
  saveActionItem as saveActionItemCommand,
  saveCalendarEvent as saveCalendarEventCommand,
  saveIcsOverlay as saveIcsOverlayCommand,
  savePinnedLink as savePinnedLinkCommand,
  saveSharedTemplate as saveSharedTemplateCommand,
  saveShowContact as saveShowContactCommand,
  saveTemplate as saveTemplateCommand,
  savedOAuthClients as savedOAuthClientsCommand,
  sendDraft,
  setDiscordWebhook as setDiscordWebhookCommand,
  setFollowUp,
  shareThread as shareThreadCommand,
  signalAppReady,
  snoozeThread as snoozeThreadCommand,
  syncAccount,
  syncStaffWorkspace as syncStaffWorkspaceCommand,
  tickScheduler,
  toggleActionItem as toggleActionItemCommand,
  unsnoozeThread,
  updateAccount as updateAccountCommand,
  updateMeetingDoc as updateMeetingDocCommand,
  updateSettings as updateSettingsCommand,
  updateSharedDraft as updateSharedDraftCommand,
  updateSharedHandoff as updateSharedHandoffCommand,
} from "../../ipc/core";
import type {
  AccountId,
  AccountView,
  ActionItemId,
  ActionItemStatus,
  ActionItemView,
  AddInviteToCalendarInput,
  AddSharedCommentInput,
  AppSettings,
  CalendarEventId,
  CalendarEventView,
  CalendarOccurrence,
  ConnectStaffWorkspaceInput,
  ContactId,
  CreateDraftInput,
  IcsOverlayId,
  IcsOverlayView,
  ImportWorkbookInput,
  InboxCategory,
  JobId,
  JobView,
  MailboxCounts,
  MailboxView,
  MeetingDocView,
  MessageId,
  NewAccountInput,
  OAuthSignInInput,
  OverlayBusyBlock,
  PinnedLinkId,
  PinnedLinkView,
  PromoteActionItemInput,
  RemoteContent,
  RsvpInviteInput,
  SaveActionItemInput,
  SaveCalendarEventInput,
  SaveIcsOverlayInput,
  SavePinnedLinkInput,
  SaveSharedTemplateInput,
  SaveShowContactInput,
  SaveTemplateInput,
  SavedOAuthClients,
  Scope,
  SendMode,
  ShareThreadInput,
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
  ThreadSummary,
  ThreadView,
  UnixMs,
  UpdateAccountInput,
  UpdateHandoffInput,
  UpdateMeetingDocInput,
  UpdateSharedDraftInput,
  WorkbookImportReport,
  WorkbookImportTarget,
} from "../../types/mail";
import type { ActionItemDraftSeed } from "../actions/ActionItemModal";
import { formatDue } from "../ui/format";

export type Theme = "dark" | "light";
export type Layout = "split" | "classic";
export type TextSize = "small" | "medium" | "large";
export type Stage = "list" | "thread";
export type PopoverMode = "snooze" | "followUp" | "sendLater";
export type ActiveSection = "mail" | "shared" | "calendar" | "actions";

const EMPTY_STAFF_WORKSPACE: StaffWorkspaceView = {
  connected: false,
  serverUrl: "",
  staffName: "",
  staffHandle: "",
  staffEmail: "",
  staffRole: "",
  connectedAt: null,
  lastSyncAt: null,
  status: "disconnected",
  lastError: null,
  pendingOutboxCount: 0,
};

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
  shareModalOpen: boolean;
  contactsOpen: boolean;
  pinnedLinksOpen: boolean;
  popover: PopoverState | null;
  replyText: string;
  replyNonce: number;
  expanded: ReadonlySet<MessageId>;
  remoteContent: RemoteContent;
  syncing: boolean;
  composeInsert: { nonce: number; body: string } | null;
  activeSection: ActiveSection;
  staffWorkspace: StaffWorkspaceView;
  staffDirectory: StaffMemberView[];
  sharedThreads: SharedThreadSummary[];
  sharedTemplates: SharedTemplateView[];
  showContacts: ShowContactView[];
  pinnedLinks: PinnedLinkView[];
  selectedSharedId: SharedThreadId | null;
  activeSharedThread: SharedThreadView | null;
  calendarOccurrences: CalendarOccurrence[];
  icsOverlays: IcsOverlayView[];
  overlayBusyBlocks: OverlayBusyBlock[];
  selectedCalendarEvent: CalendarEventView | null;
  activeMeetingDoc: MeetingDocView | null;
  discordWebhookConfigured: boolean;
  actionItems: ActionItemView[];
  actionItemModalSeed: ActionItemDraftSeed | null;
  workbookImportOpen: boolean;
  workbookImportTarget: WorkbookImportTarget;

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
  addAccount(input: NewAccountInput): Promise<void>;
  oauthSignInAccount(input: OAuthSignInInput): Promise<AccountView>;
  updateAccount(input: UpdateAccountInput): Promise<AccountView>;
  reauthAccount(id: AccountId): Promise<void>;
  removeAccount(id: AccountId): Promise<void>;
  openSharedSection(): void;
  openSharedThread(id: SharedThreadId): Promise<void>;
  openShareModal(): void;
  closeShareModal(): void;
  openContacts(): void;
  closeContacts(): void;
  openPinnedLinks(): void;
  closePinnedLinks(): void;
  connectStaffWorkspace(input: ConnectStaffWorkspaceInput): Promise<StaffWorkspaceView>;
  disconnectStaffWorkspace(): Promise<void>;
  syncStaffWorkspace(): Promise<void>;
  shareThread(input: ShareThreadInput): Promise<SharedThreadView>;
  updateHandoff(input: UpdateHandoffInput): Promise<void>;
  postSharedComment(input: AddSharedCommentInput): Promise<void>;
  mutateSharedDraft(input: UpdateSharedDraftInput): Promise<void>;
  loadApprovedDraftToReply(bodyText: string): void;
  insertTextSnippet(bodyText: string): void;
  persistSharedTemplate(input: SaveSharedTemplateInput): Promise<void>;
  removeSharedTemplate(id: SharedTemplateId): Promise<void>;
  persistShowContact(input: SaveShowContactInput): Promise<void>;
  removeShowContact(id: ContactId): Promise<void>;
  persistPinnedLink(input: SavePinnedLinkInput): Promise<void>;
  removePinnedLink(id: PinnedLinkId): Promise<void>;
  launchPinnedLink(url: string): Promise<void>;
  openCalendarSection(): void;
  loadCalendarRange(rangeStart: UnixMs, rangeEnd: UnixMs): Promise<void>;
  selectCalendarEvent(event: CalendarEventView): Promise<void>;
  persistCalendarEvent(input: SaveCalendarEventInput): Promise<CalendarEventView>;
  quickAddEvent(text: string): Promise<CalendarEventView>;
  removeCalendarEvent(id: CalendarEventId): Promise<void>;
  mutateMeetingDoc(input: UpdateMeetingDocInput): Promise<void>;
  promoteAgendaToAction(input: PromoteActionItemInput): Promise<void>;
  setActionItemStatus(id: ActionItemId, status: ActionItemStatus): Promise<void>;
  rsvpInvite(input: RsvpInviteInput): Promise<void>;
  addInviteToCalendar(input: AddInviteToCalendarInput): Promise<void>;
  persistIcsOverlay(input: SaveIcsOverlayInput): Promise<IcsOverlayView>;
  removeIcsOverlay(id: IcsOverlayId): Promise<void>;
  syncIcsOverlay(id: IcsOverlayId): Promise<void>;
  saveDiscordWebhook(url: string | null): Promise<void>;
  copyDiscordRecap(eventId: CalendarEventId): Promise<void>;
  postDiscordRecap(eventId: CalendarEventId): Promise<void>;
  openActionsSection(): void;
  openNewActionItemModal(seed?: ActionItemDraftSeed): void;
  closeNewActionItemModal(): void;
  openWorkbookImport(target?: WorkbookImportTarget): void;
  closeWorkbookImport(): void;
  persistActionItem(input: SaveActionItemInput): Promise<ActionItemView>;
  removeActionItem(id: ActionItemId): Promise<void>;
  jumpToCalendarEvent(eventId: CalendarEventId): Promise<void>;
  importWorkbook(input: ImportWorkbookInput): Promise<WorkbookImportReport>;
  downloadActionItemsCsv(): Promise<void>;
  downloadShowContactsCsv(): Promise<void>;
}

export function useMailApp(): MailApp {
  const [booting, setBooting] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<AccountView[]>([]);
  const [savedOAuthClients, setSavedOAuthClients] = useState<SavedOAuthClients>({
    gmailClientId: null,
    outlookClientId: null,
    outlookTenant: null,
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
  const [shareModalOpen, setShareModalOpen] = useState(false);
  const [contactsOpen, setContactsOpen] = useState(false);
  const [pinnedLinksOpen, setPinnedLinksOpen] = useState(false);
  const [popover, setPopover] = useState<PopoverState | null>(null);
  const [replyText, setReplyText] = useState("");
  const [replyNonce, setReplyNonce] = useState(0);
  const [expanded, setExpanded] = useState<ReadonlySet<MessageId>>(() => new Set());
  const [syncing, setSyncing] = useState(false);
  const [composeInsert, setComposeInsert] = useState<{ nonce: number; body: string } | null>(null);
  const [activeSection, setActiveSection] = useState<ActiveSection>("mail");
  const [staffWorkspace, setStaffWorkspace] = useState<StaffWorkspaceView>(EMPTY_STAFF_WORKSPACE);
  const [staffDirectory, setStaffDirectory] = useState<StaffMemberView[]>([]);
  const [sharedThreads, setSharedThreads] = useState<SharedThreadSummary[]>([]);
  const [sharedTemplates, setSharedTemplates] = useState<SharedTemplateView[]>([]);
  const [showContacts, setShowContacts] = useState<ShowContactView[]>([]);
  const [pinnedLinks, setPinnedLinks] = useState<PinnedLinkView[]>([]);
  const [selectedSharedId, setSelectedSharedId] = useState<SharedThreadId | null>(null);
  const [activeSharedThread, setActiveSharedThread] = useState<SharedThreadView | null>(null);
  const [calendarOccurrences, setCalendarOccurrences] = useState<CalendarOccurrence[]>([]);
  const [icsOverlays, setIcsOverlays] = useState<IcsOverlayView[]>([]);
  const [overlayBusyBlocks, setOverlayBusyBlocks] = useState<OverlayBusyBlock[]>([]);
  const [selectedCalendarEvent, setSelectedCalendarEvent] = useState<CalendarEventView | null>(
    null,
  );
  const [activeMeetingDoc, setActiveMeetingDoc] = useState<MeetingDocView | null>(null);
  const [discordWebhookConfigured, setDiscordWebhookConfigured] = useState(false);
  const [actionItems, setActionItems] = useState<ActionItemView[]>([]);
  const [actionItemModalSeed, setActionItemModalSeed] = useState<ActionItemDraftSeed | null>(null);
  const [workbookImportOpen, setWorkbookImportOpen] = useState(false);
  const [workbookImportTarget, setWorkbookImportTarget] = useState<WorkbookImportTarget>("auto");

  const toastSeq = useRef(0);
  const insertSeq = useRef(0);
  const jobsSignature = useRef<string>("");
  const themeRef = useRef<Theme>(theme);
  const replyTextRef = useRef(replyText);
  const threadRef = useRef<ThreadView | null>(thread);
  const keyHandlerRef = useRef<(event: KeyboardEvent) => void>(() => undefined);

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

  const reloadStaff = useCallback(async () => {
    const nowMs = Date.now();
    const [ws, dir, shared, sharedTpls, contacts, links, overlays, webhook, upcoming, actions] =
      await Promise.all([
        getStaffWorkspace(),
        listStaffDirectory(),
        listSharedThreads(),
        listSharedTemplates(),
        listShowContacts(),
        listPinnedLinks(),
        listIcsOverlays(),
        getDiscordWebhookStatus(),
        listCalendarEvents({
          rangeStart: nowMs - 7 * 86_400_000,
          rangeEnd: nowMs + 30 * 86_400_000,
        }),
        listActionItems(),
      ]);
    setStaffWorkspace(ws);
    setStaffDirectory(dir);
    setSharedThreads(shared);
    setSharedTemplates(sharedTpls);
    setShowContacts(contacts);
    setPinnedLinks(links);
    setIcsOverlays(overlays);
    setDiscordWebhookConfigured(webhook.configured);
    setCalendarOccurrences(upcoming);
    setActionItems(actions);
  }, []);

  const reloadMeta = useCallback(async () => {
    const [nextAccounts, nextJobs, nextTemplates, nextClients, nextSettings] = await Promise.all([
      listAccounts(),
      listJobs(),
      listTemplates(),
      savedOAuthClientsCommand(),
      getSettings(),
    ]);
    setAccounts(nextAccounts);
    setJobs(nextJobs);
    setTemplates(nextTemplates);
    setSavedOAuthClients(nextClients);
    setSettings(nextSettings);
    jobsSignature.current = nextJobs.map((j) => `${j.id}:${j.state}`).join("|");
    await reloadStaff();
    return { accounts: nextAccounts, settings: nextSettings };
  }, [reloadStaff]);

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

  // Keep refs fresh so popover callbacks read current reply state.
  useEffect(() => {
    replyTextRef.current = replyText;
    threadRef.current = thread;
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
        void findSharedThreadForLocal(next.id)
          .then((shared) => {
            if (!cancelled) setActiveSharedThread(shared);
          })
          .catch(() => undefined);
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

  function openThread(id: ThreadId): void {
    setActiveSection("mail");
    setSelectedSharedId(null);
    setActiveSharedThread(null);
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
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    setScopeAccount(id);
  }

  function selectView(next: MailboxView): void {
    setActiveSection("mail");
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    setView(next);
  }

  function selectCategory(category: InboxCategory): void {
    setActiveSection("mail");
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    setView({ kind: "inbox", category });
  }

  function openSharedSection(): void {
    setActiveSection("shared");
    setSelectedId(null);
    setThread(null);
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    if (layout === "classic") setStage("list");
    void reloadStaff().catch(() => undefined);
  }

  async function openSharedThread(id: SharedThreadId): Promise<void> {
    try {
      setActiveSection("shared");
      setSelectedId(null);
      setThread(null);
      setSelectedSharedId(id);
      const detail = await openSharedThreadCommand(id);
      setActiveSharedThread(detail);
      if (layout === "classic") setStage("thread");
    } catch (err) {
      showToast(friendlyError(err));
    }
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
    if (syncing) return;
    const targets = scopeAccount
      ? accounts.filter((a) => a.id === scopeAccount)
      : accounts;
    if (targets.length === 0) return;
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
    if (shareModalOpen) {
      setShareModalOpen(false);
      return;
    }
    if (contactsOpen) {
      setContactsOpen(false);
      return;
    }
    if (pinnedLinksOpen) {
      setPinnedLinksOpen(false);
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

  function loadApprovedDraftToReply(bodyText: string): void {
    setReplyText(bodyText);
    setReplyNonce((n) => n + 1);
    showToast("Loaded approved draft into reply dock");
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

  // ── Phase B: Staff Sync & Collaboration actions ──────────────────────────

  async function connectStaffWorkspace(
    input: ConnectStaffWorkspaceInput,
  ): Promise<StaffWorkspaceView> {
    try {
      const ws = await connectStaffWorkspaceCommand(input);
      await reloadStaff();
      showToast(`Connected to Pigment Staff Sync as @${ws.staffHandle}`);
      return ws;
    } catch (err) {
      const msg = friendlyError(err);
      showToast(msg, { durationMs: 7000 });
      throw new Error(msg);
    }
  }

  async function disconnectStaffWorkspace(): Promise<void> {
    try {
      await disconnectStaffWorkspaceCommand();
      await reloadStaff();
      showToast("Disconnected from Staff Sync");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function syncStaffWorkspace(): Promise<void> {
    try {
      const ws = await syncStaffWorkspaceCommand();
      await reloadStaff();
      if (activeSharedThread) {
        const refreshed = await openSharedThreadCommand(activeSharedThread.id).catch(() => null);
        if (refreshed) setActiveSharedThread(refreshed);
      }
      const offlineMsg =
        ws.pendingOutboxCount > 0
          ? `Staff Sync offline (${ws.pendingOutboxCount} pending), working from local cache`
          : "Staff Sync offline, working from local cache";
      showToast(
        ws.status === "connected"
          ? "Staff workspace synced"
          : ws.lastError ?? offlineMsg,
      );
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function shareThread(input: ShareThreadInput): Promise<SharedThreadView> {
    try {
      const shared = await shareThreadCommand(input);
      setActiveSharedThread(shared);
      await reloadStaff();
      showToast(
        input.visibility === "allStaff"
          ? "Shared thread with All Staff"
          : `Shared thread with ${shared.invitedHandles.map((h) => `@${h}`).join(", ")}`,
      );
      return shared;
    } catch (err) {
      const msg = friendlyError(err);
      showToast(msg, { durationMs: 7000 });
      throw new Error(msg);
    }
  }

  async function updateHandoff(input: UpdateHandoffInput): Promise<void> {
    try {
      const updated = await updateSharedHandoffCommand(input);
      setActiveSharedThread(updated);
      await reloadStaff();
      showToast(
        updated.assigneeHandle
          ? `Handed off to @${updated.assigneeHandle} (${updated.status})`
          : `Updated status to ${updated.status}`,
      );
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function postSharedComment(input: AddSharedCommentInput): Promise<void> {
    try {
      await addSharedCommentCommand(input);
      const refreshed = await openSharedThreadCommand(input.sharedThreadId);
      setActiveSharedThread(refreshed);
      await reloadStaff();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function mutateSharedDraft(input: UpdateSharedDraftInput): Promise<void> {
    try {
      await updateSharedDraftCommand(input);
      const refreshed = await openSharedThreadCommand(input.sharedThreadId);
      setActiveSharedThread(refreshed);
      await reloadStaff();
      if (input.action.kind === "requestReview") {
        showToast("Review requested and teammates notified");
      } else if (input.action.kind === "approve") {
        showToast("Reply draft approved — ready for mailbox owner to send");
      } else if (input.action.kind === "takeOver") {
        showToast("You took over the reply draft lock");
      }
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistSharedTemplate(input: SaveSharedTemplateInput): Promise<void> {
    try {
      await saveSharedTemplateCommand(input);
      await reloadStaff();
      showToast("Saved shared staff template");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeSharedTemplate(id: SharedTemplateId): Promise<void> {
    try {
      await deleteSharedTemplateCommand(id);
      await reloadStaff();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistShowContact(input: SaveShowContactInput): Promise<void> {
    try {
      await saveShowContactCommand(input);
      await reloadStaff();
      showToast("Saved show contact");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removeShowContact(id: ContactId): Promise<void> {
    try {
      await deleteShowContactCommand(id);
      await reloadStaff();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function persistPinnedLink(input: SavePinnedLinkInput): Promise<void> {
    try {
      await savePinnedLinkCommand(input);
      await reloadStaff();
      showToast("Saved pinned staff link");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function removePinnedLink(id: PinnedLinkId): Promise<void> {
    try {
      await deletePinnedLinkCommand(id);
      await reloadStaff();
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function launchPinnedLink(url: string): Promise<void> {
    try {
      await openExternalUrl(url);
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
      shareModalOpen ||
      contactsOpen ||
      pinnedLinksOpen ||
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
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    void reloadStaff().catch(() => undefined);
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

  async function selectCalendarEvent(event: CalendarEventView): Promise<void> {
    setSelectedCalendarEvent(event);
    try {
      const doc = await getMeetingDocCommand(event.id);
      setActiveMeetingDoc(doc);
    } catch (err) {
      showToast(friendlyError(err));
    }
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
      showToast(`Added "${created.title}" to Pigment Calendar`);
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
        setActiveMeetingDoc(null);
      }
      showToast("Calendar event deleted");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function mutateMeetingDoc(input: UpdateMeetingDocInput): Promise<void> {
    try {
      const updated = await updateMeetingDocCommand(input);
      setActiveMeetingDoc(updated);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function promoteAgendaToAction(input: PromoteActionItemInput): Promise<void> {
    try {
      const item = await promoteToActionItemCommand(input);
      const [refreshed, nextItems] = await Promise.all([
        getMeetingDocCommand(input.eventId),
        listActionItems(),
      ]);
      setActiveMeetingDoc(refreshed);
      setActionItems(nextItems);
      showToast(`Promoted to Action Item: ${item.title}`);
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function setActionItemStatus(id: ActionItemId, status: ActionItemStatus): Promise<void> {
    try {
      await toggleActionItemCommand(id, status);
      const nextItems = await listActionItems();
      setActionItems(nextItems);
      if (selectedCalendarEvent) {
        const refreshed = await getMeetingDocCommand(selectedCalendarEvent.id);
        setActiveMeetingDoc(refreshed);
      }
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
      const ev = await addInviteToPigmentCalendarCommand(input);
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
      showToast(`Added "${ev.title}" to Pigment Calendar`);
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

  async function saveDiscordWebhook(url: string | null): Promise<void> {
    const status = await setDiscordWebhookCommand(url);
    setDiscordWebhookConfigured(status.configured);
    showToast(
      status.configured
        ? "Discord webhook saved to OS keychain"
        : "Discord webhook cleared from OS keychain",
    );
  }

  async function copyDiscordRecap(eventId: CalendarEventId): Promise<void> {
    try {
      const md = await formatMeetingRecapCommand(eventId);
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(md);
        showToast("Discord recap copied to clipboard");
      } else {
        showToast("Recap formatted (clipboard unavailable in this browser)");
      }
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function postDiscordRecap(eventId: CalendarEventId): Promise<void> {
    try {
      await postMeetingRecapToDiscordCommand(eventId, null);
      showToast("Meeting recap posted to Discord");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  function openActionsSection(): void {
    setActiveSection("actions");
    setSelectedId(null);
    setThread(null);
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    void reloadStaff().catch(() => undefined);
  }

  function openNewActionItemModal(seed?: ActionItemDraftSeed): void {
    setActionItemModalSeed(seed ?? {});
  }

  function closeNewActionItemModal(): void {
    setActionItemModalSeed(null);
  }

  function openWorkbookImport(target: WorkbookImportTarget = "auto"): void {
    setWorkbookImportTarget(target);
    setWorkbookImportOpen(true);
  }

  function closeWorkbookImport(): void {
    setWorkbookImportOpen(false);
  }

  async function persistActionItem(input: SaveActionItemInput): Promise<ActionItemView> {
    const saved = await saveActionItemCommand(input);
    await reloadStaff();
    showToast(input.id ? "Action item updated" : "Action item created");
    return saved;
  }

  async function removeActionItem(id: ActionItemId): Promise<void> {
    try {
      await deleteActionItemCommand(id);
      const next = await listActionItems();
      setActionItems(next);
      showToast("Action item deleted");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function jumpToCalendarEvent(eventId: CalendarEventId): Promise<void> {
    setActiveSection("calendar");
    setSelectedId(null);
    setThread(null);
    setSelectedSharedId(null);
    setActiveSharedThread(null);
    const match = calendarOccurrences.find((o) => o.event.id === eventId)?.event;
    if (match) {
      await selectCalendarEvent(match);
      return;
    }
    try {
      const nowMs = Date.now();
      const occs = await listCalendarEvents({
        rangeStart: nowMs - 365 * 86_400_000,
        rangeEnd: nowMs + 730 * 86_400_000,
      });
      setCalendarOccurrences(occs);
      const found = occs.find((o) => o.event.id === eventId)?.event;
      if (found) {
        await selectCalendarEvent(found);
      } else {
        showToast("Linked calendar event was deleted");
      }
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function importWorkbook(input: ImportWorkbookInput): Promise<WorkbookImportReport> {
    const report = await importOperationsWorkbookCommand(input);
    if (!input.dryRun) {
      await reloadStaff();
      const parts: string[] = [];
      const tasks = report.actionItemsCreated + report.actionItemsUpdated;
      const contacts = report.contactsCreated + report.contactsUpdated;
      if (tasks > 0) parts.push(`${tasks} action item${tasks === 1 ? "" : "s"}`);
      if (contacts > 0) parts.push(`${contacts} contact${contacts === 1 ? "" : "s"}`);
      if (report.calendarEventsCreated > 0) {
        parts.push(
          `${report.calendarEventsCreated} calendar event${report.calendarEventsCreated === 1 ? "" : "s"}`,
        );
      }
      showToast(parts.length > 0 ? `Imported ${parts.join(", ")}` : "Workbook import complete");
    }
    return report;
  }

  function triggerCsvDownload(filename: string, csv: string): void {
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }

  async function downloadActionItemsCsv(): Promise<void> {
    try {
      const csv = await exportActionItemsCsv();
      const stamp = new Date().toISOString().slice(0, 10);
      triggerCsvDownload(`pigment-action-items-${stamp}.csv`, csv);
      showToast("Exported Action Items CSV");
    } catch (err) {
      showToast(friendlyError(err));
    }
  }

  async function downloadShowContactsCsv(): Promise<void> {
    try {
      const csv = await exportShowContactsCsv();
      const stamp = new Date().toISOString().slice(0, 10);
      triggerCsvDownload(`pigment-show-contacts-${stamp}.csv`, csv);
      showToast("Exported Show Contacts CSV");
    } catch (err) {
      showToast(friendlyError(err));
    }
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
    shareModalOpen,
    contactsOpen,
    pinnedLinksOpen,
    popover,
    replyText,
    replyNonce,
    expanded,
    remoteContent,
    syncing,
    composeInsert,
    activeSection,
    staffWorkspace,
    staffDirectory,
    sharedThreads,
    sharedTemplates,
    showContacts,
    pinnedLinks,
    selectedSharedId,
    activeSharedThread,
    calendarOccurrences,
    icsOverlays,
    overlayBusyBlocks,
    selectedCalendarEvent,
    activeMeetingDoc,
    discordWebhookConfigured,
    actionItems,
    actionItemModalSeed,
    workbookImportOpen,
    workbookImportTarget,
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
    addAccount,
    oauthSignInAccount,
    updateAccount,
    reauthAccount,
    removeAccount,
    openSharedSection,
    openSharedThread,
    openShareModal: () => setShareModalOpen(true),
    closeShareModal: () => setShareModalOpen(false),
    openContacts: () => setContactsOpen(true),
    closeContacts: () => setContactsOpen(false),
    openPinnedLinks: () => setPinnedLinksOpen(true),
    closePinnedLinks: () => setPinnedLinksOpen(false),
    connectStaffWorkspace,
    disconnectStaffWorkspace,
    syncStaffWorkspace,
    shareThread,
    updateHandoff,
    postSharedComment,
    mutateSharedDraft,
    loadApprovedDraftToReply,
    insertTextSnippet,
    persistSharedTemplate,
    removeSharedTemplate,
    persistShowContact,
    removeShowContact,
    persistPinnedLink,
    removePinnedLink,
    launchPinnedLink,
    openCalendarSection,
    loadCalendarRange,
    selectCalendarEvent,
    persistCalendarEvent,
    quickAddEvent,
    removeCalendarEvent,
    mutateMeetingDoc,
    promoteAgendaToAction,
    setActionItemStatus,
    rsvpInvite,
    addInviteToCalendar,
    persistIcsOverlay,
    removeIcsOverlay,
    syncIcsOverlay,
    saveDiscordWebhook,
    copyDiscordRecap,
    postDiscordRecap,
    openActionsSection,
    openNewActionItemModal,
    closeNewActionItemModal,
    openWorkbookImport,
    closeWorkbookImport,
    persistActionItem,
    removeActionItem,
    jumpToCalendarEvent,
    importWorkbook,
    downloadActionItemsCsv,
    downloadShowContactsCsv,
  };
}
