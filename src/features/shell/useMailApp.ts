import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  actOnThreads,
  addAccount as addAccountCommand,
  cancelSend,
  createDraft,
  deleteTemplate,
  dismissFollowUp,
  listAccounts,
  listJobs,
  listTemplates,
  listThreads,
  mailboxCounts,
  oauthSignInAccount as oauthSignInAccountCommand,
  openThread as openThreadCommand,
  reauthOAuthAccount as reauthOAuthAccountCommand,
  removeAccount as removeAccountCommand,
  saveTemplate as saveTemplateCommand,
  savedOAuthClients as savedOAuthClientsCommand,
  seedDemoIfEmpty,
  sendDraft,
  setFollowUp,
  snoozeThread as snoozeThreadCommand,
  syncAccount,
  tickScheduler,
  unsnoozeThread,
  updateAccount as updateAccountCommand,
} from "../../ipc/core";
import type {
  AccountId,
  AccountView,
  CreateDraftInput,
  InboxCategory,
  JobId,
  JobView,
  MailboxCounts,
  MailboxView,
  MessageId,
  NewAccountInput,
  OAuthSignInInput,
  RemoteContent,
  SaveTemplateInput,
  SavedOAuthClients,
  Scope,
  SendMode,
  TemplateId,
  TemplateView,
  ThreadId,
  ThreadSummary,
  ThreadView,
  UnixMs,
  UpdateAccountInput,
} from "../../types/mail";
import { formatDue } from "../ui/format";

export type Theme = "dark" | "light";
export type Layout = "split" | "classic";
export type Stage = "list" | "thread";
export type PopoverMode = "snooze" | "followUp" | "sendLater";

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

const THEME_KEY = "galapaghost.theme";
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
  stage: Stage;
  toast: ToastData | null;
  commandOpen: boolean;
  composeOpen: boolean;
  addAccountOpen: boolean;
  templatesOpen: boolean;
  popover: PopoverState | null;
  replyText: string;
  replyNonce: number;
  expanded: ReadonlySet<MessageId>;
  remoteContent: RemoteContent;
  syncing: boolean;
  composeInsert: { nonce: number; body: string } | null;

  toggleTheme(): void;
  toggleLayout(): void;
  backToList(): void;
  selectSpace(id: AccountId | null): void;
  selectView(view: MailboxView): void;
  selectCategory(category: InboxCategory): void;
  openThread(id: ThreadId): void;
  archiveThread(id: ThreadId): void;
  trashThread(id: ThreadId): void;
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
  const [layout, setLayout] = useState<Layout>("split");
  const [stage, setStage] = useState<Stage>("list");
  const [toast, setToast] = useState<ToastData | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);
  const [composeOpen, setComposeOpen] = useState(false);
  const [addAccountOpen, setAddAccountOpen] = useState(false);
  const [templatesOpen, setTemplatesOpen] = useState(false);
  const [popover, setPopover] = useState<PopoverState | null>(null);
  const [replyText, setReplyText] = useState("");
  const [replyNonce, setReplyNonce] = useState(0);
  const [expanded, setExpanded] = useState<ReadonlySet<MessageId>>(() => new Set());
  const [syncing, setSyncing] = useState(false);
  const [composeInsert, setComposeInsert] = useState<{ nonce: number; body: string } | null>(null);

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

  const reloadMeta = useCallback(async () => {
    const [nextAccounts, nextJobs, nextTemplates, nextClients] = await Promise.all([
      listAccounts(),
      listJobs(),
      listTemplates(),
      savedOAuthClientsCommand(),
    ]);
    setAccounts(nextAccounts);
    setJobs(nextJobs);
    setTemplates(nextTemplates);
    setSavedOAuthClients(nextClients);
    jobsSignature.current = nextJobs.map((j) => `${j.id}:${j.state}`).join("|");
  }, []);

  // Theme persistence + accent wiring.
  useEffect(() => {
    themeRef.current = theme;
    try {
      window.localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Preview environments may deny storage; theme still applies for the session.
    }
    document.documentElement.dataset.theme = theme;
  }, [theme]);

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

  // Boot: seed the preview store, then load meta. The scope/view effect below
  // performs the first counts + threads load on mount.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        await seedDemoIfEmpty();
        if (!cancelled) await reloadMeta();
      } catch (err) {
        if (!cancelled) setError(friendlyError(err));
      } finally {
        if (!cancelled) setBooting(false);
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

  function openThread(id: ThreadId): void {
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
    setScopeAccount(id);
  }

  function selectView(next: MailboxView): void {
    setView(next);
  }

  function selectCategory(category: InboxCategory): void {
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
      showToast("Moved to trash");
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

  function insertTemplate(template: TemplateView): void {
    if (composeOpen) {
      insertSeq.current += 1;
      setComposeInsert({ nonce: insertSeq.current, body: template.bodyText });
      return;
    }
    if (selectedId !== null) {
      setReplyText((prev) => (prev ? `${prev}\n\n${template.bodyText}` : template.bodyText));
      setReplyNonce((n) => n + 1);
      return;
    }
    showToast("Open a thread or start a compose to insert a snippet");
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
    if (commandOpen || composeOpen || addAccountOpen || templatesOpen || popover !== null) return;

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

  return {
    booting,
    error,
    accounts,
    savedOAuthClients,
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
    stage,
    toast,
    commandOpen,
    composeOpen,
    addAccountOpen,
    templatesOpen,
    popover,
    replyText,
    replyNonce,
    expanded,
    remoteContent,
    syncing,
    composeInsert,
    toggleTheme: () => setTheme((prev) => (prev === "dark" ? "light" : "dark")),
    toggleLayout: () => setLayout((prev) => (prev === "split" ? "classic" : "split")),
    backToList,
    selectSpace,
    selectView,
    selectCategory,
    openThread,
    archiveThread,
    trashThread,
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
  };
}
