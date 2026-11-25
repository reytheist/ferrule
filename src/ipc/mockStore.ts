// Stateful in-memory backend for the Vite browser preview. Every function here
// mirrors one Tauri command from src/ipc/core.ts so the full UI stays
// interactive without the Rust mail-core.

import {
  newAccountId,
  newActionItemId,
  newAttachmentId,
  newCalendarEventId,
  newCommentId,
  newContactId,
  newDraftId,
  newIcsOverlayId,
  newIdentityId,
  newJobId,
  newLabelId,
  newMessageId,
  newPinnedLinkId,
  newSharedTemplateId,
  newSharedThreadId,
  newStaffMemberId,
  newTemplateId,
  newThreadId,
} from "../types/ids";
import type {
  AccountId,
  AccountView,
  ActionItemId,
  ActionItemStatus,
  ActionItemView,
  ActionOutcome,
  AddInviteToCalendarInput,
  AddSharedCommentInput,
  AppSettings,
  AttachmentMeta,
  AvailabilityQuery,
  AvailabilitySlot,
  CalendarEventId,
  CalendarEventKind,
  CalendarEventView,
  CalendarInviteView,
  CalendarOccurrence,
  CalendarRangeQuery,
  ConnectStaffWorkspaceInput,
  ContactCategory,
  ContactId,
  CreateDraftInput,
  DiscordWebhookStatus,
  DraftId,
  DraftView,
  IcsOverlayId,
  IcsOverlayView,
  IdentityView,
  ImportWorkbookInput,
  InboxCategory,
  JobId,
  JobView,
  LabelBadge,
  MailAction,
  MailboxCounts,
  MailboxView,
  MeetingDocView,
  MessageId,
  MessageView,
  NewAccountInput,
  OAuthSignInInput,
  OverlayBusyBlock,
  PinnedLinkId,
  PinnedLinkView,
  PrivacyReport,
  PromoteActionItemInput,
  ProviderKind,
  RecurrenceRule,
  RemoteContent,
  RemoteImagesStatus,
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
  SpaceColor,
  SpaceKind,
  StaffMemberView,
  StaffWorkspaceView,
  SyncState,
  TemplateId,
  TemplateView,
  ThreadId,
  ThreadPage,
  ThreadQuery,
  ThreadSummary,
  ThreadView,
  TrackerHit,
  TrackerKind,
  UndoOutcome,
  UnixMs,
  UnsubscribeInfo,
  UpdateAccountInput,
  UpdateHandoffInput,
  UpdateMeetingDocInput,
  UpdateSharedDraftInput,
  WorkbookImportReport,
  WorkbookPreviewRow,
} from "../types/mail";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function now(): UnixMs {
  return Date.now();
}

// ── Stored records ──────────────────────────────────────────────────────────

interface StoredAccount {
  id: AccountId;
  displayName: string;
  provider: ProviderKind;
  color: SpaceColor;
  spaceKind: SpaceKind;
  email: string;
  identities: IdentityView[];
  sync: SyncState;
  position: number;
}

interface StoredMessage {
  id: MessageId;
  fromName: string | null;
  fromEmail: string;
  to: string[];
  cc: string[];
  date: UnixMs;
  snippet: string;
  rawHtml: string;
  bodyText: string;
  read: boolean;
  attachments: AttachmentMeta[];
  unsubscribe: UnsubscribeInfo | null;
}

interface StoredThread {
  id: ThreadId;
  accountId: AccountId;
  subject: string;
  senderName: string;
  senderEmail: string;
  category: InboxCategory;
  starred: boolean;
  snoozedUntil: UnixMs | null;
  followUpDue: UnixMs | null;
  labels: LabelBadge[];
  archived: boolean;
  trashed: boolean;
  spam?: boolean;
  sentByUser: boolean;
  pendingSendJob: JobId | null;
  messages: StoredMessage[];
}

interface SendPayload {
  subject: string;
  to: string[];
  cc: string[];
  bodyText: string;
  fromIdentity: IdentityView;
}

interface StoredJob {
  id: JobId;
  kind: "send" | "followUp";
  accountId: AccountId;
  threadId: ThreadId | null;
  dueAt: UnixMs;
  state: string;
  summary: string;
  draftId: DraftId | null;
  payload: SendPayload | null;
}

interface MockState {
  accounts: StoredAccount[];
  threads: StoredThread[];
  drafts: Map<DraftId, DraftView>;
  jobs: StoredJob[];
  templates: TemplateView[];
  settings: AppSettings;
  staffWorkspace: StaffWorkspaceView;
  staffDirectory: StaffMemberView[];
  sharedThreads: SharedThreadView[];
  sharedTemplates: SharedTemplateView[];
  showContacts: ShowContactView[];
  pinnedLinks: PinnedLinkView[];
}

function emptyWorkspace(): StaffWorkspaceView {
  return {
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
}

function emptyState(): MockState {
  return {
    accounts: [],
    threads: [],
    drafts: new Map(),
    jobs: [],
    templates: [],
    settings: {
      developerToolsEnabled: false,
      onboardingCompleted: false,
      tourCompleted: false,
    },
    staffWorkspace: emptyWorkspace(),
    staffDirectory: [],
    sharedThreads: [],
    sharedTemplates: [],
    showContacts: [],
    pinnedLinks: [],
  };
}

const state: MockState = emptyState();

// ── Projections ─────────────────────────────────────────────────────────────

function accountById(id: AccountId): StoredAccount | undefined {
  return state.accounts.find((a) => a.id === id);
}

function threadById(id: ThreadId): StoredThread | undefined {
  return state.threads.find((t) => t.id === id);
}

function lastMessage(thread: StoredThread): StoredMessage | undefined {
  return thread.messages[thread.messages.length - 1];
}

function threadLastAt(thread: StoredThread): UnixMs {
  const last = lastMessage(thread);
  return last ? last.date : 0;
}

function unreadInThread(thread: StoredThread): number {
  return thread.messages.reduce((sum, m) => sum + (m.read ? 0 : 1), 0);
}

function unreadForAccount(id: AccountId): number {
  return state.threads
    .filter(
      (t) =>
        t.accountId === id &&
        !t.archived &&
        !t.trashed &&
        !t.sentByUser &&
        t.snoozedUntil === null,
    )
    .reduce((sum, t) => sum + unreadInThread(t), 0);
}

function accountView(account: StoredAccount): AccountView {
  return {
    id: account.id,
    displayName: account.displayName,
    provider: account.provider,
    color: account.color,
    spaceKind: account.spaceKind,
    email: account.email,
    identities: account.identities,
    sync: account.sync,
    unread: unreadForAccount(account.id),
    position: account.position,
  };
}

function trackersOf(thread: StoredThread): number {
  return thread.messages.reduce((sum, m) => sum + countTrackers(m.rawHtml), 0);
}

function threadSummary(thread: StoredThread): ThreadSummary | null {
  const account = accountById(thread.accountId);
  if (!account) return null;
  const last = lastMessage(thread);
  return {
    id: thread.id,
    accountId: account.id,
    accountLabel: account.displayName,
    accountColor: account.color,
    accountEmail: account.email,
    subject: thread.subject,
    snippet: last ? last.snippet : "",
    senderName: thread.senderName,
    senderEmail: thread.senderEmail,
    lastAt: threadLastAt(thread),
    unreadCount: unreadInThread(thread),
    messageCount: thread.messages.length,
    hasAttachments: thread.messages.some((m) => m.attachments.length > 0),
    starred: thread.starred,
    category: thread.category,
    snoozedUntil: thread.snoozedUntil,
    followUpDue: thread.followUpDue,
    trackersBlocked: trackersOf(thread),
    labels: thread.labels,
  };
}

function jobView(job: StoredJob): JobView {
  return {
    id: job.id,
    kind: job.kind,
    accountId: job.accountId,
    threadId: job.threadId,
    dueAt: job.dueAt,
    state: job.state,
    summary: job.summary,
  };
}

// ── HTML sanitizing (spy-pixel stripping + CSP srcdoc) ──────────────────────

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

function attrOf(tag: string, name: string): string | null {
  const match = new RegExp(`\\b${name}\\s*=\\s*"([^"]*)"`, "i").exec(tag);
  return match ? (match[1] ?? null) : null;
}

function isTrackerHost(host: string): boolean {
  return /(track|pixel|beacon|stats?|px\.|-px)/i.test(host);
}

interface SanitizeResult {
  html: string;
  trackers: TrackerHit[];
  remoteImages: number;
}

function sanitize(rawHtml: string, remoteContent: RemoteContent): SanitizeResult {
  const trackers: TrackerHit[] = [];
  let remoteImages = 0;

  let html = rawHtml.replace(
    /url\(\s*(['"]?)(https?:\/\/[^)'"]+)\1\s*\)/gi,
    (_full, _quote: string, url: string) => {
      trackers.push({ host: hostOf(url), kind: "cssBeacon" });
      return "none";
    },
  );

  html = html.replace(/<img\b[^>]*>/gi, (tag) => {
    const src = attrOf(tag, "src");
    if (!src || !/^https?:/i.test(src)) return tag;
    const width = attrOf(tag, "width");
    const height = attrOf(tag, "height");
    const style = attrOf(tag, "style") ?? "";
    const hidden = /display\s*:\s*none|visibility\s*:\s*hidden/i.test(style);
    const onePx = width === "1" || height === "1";
    const host = hostOf(src);
    if (onePx || hidden || isTrackerHost(host)) {
      const kind: TrackerKind = onePx ? "pixel1x1" : hidden ? "hiddenImage" : "knownDomain";
      trackers.push({ host, kind });
      return "";
    }
    remoteImages += 1;
    return remoteContent === "blockAll" ? "" : tag;
  });

  return { html, trackers, remoteImages };
}

function countTrackers(rawHtml: string): number {
  return sanitize(rawHtml, "blockAll").trackers.length;
}

function srcdocFor(sanitized: SanitizeResult, remoteContent: RemoteContent): string {
  const theme = document.documentElement.dataset.theme === "light" ? "light" : "dark";
  const ink = theme === "light" ? "#121621" : "#EDF0F8";
  const bg = theme === "light" ? "#FFFFFF" : "#161923";
  const imgPolicy = remoteContent === "blockAll" ? "'none'" : "http: https: data:";
  const csp = `default-src 'none'; img-src ${imgPolicy}; style-src 'unsafe-inline'; font-src 'none'`;
  return (
    `<!DOCTYPE html><html><head><meta charset="utf-8">` +
    `<meta http-equiv="Content-Security-Policy" content="${csp}">` +
    `<style>` +
    `body{margin:12px;background:${bg};color:${ink};` +
    `font-family:Inter,-apple-system,sans-serif;font-size:13px;line-height:1.6}` +
    `img{max-width:100%;height:auto}a{color:#4F86FF}` +
    `</style></head><body>${sanitized.html}</body></html>`
  );
}

function privacyReport(sanitized: SanitizeResult, remoteContent: RemoteContent): PrivacyReport {
  const remoteImages: RemoteImagesStatus =
    remoteContent === "allowImagesStripTrackers"
      ? { kind: "allowed" }
      : { kind: "blocked", count: sanitized.remoteImages + sanitized.trackers.length };
  return { remoteImages, trackers: sanitized.trackers };
}

function messageView(message: StoredMessage, remoteContent: RemoteContent): MessageView {
  const sanitized = sanitize(message.rawHtml, remoteContent);
  return {
    id: message.id,
    fromName: message.fromName,
    fromEmail: message.fromEmail,
    to: message.to,
    cc: message.cc,
    date: message.date,
    snippet: message.snippet,
    bodySrcdoc: srcdocFor(sanitized, remoteContent),
    bodyText: message.bodyText,
    privacy: privacyReport(sanitized, remoteContent),
    attachments: message.attachments,
    unsubscribe: message.unsubscribe,
    invites: mockInvitesForMessage(message),
  };
}

// ── Filtering ───────────────────────────────────────────────────────────────

function inScope(thread: StoredThread, scope: Scope): boolean {
  return scope.kind === "unified" || scope.accountId === thread.accountId;
}

function isScheduled(thread: StoredThread): boolean {
  if (thread.pendingSendJob === null) return false;
  return state.jobs.some(
    (j) => j.id === thread.pendingSendJob && j.kind === "send" && j.state === "queued",
  );
}

function matchesView(thread: StoredThread, view: MailboxView): boolean {
  switch (view.kind) {
    case "inbox":
      if (thread.archived || thread.trashed || thread.sentByUser) return false;
      if (thread.snoozedUntil !== null) return false;
      if (isScheduled(thread)) return false;
      return view.category === "all" || thread.category === view.category;
    case "snoozed":
      return !thread.trashed && !thread.sentByUser && thread.snoozedUntil !== null;
    case "followUps":
      return !thread.trashed && thread.followUpDue !== null;
    case "scheduled":
      return !thread.trashed && isScheduled(thread);
    case "sent":
      return thread.sentByUser && !thread.trashed && !isScheduled(thread);
    case "drafts":
      return false;
    case "starred":
      return thread.starred && !thread.trashed && !thread.spam;
    case "archive":
      return thread.archived && !thread.trashed && !thread.spam;
    case "spam":
      return Boolean(thread.spam) && !thread.trashed;
    case "trash":
      return thread.trashed;
    case "label":
      return thread.labels.some((l) => l.id === view.labelId);
    case "search": {
      const needle = view.text.trim().toLowerCase();
      if (!needle) return true;
      const haystack = [
        thread.subject,
        thread.senderName,
        thread.senderEmail,
        ...thread.messages.map((m) => m.bodyText),
      ]
        .join("\n")
        .toLowerCase();
      return haystack.includes(needle);
    }
    default: {
      const exhaustive: never = view;
      throw new Error(`Unexpected view: ${JSON.stringify(exhaustive)}`);
    }
  }
}

// ── Seeding ─────────────────────────────────────────────────────────────────

function makeAccount(
  displayName: string,
  provider: ProviderKind,
  color: SpaceColor,
  email: string,
  aliases: string[],
  position: number,
  spaceKind: SpaceKind = "personal",
): StoredAccount {
  const primary: IdentityView = {
    id: newIdentityId(),
    address: email,
    displayName,
    isPrimary: true,
  };
  const rest: IdentityView[] = aliases.map((address) => ({
    id: newIdentityId(),
    address,
    displayName,
    isPrimary: false,
  }));
  return {
    id: newAccountId(),
    displayName,
    provider,
    color,
    spaceKind,
    email,
    identities: [primary, ...rest],
    sync: { kind: "idle", lastOk: now() - 4 * MINUTE },
    position,
  };
}

function trackerTag(url: string): string {
  return `<img src="${url}" width="1" height="1" alt="">`;
}

interface MessageSeed {
  fromName: string | null;
  fromEmail: string;
  to: string[];
  date: UnixMs;
  read: boolean;
  paragraphs: string[];
  trackers?: string[];
  images?: string[];
  attachments?: AttachmentMeta[];
  unsubscribe?: UnsubscribeInfo | null;
}

function makeMessage(seed: MessageSeed): StoredMessage {
  const extras = [
    ...(seed.trackers ?? []).map(trackerTag),
    ...(seed.images ?? []).map((u) => `<img src="${u}" alt="">`),
  ].join("");
  const paragraphs = seed.paragraphs.map((p) => `<p>${escapeHtml(p)}</p>`).join("");
  const rawHtml = paragraphs + extras;
  const bodyText = seed.paragraphs.join("\n\n");
  return {
    id: newMessageId(),
    fromName: seed.fromName,
    fromEmail: seed.fromEmail,
    to: seed.to,
    cc: [],
    date: seed.date,
    snippet: bodyText.slice(0, 140),
    rawHtml,
    bodyText,
    read: seed.read,
    attachments: seed.attachments ?? [],
    unsubscribe: seed.unsubscribe ?? null,
  };
}

interface ThreadSeed {
  accountId: AccountId;
  subject: string;
  senderName: string;
  senderEmail: string;
  category: InboxCategory;
  starred?: boolean;
  snoozedUntil?: UnixMs | null;
  followUpDue?: UnixMs | null;
  labels?: LabelBadge[];
  archived?: boolean;
  trashed?: boolean;
  sentByUser?: boolean;
  messages: MessageSeed[];
}

function makeThread(seed: ThreadSeed): StoredThread {
  return {
    id: newThreadId(),
    accountId: seed.accountId,
    subject: seed.subject,
    senderName: seed.senderName,
    senderEmail: seed.senderEmail,
    category: seed.category,
    starred: seed.starred ?? false,
    snoozedUntil: seed.snoozedUntil ?? null,
    followUpDue: seed.followUpDue ?? null,
    labels: seed.labels ?? [],
    archived: seed.archived ?? false,
    trashed: seed.trashed ?? false,
    sentByUser: seed.sentByUser ?? false,
    pendingSendJob: null,
    messages: seed.messages.map(makeMessage),
  };
}

function seedDemo(): void {
  const t = now();
  const studio = makeAccount(
    "Studio Gmail",
    "gmail",
    "azure",
    "studio@gmail.com",
    ["studio@ferrule.dev"],
    0,
  );
  const personal = makeAccount("Personal Gmail", "gmail", "berry", "rey@gmail.com", [], 1);
  const outlook = makeAccount("Outlook Work", "outlook", "teal", "rey.h@outlook.com", [], 2);
  const imap = makeAccount("Domain IMAP", "imap", "amber", "hello@domain.org", [], 3);
  state.accounts = [studio, personal, outlook, imap];

  const vipLabel: LabelBadge = { id: newLabelId(), name: "VIP" };
  const printLabel: LabelBadge = { id: newLabelId(), name: "Print" };

  const threads: StoredThread[] = [
    makeThread({
      accountId: studio.id,
      subject: "Updated floor plan & AV load-in schedule for July",
      senderName: "Hannah Cho",
      senderEmail: "hannah@northlight.events",
      category: "primary",
      followUpDue: t + 2 * DAY,
      labels: [vipLabel],
      messages: [
        {
          fromName: "Hannah Cho",
          fromEmail: "hannah@northlight.events",
          to: [studio.email],
          date: t - 2 * DAY - 3 * HOUR,
          read: true,
          paragraphs: [
            "Rey — kicking off the July show logistics. Ballroom B is confirmed for load-in starting the 14th.",
            "I'll circulate the first floor plan draft once AV signs off on the truss positions.",
          ],
        },
        {
          fromName: null,
          fromEmail: studio.email,
          to: ["hannah@northlight.events"],
          date: t - DAY - 5 * HOUR,
          read: true,
          paragraphs: [
            "Can we widen the east aisle to 10 ft and add a second lighting truss? It keeps the judge tables clear of the demo benches.",
          ],
        },
        {
          fromName: "Hannah Cho",
          fromEmail: "hannah@northlight.events",
          to: [studio.email],
          date: t - 42 * MINUTE,
          read: false,
          paragraphs: [
            "Revised ballroom layout attached — the east aisle is now 10 ft and the second truss is priced in.",
            "Can you take a quick look at rows B–E and confirm the judge table count before Friday at 3 PM so we can lock the rental order?",
          ],
          trackers: [
            "https://track.northlight-events.net/open/p.gif",
            "https://beacon.mail-stats.io/t/open.gif",
          ],
          images: ["https://cdn.northlight-events.net/floorplan-rev3.png"],
          attachments: [
            {
              id: newAttachmentId(),
              filename: "ballroom-floorplan-rev3.pdf",
              mime: "application/pdf",
              size: 1_842_000,
              cached: true,
            },
          ],
        },
      ],
    }),
    makeThread({
      accountId: outlook.id,
      subject: "Re: Shipping quote for acrylic display risers",
      senderName: "Marcus Vance",
      senderEmail: "marcus@vancefreight.com",
      category: "primary",
      snoozedUntil: t + 18 * HOUR,
      messages: [
        {
          fromName: null,
          fromEmail: outlook.email,
          to: ["marcus@vancefreight.com"],
          date: t - DAY - 7 * HOUR,
          read: true,
          paragraphs: [
            "Checking if pallet freight beats UPS ground for 40 acrylic risers going to the convention center.",
          ],
        },
        {
          fromName: "Marcus Vance",
          fromEmail: "marcus@vancefreight.com",
          to: [outlook.email],
          date: t - 26 * HOUR,
          read: false,
          paragraphs: [
            "Freight came back $180 lower if we consolidate into two half-pallets instead of boxes.",
            "Consolidating also cuts transit damage risk. Reply \"ship pallets\" and I'll book the Monday pickup window.",
          ],
          trackers: ["https://track.vancefreight.com/px/delivery.gif"],
        },
      ],
    }),
    makeThread({
      accountId: imap.id,
      subject: "Sponsor badge artwork — final vector files",
      senderName: "Nora Lindqvist",
      senderEmail: "nora@formverket.se",
      category: "primary",
      labels: [printLabel],
      messages: [
        {
          fromName: "Nora Lindqvist",
          fromEmail: "nora@formverket.se",
          to: [imap.email],
          date: t - 3 * DAY,
          read: true,
          paragraphs: [
            "Hi! First pass at the lanyard badges is done — CMYK print-ready PDFs with 0.125\" bleed.",
          ],
        },
        {
          fromName: "Nora Lindqvist",
          fromEmail: "nora@formverket.se",
          to: [imap.email],
          date: t - 2 * DAY - 4 * HOUR,
          read: true,
          paragraphs: [
            "Uploaded the final vector files: CMYK print-ready PDFs and dark-background SVG lockups.",
            "Let me know if your printer prefers Pantone spot callouts instead of CMYK.",
          ],
          attachments: [
            {
              id: newAttachmentId(),
              filename: "sponsor-badges-final.zip",
              mime: "application/zip",
              size: 6_420_000,
              cached: false,
            },
          ],
        },
      ],
    }),
    makeThread({
      accountId: studio.id,
      subject: "Workshop registration cap & waitlist behavior",
      senderName: "Devon Park",
      senderEmail: "devon@ticketforge.io",
      category: "primary",
      messages: [
        {
          fromName: "Devon Park",
          fromEmail: "devon@ticketforge.io",
          to: [studio.email],
          date: t - 6 * HOUR,
          read: false,
          paragraphs: [
            "Quick question before we open tickets: when an attendee cancels a workshop seat, should the next waitlist person get a 24-hour claim link automatically?",
            "Easy either way — just say the word.",
          ],
          trackers: [
            "https://track.ticketforge.io/mail/imp.gif",
            "https://px.ticketforge.io/px.gif",
          ],
          unsubscribe: { oneClick: true, target: "https://ticketforge.io/unsubscribe" },
        },
      ],
    }),
    makeThread({
      accountId: personal.id,
      subject: "Receipt: Annual DNS & domain renewal (#INV-8841)",
      senderName: "Domain Registrar",
      senderEmail: "billing@domainregistrar.example",
      category: "receipts",
      messages: [
        {
          fromName: "Domain Registrar",
          fromEmail: "billing@domainregistrar.example",
          to: [personal.email],
          date: t - 4 * DAY,
          read: true,
          paragraphs: [
            "Thanks for renewing your domains. Your invoice #INV-8841 for $24.00 is paid and indexed locally for instant offline search.",
            "Auto-renewal remains enabled for November 2027.",
          ],
          trackers: [
            "https://stats.domainregistrar.example/px/open.gif",
            "https://track.domainregistrar.example/p.gif",
          ],
        },
      ],
    }),
    makeThread({
      accountId: personal.id,
      subject: "Issue #318: Calm desktop tools & local-first software",
      senderName: "Dense Discovery",
      senderEmail: "hello@densediscovery.example",
      category: "newsletters",
      messages: [
        {
          fromName: "Dense Discovery",
          fromEmail: "hello@densediscovery.example",
          to: [personal.email],
          date: t - 5 * DAY,
          read: false,
          paragraphs: [
            "This week we look at the wave of fast, local-first desktop utilities ditching 400 MB browser runtimes in favor of tiny native cores and local SQLite indexes.",
            "Plus: three typographic tricks for dense interfaces that still feel peaceful.",
          ],
          trackers: [
            "https://track.densediscovery.example/beacon.gif",
            "https://stats.densediscovery.example/pixel.gif",
            "https://beacon.densediscovery.example/o.gif",
          ],
          images: ["https://cdn.densediscovery.example/issues/318-cover.png"],
          unsubscribe: { oneClick: true, target: "https://densediscovery.example/unsubscribe" },
        },
      ],
    }),
    makeThread({
      accountId: personal.id,
      subject: "Figma — Receipt for Professional plan (#F-2291)",
      senderName: "Figma Billing",
      senderEmail: "receipts@figma.example",
      category: "receipts",
      messages: [
        {
          fromName: "Figma Billing",
          fromEmail: "receipts@figma.example",
          to: [personal.email],
          date: t - 6 * DAY,
          read: true,
          paragraphs: [
            "Your Professional plan renewed for $15.00. Card ending in 4019 was charged on this date.",
          ],
          trackers: ["https://stats.figma.example/px/mail-open.gif"],
        },
      ],
    }),
    makeThread({
      accountId: studio.id,
      subject: "Re: Updated floor plan & AV load-in schedule for July",
      senderName: "You",
      senderEmail: studio.email,
      category: "primary",
      sentByUser: true,
      messages: [
        {
          fromName: null,
          fromEmail: studio.email,
          to: ["hannah@northlight-events"],
          date: t - 3 * HOUR,
          read: true,
          paragraphs: [
            "Truss positions look right. Confirming 6 judge tables for now — I'll lock the count after Friday's walkthrough.",
          ],
        },
      ],
    }),
  ];

  state.threads = threads;

  const followThread = threads[0];
  if (followThread) {
    state.jobs.push({
      id: newJobId(),
      kind: "followUp",
      accountId: studio.id,
      threadId: followThread.id,
      dueAt: t + 2 * DAY,
      state: "scheduled",
      summary: "Follow up with Hannah Cho if no reply",
      draftId: null,
      payload: null,
    });
  }

  const replyTarget = threads[3];
  if (replyTarget) {
    const devon = replyTarget.messages[0];
    const draftId = newDraftId();
    const jobId = newJobId();
    const payload: SendPayload = {
      subject: `Re: ${replyTarget.subject}`,
      to: devon ? [devon.fromEmail] : [],
      cc: [],
      bodyText: "Hold seats manually for v1 — automatic claim links can wait until we see the cancel rate.",
      fromIdentity: studio.identities[0] ?? {
        id: newIdentityId(),
        address: studio.email,
        displayName: studio.displayName,
        isPrimary: true,
      },
    };
    replyTarget.pendingSendJob = jobId;
    state.jobs.push({
      id: jobId,
      kind: "send",
      accountId: studio.id,
      threadId: replyTarget.id,
      dueAt: t + 3 * HOUR,
      state: "queued",
      summary: `Scheduled reply to ${replyTarget.senderName}`,
      draftId,
      payload,
    });
    state.drafts.set(draftId, {
      id: draftId,
      accountId: studio.id,
      spaceColor: studio.color,
      fromIdentity: payload.fromIdentity,
      fromLocked: true,
      replyToThread: replyTarget.id,
      to: payload.to,
      cc: [],
      bcc: [],
      subject: payload.subject,
      bodyText: payload.bodyText,
      rev: 1,
      origin: { kind: "human" },
      state: { kind: "queued", jobId },
    });
  }

  state.templates = [
    {
      id: newTemplateId(),
      name: "Quick status check",
      shortcut: "sc",
      subject: "Quick status check",
      bodyText: "Hi,\n\nQuick check — any updates on this?\n\nThanks,\nRey",
    },
    {
      id: newTemplateId(),
      name: "Shipping confirmed",
      shortcut: "ship",
      subject: "Shipping confirmed",
      bodyText: "Your shipment is booked. Tracking will follow once it leaves the dock.",
    },
    {
      id: newTemplateId(),
      name: "Thanks & next steps",
      shortcut: "tns",
      subject: "Thanks & next steps",
      bodyText:
        "Thanks — next step on my side is X. I'll follow up by Thursday with the outcome.\n\nBest,\nRey",
    },
  ];
}

// ── Command implementations ─────────────────────────────────────────────────

export async function getSettings(): Promise<AppSettings> {
  return { ...state.settings };
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  state.settings = { ...settings };
  return { ...state.settings };
}

export async function seedDemoIfEmpty(): Promise<void> {
  if (state.accounts.length === 0) seedDemo();
}

export async function listAccounts(): Promise<AccountView[]> {
  return state.accounts
    .slice()
    .sort((a, b) => a.position - b.position)
    .map(accountView);
}

export async function addAccount(input: NewAccountInput): Promise<AccountView> {
  const auth = input.auth;
  const address = auth.address;
  const provider: ProviderKind =
    auth.kind === "imapPassword" ? "imap" : auth.kind === "outlookOAuth" ? "outlook" : "gmail";
  const identity: IdentityView = {
    id: newIdentityId(),
    address,
    displayName: input.displayName,
    isPrimary: true,
  };
  const account: StoredAccount = {
    id: newAccountId(),
    displayName: input.displayName,
    provider,
    color: input.color,
    spaceKind: input.spaceKind ?? "personal",
    email: address,
    identities: [identity],
    sync: { kind: "idle", lastOk: null },
    position: state.accounts.length,
  };
  state.accounts.push(account);
  return accountView(account);
}

const PALETTE: SpaceColor[] = ["azure", "berry", "teal", "amber", "violet", "emerald"];

function pickUniqueColor(): SpaceColor {
  let best: SpaceColor = PALETTE[0] ?? "azure";
  let bestCount = Number.POSITIVE_INFINITY;
  for (const candidate of PALETTE) {
    const count = state.accounts.filter((a) => a.color === candidate).length;
    if (count === 0) return candidate;
    if (count < bestCount) {
      best = candidate;
      bestCount = count;
    }
  }
  return best;
}

export async function savedOAuthClients(): Promise<SavedOAuthClients> {
  return {
    gmailClientId: "preview-google-client.apps.googleusercontent.com",
    outlookClientId: null,
    outlookTenant: "common",
  };
}

export async function oauthSignInAccount(input: OAuthSignInInput): Promise<AccountView> {
  const index = state.accounts.length + 1;
  const domain = input.provider === "outlook" ? "outlook.com" : "gmail.com";
  const email = `account${index}@${domain}`;
  const displayName = input.displayName?.trim() || `account${index}`;
  const color = input.color ?? pickUniqueColor();
  const identity: IdentityView = {
    id: newIdentityId(),
    address: email,
    displayName,
    isPrimary: true,
  };
  const account: StoredAccount = {
    id: newAccountId(),
    displayName,
    provider: input.provider,
    color,
    spaceKind: input.spaceKind ?? "personal",
    email,
    identities: [identity],
    sync: { kind: "idle", lastOk: null },
    position: state.accounts.length,
  };
  state.accounts.push(account);
  return accountView(account);
}

export async function updateAccount(input: UpdateAccountInput): Promise<AccountView> {
  const account = accountById(input.accountId);
  if (!account) throw new Error("Account not found");
  const name = input.displayName.trim();
  if (!name) throw new Error("Space name is required");
  account.displayName = name;
  account.color = input.color;
  if (input.spaceKind !== undefined && input.spaceKind !== null) {
    account.spaceKind = input.spaceKind;
  }
  return accountView(account);
}

export async function removeAccount(accountId: AccountId): Promise<void> {
  state.accounts = state.accounts.filter((a) => a.id !== accountId);
  state.threads = state.threads.filter((t) => t.accountId !== accountId);
  state.jobs = state.jobs.filter((j) => j.accountId !== accountId);
}

export async function syncAccount(accountId: AccountId): Promise<AccountView> {
  const account = accountById(accountId);
  if (!account) throw new Error("Account not found");
  account.sync = { kind: "syncing", done: 0, total: null };
  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, 700);
  });
  account.sync = { kind: "idle", lastOk: now() };
  return accountView(account);
}

export async function listThreads(query: ThreadQuery): Promise<ThreadPage> {
  const filtered = state.threads
    .filter((t) => inScope(t, query.scope) && matchesView(t, query.view))
    .sort((a, b) => threadLastAt(b) - threadLastAt(a));
  const items = filtered.slice(query.offset, query.offset + query.limit);
  const summaries = items.map(threadSummary).filter((s): s is ThreadSummary => s !== null);
  return { items: summaries, total: filtered.length };
}

export async function mailboxCounts(scope: Scope): Promise<MailboxCounts> {
  const inScopeThreads = state.threads.filter((t) => inScope(t, scope));
  const visible = (predicate: (t: StoredThread) => boolean): number =>
    inScopeThreads.filter(predicate).length;
  const inboxOf = (category: InboxCategory): number =>
    visible(
      (t) =>
        !t.archived &&
        !t.trashed &&
        !t.sentByUser &&
        t.snoozedUntil === null &&
        !isScheduled(t) &&
        (category === "all" || t.category === category),
    );
  return {
    inbox: inboxOf("all"),
    snoozed: visible((t) => !t.trashed && !t.sentByUser && t.snoozedUntil !== null),
    followUps: visible((t) => !t.trashed && t.followUpDue !== null),
    scheduled: state.jobs.filter(
      (j) => j.kind === "send" && j.state === "queued" && inJobScope(j, scope),
    ).length,
    primary: inboxOf("primary"),
    receipts: inboxOf("receipts"),
    newsletters: inboxOf("newsletters"),
  };
}

function inJobScope(job: StoredJob, scope: Scope): boolean {
  return scope.kind === "unified" || scope.accountId === job.accountId;
}

export async function openThread(
  threadId: ThreadId,
  remoteContent: RemoteContent,
): Promise<ThreadView> {
  const thread = threadById(threadId);
  if (!thread) throw new Error("Thread not found");
  const account = accountById(thread.accountId);
  if (!account) throw new Error("Account not found");
  for (const message of thread.messages) message.read = true;
  return {
    id: thread.id,
    accountId: account.id,
    accountLabel: account.displayName,
    accountColor: account.color,
    accountEmail: account.email,
    subject: thread.subject,
    starred: thread.starred,
    snoozedUntil: thread.snoozedUntil,
    followUpDue: thread.followUpDue,
    messages: thread.messages.map((m) => messageView(m, remoteContent)),
  };
}

export async function actOnThreads(
  action: MailAction,
  threadIds: ThreadId[],
): Promise<ActionOutcome> {
  const targets = threadIds.map(threadById).filter((t): t is StoredThread => t !== undefined);
  for (const thread of targets) {
    switch (action.kind) {
      case "archive":
        thread.archived = true;
        thread.trashed = false;
        thread.spam = false;
        break;
      case "unarchive":
        thread.archived = false;
        thread.trashed = false;
        thread.spam = false;
        break;
      case "trash":
        thread.trashed = true;
        thread.archived = false;
        thread.spam = false;
        break;
      case "reportSpam":
        thread.spam = true;
        thread.archived = false;
        thread.trashed = false;
        break;
      case "markRead":
        for (const message of thread.messages) message.read = action.read;
        break;
      case "star":
        thread.starred = action.on;
        break;
      default: {
        const exhaustive: never = action;
        throw new Error(`Unexpected action: ${JSON.stringify(exhaustive)}`);
      }
    }
  }
  const updatedIds = targets.map((t) => t.id);
  const undoAction: ActionOutcome["undoAction"] =
    action.kind === "archive"
      ? { kind: "unarchive" }
      : action.kind === "markRead"
        ? { kind: "markRead", read: !action.read }
        : action.kind === "star"
          ? { kind: "star", on: !action.on }
          : null;
  return { updatedIds, undoAction };
}

export async function createDraft(input: CreateDraftInput): Promise<DraftView> {
  const account = accountById(input.accountId);
  if (!account) throw new Error("Account not found");
  const identity = account.identities.find((i) => i.isPrimary) ?? account.identities[0];
  if (!identity) throw new Error("Account has no identity");
  const draft: DraftView = {
    id: newDraftId(),
    accountId: account.id,
    spaceColor: account.color,
    fromIdentity: identity,
    fromLocked: input.replyToThread !== null,
    replyToThread: input.replyToThread,
    to: input.to,
    cc: input.cc,
    bcc: input.bcc,
    subject: input.subject,
    bodyText: input.bodyText,
    rev: 0,
    origin: { kind: "human" },
    state: { kind: "editing" },
  };
  state.drafts.set(draft.id, draft);
  return draft;
}

export async function sendDraft(draftId: DraftId, mode: SendMode): Promise<SendReceipt> {
  const draft = state.drafts.get(draftId);
  if (!draft) throw new Error("Draft not found");
  const account = accountById(draft.accountId);
  if (!account) throw new Error("Account not found");

  const dueAt = mode.kind === "undoable" ? now() + mode.undoSecs * 1000 : mode.sendAt;
  const jobId = newJobId();
  const payload: SendPayload = {
    subject: draft.subject,
    to: draft.to,
    cc: draft.cc,
    bodyText: draft.bodyText,
    fromIdentity: draft.fromIdentity,
  };

  const job: StoredJob = {
    id: jobId,
    kind: "send",
    accountId: draft.accountId,
    threadId: draft.replyToThread,
    dueAt,
    state: "queued",
    summary:
      mode.kind === "scheduled"
        ? `Scheduled: ${draft.subject || "(no subject)"}`
        : `Sending: ${draft.subject || "(no subject)"}`,
    draftId: draft.id,
    payload,
  };
  state.jobs.push(job);
  draft.state = { kind: "queued", jobId };

  if (draft.replyToThread === null) {
    // New outgoing message gets a placeholder thread so Scheduled shows it.
    const placeholder = makeThread({
      accountId: draft.accountId,
      subject: draft.subject || "(no subject)",
      senderName: "You",
      senderEmail: payload.fromIdentity.address,
      category: "primary",
      sentByUser: true,
      messages: [],
    });
    placeholder.pendingSendJob = jobId;
    job.threadId = placeholder.id;
    state.threads.push(placeholder);
  } else {
    const thread = threadById(draft.replyToThread);
    if (thread) thread.pendingSendJob = jobId;
  }

  return {
    jobId,
    draftId: draft.id,
    dueAt,
    scheduled: mode.kind === "scheduled",
  };
}

export async function cancelSend(jobId: JobId): Promise<UndoOutcome> {
  const job = state.jobs.find((j) => j.id === jobId);
  if (!job || job.kind !== "send" || job.state !== "queued" || job.dueAt <= now()) {
    return "tooLate";
  }
  job.state = "cancelled";
  if (job.draftId) {
    const draft = state.drafts.get(job.draftId);
    if (draft) draft.state = { kind: "editing" };
  }
  if (job.threadId) {
    const thread = threadById(job.threadId);
    if (thread && thread.messages.length === 0) {
      state.threads = state.threads.filter((t) => t.id !== thread.id);
    } else if (thread) {
      thread.pendingSendJob = null;
    }
  }
  return "cancelled";
}

export async function snoozeThread(threadId: ThreadId, until: UnixMs): Promise<void> {
  const thread = threadById(threadId);
  if (!thread) throw new Error("Thread not found");
  thread.snoozedUntil = until;
}

export async function unsnoozeThread(threadId: ThreadId): Promise<void> {
  const thread = threadById(threadId);
  if (!thread) throw new Error("Thread not found");
  thread.snoozedUntil = null;
}

export async function setFollowUp(threadId: ThreadId, dueAt: UnixMs): Promise<JobId> {
  const thread = threadById(threadId);
  if (!thread) throw new Error("Thread not found");
  const existing = state.jobs.find(
    (j) => j.kind === "followUp" && j.threadId === threadId && j.state !== "dismissed",
  );
  if (existing) {
    existing.dueAt = dueAt;
    existing.state = "scheduled";
    thread.followUpDue = dueAt;
    return existing.id;
  }
  const jobId = newJobId();
  state.jobs.push({
    id: jobId,
    kind: "followUp",
    accountId: thread.accountId,
    threadId,
    dueAt,
    state: "scheduled",
    summary: `Follow up: ${thread.subject}`,
    draftId: null,
    payload: null,
  });
  thread.followUpDue = dueAt;
  return jobId;
}

export async function dismissFollowUp(jobId: JobId): Promise<void> {
  const job = state.jobs.find((j) => j.id === jobId);
  if (!job) return;
  state.jobs = state.jobs.filter((j) => j.id !== jobId);
  if (job.threadId) {
    const thread = threadById(job.threadId);
    if (thread) thread.followUpDue = null;
  }
}

export async function listJobs(): Promise<JobView[]> {
  return state.jobs
    .filter((j) => j.state !== "cancelled" && j.state !== "dismissed")
    .slice()
    .sort((a, b) => a.dueAt - b.dueAt)
    .map(jobView);
}

export async function listTemplates(): Promise<TemplateView[]> {
  return state.templates.slice();
}

export async function saveTemplate(input: SaveTemplateInput): Promise<TemplateView> {
  const existing = input.id ? state.templates.find((t) => t.id === input.id) : undefined;
  if (existing) {
    existing.name = input.name;
    existing.shortcut = input.shortcut;
    existing.subject = input.subject;
    existing.bodyText = input.bodyText;
    return existing;
  }
  const created: TemplateView = {
    id: newTemplateId(),
    name: input.name,
    shortcut: input.shortcut,
    subject: input.subject,
    bodyText: input.bodyText,
  };
  state.templates.push(created);
  return created;
}

export async function deleteTemplate(templateId: TemplateId): Promise<void> {
  state.templates = state.templates.filter((t) => t.id !== templateId);
}

function deliver(job: StoredJob, payload: SendPayload, accountId: AccountId): void {
  const thread = job.threadId ? threadById(job.threadId) : undefined;
  const message = makeMessage({
    fromName: payload.fromIdentity.displayName,
    fromEmail: payload.fromIdentity.address,
    to: payload.to,
    date: now(),
    read: true,
    paragraphs: payload.bodyText.split(/\n{2,}/).filter(Boolean),
  });
  message.snippet = payload.bodyText.slice(0, 140);
  if (thread) {
    thread.messages.push(message);
    thread.senderName = "You";
    thread.senderEmail = payload.fromIdentity.address;
    thread.pendingSendJob = null;
    thread.archived = false;
  } else {
    state.threads.push(
      makeThread({
        accountId,
        subject: payload.subject || "(no subject)",
        senderName: "You",
        senderEmail: payload.fromIdentity.address,
        category: "primary",
        sentByUser: true,
        messages: [
          {
            fromName: payload.fromIdentity.displayName,
            fromEmail: payload.fromIdentity.address,
            to: payload.to,
            date: now(),
            read: true,
            paragraphs: payload.bodyText.split(/\n{2,}/).filter(Boolean),
          },
        ],
      }),
    );
  }
}

export async function tickScheduler(): Promise<void> {
  const t = now();
  for (const thread of state.threads) {
    if (thread.snoozedUntil !== null && thread.snoozedUntil <= t) {
      thread.snoozedUntil = null;
    }
  }
  for (const job of state.jobs) {
    if (job.kind === "send" && job.state === "queued" && job.dueAt <= t) {
      if (job.payload) deliver(job, job.payload, job.accountId);
      job.state = "sent";
      if (job.draftId) {
        const draft = state.drafts.get(job.draftId);
        if (draft) draft.state = { kind: "sent", at: t };
      }
    } else if (job.kind === "followUp" && job.state === "scheduled" && job.dueAt <= t) {
      job.state = "due";
    }
  }
  state.jobs = state.jobs.filter((j) => j.state !== "cancelled");
}

// ── Phase B: Staff Sync & Collaboration mock backend ────────────────────────

function normalizeHandle(raw: string): string {
  return raw
    .trim()
    .replace(/^@+/, "")
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, "");
}

function extractMentions(body: string): string[] {
  const found = new Set<string>();
  const re = /(?:^|[^a-zA-Z0-9_.])@([a-zA-Z0-9_-]+)/g;
  let match = re.exec(body);
  while (match !== null) {
    const h = match[1]?.toLowerCase();
    if (h) found.add(h);
    match = re.exec(body);
  }
  return Array.from(found);
}

function seedMockStaffDemo(): void {
  if (state.staffDirectory.length > 0) return;
  const t = now();
  state.staffWorkspace = {
    connected: true,
    serverUrl: "local://pigment-demo",
    staffName: "Reyth",
    staffHandle: "reyth",
    staffEmail: "info@pigmentshow.us",
    staffRole: "Show Director",
    connectedAt: t - 3 * DAY,
    lastSyncAt: t - 2 * MINUTE,
    status: "connected",
    lastError: null,
    pendingOutboxCount: 0,
  };
  state.staffDirectory = [
    {
      id: newStaffMemberId(),
      name: "Reyth",
      role: "Show Director",
      handle: "reyth",
      email: "info@pigmentshow.us",
      department: "operations",
      updatedAt: t,
    },
    {
      id: newStaffMemberId(),
      name: "Maya Chen",
      role: "Sponsors Lead",
      handle: "maya",
      email: "sponsors@pigmentshow.us",
      department: "sponsors",
      updatedAt: t,
    },
    {
      id: newStaffMemberId(),
      name: "Devon Brooks",
      role: "Exhibition Coordinator",
      handle: "devon",
      email: "exhibition@pigmentshow.us",
      department: "exhibition",
      updatedAt: t,
    },
    {
      id: newStaffMemberId(),
      name: "Tessa Morales",
      role: "Marketing Lead",
      handle: "tessa",
      email: "marketing@pigmentshow.us",
      department: "marketing",
      updatedAt: t,
    },
    {
      id: newStaffMemberId(),
      name: "Sam Patel",
      role: "Volunteer Coordinator",
      handle: "sam",
      email: "volunteers@pigmentshow.us",
      department: "volunteers",
      updatedAt: t,
    },
  ];
  state.sharedTemplates = [
    {
      id: newSharedTemplateId(),
      name: "Sponsor Prospectus Reply",
      shortcut: "/sponsor",
      category: "sponsors",
      subject: "Pigment Miniatures Show 2027 — Sponsorship Prospectus",
      bodyText:
        "Hi there,\n\nThank you for your interest in sponsoring Pigment Miniatures Show 2027! I've attached our 2027 Sponsor Prospectus with booth dimensions, Gold/Silver/Bronze tiers, and print deadlines.\n\nWarmly,\nPigment Sponsors Team",
      updatedBy: "maya",
      updatedAt: t - DAY,
    },
    {
      id: newSharedTemplateId(),
      name: "Exhibitor Load-In FAQ",
      shortcut: "/loadin",
      category: "exhibitors",
      subject: "Pigment 2027 — Exhibitor Load-In & Table Details",
      bodyText:
        "Hi,\n\nExhibitor load-in opens Friday at 1:00 PM via the East Loading Dock. Each 6ft table includes black draping, two chairs, and two staff badges.\n\nBest,\nPigment Exhibition Team",
      updatedBy: "devon",
      updatedAt: t - 2 * DAY,
    },
    {
      id: newSharedTemplateId(),
      name: "Volunteer Onboarding Welcome",
      shortcut: "/vol",
      category: "volunteers",
      subject: "Welcome to the Pigment 2027 Volunteer Crew!",
      bodyText:
        "Hi,\n\nThank you for signing up to help run Pigment 2027! Volunteers who work two 4-hour shifts receive a full weekend badge and staff shirt.\n\nSee you at the show!\nPigment Volunteer Team",
      updatedBy: "sam",
      updatedAt: t - 4 * DAY,
    },
  ];
  state.showContacts = [
    {
      id: newContactId(),
      name: "Dana Okafor",
      emails: ["dana@ferngate.co"],
      category: "sponsor",
      organization: "Ferngate Miniatures",
      tier: "Gold Sponsor",
      notes: "Returning 2026 sponsor; prefers corner booth near lighting demo tables.",
      updatedAt: t - DAY,
    },
    {
      id: newContactId(),
      name: "Marco Bellini",
      emails: ["marco@vallejo-atelier.example"],
      category: "instructor",
      organization: "Bellini Studio",
      tier: "Keynote Workshop",
      notes: "Teaching NMM & Skin Tones Saturday 10am–2pm. Needs overhead camera rig.",
      updatedAt: t - 2 * DAY,
    },
    {
      id: newContactId(),
      name: "Priya Nair",
      emails: ["priya@austinconvention.example"],
      category: "venue",
      organization: "Cedar Creek Event Center",
      tier: "Venue Ops Lead",
      notes: "Main hall A/V, loading dock schedule, and room keycard contact.",
      updatedAt: t - 3 * DAY,
    },
  ];
  state.pinnedLinks = [
    {
      id: newPinnedLinkId(),
      title: "2027 Operations Workbook",
      url: "https://docs.google.com/spreadsheets/d/pigment-2027-ops",
      category: "operations",
      description: "Master floor plan, vendor map/payment, and room supplies sheets",
      position: 0,
      updatedAt: t,
    },
    {
      id: newPinnedLinkId(),
      title: "Pigment Admin Portal",
      url: "https://pigmentshow.us/admin",
      category: "admin",
      description: "Registrations, exhibitor badges, and ticket check-in",
      position: 1,
      updatedAt: t,
    },
    {
      id: newPinnedLinkId(),
      title: "Canva Marketing Assets",
      url: "https://www.canva.com/folder/pigment-2027-marketing",
      category: "marketing",
      description: "Social graphics, badges, and print banners",
      position: 2,
      updatedAt: t,
    },
    {
      id: newPinnedLinkId(),
      title: "Staff Discord",
      url: "https://discord.com/channels/@me",
      category: "comms",
      description: "Live staff chat and weekly planning voice channel",
      position: 3,
      updatedAt: t,
    },
  ];

  // A thread shared by Maya from sponsors@ so Shared with Me works out of the box.
  const sharedId = newSharedThreadId();
  state.sharedThreads = [
    {
      id: sharedId,
      sourceThreadId: null,
      sourceAccountId: null,
      ownerEmail: "sponsors@pigmentshow.us",
      ownerName: "Maya Chen",
      subject: "Gold Sponsor corner booth + lighting drop request",
      snippet: "Could we reserve Corner Booth A1 again and add a 20A power drop?",
      participants: ["dana@ferngate.co", "sponsors@pigmentshow.us"],
      messages: [
        {
          id: "snap-1",
          fromName: "Dana Okafor",
          fromEmail: "dana@ferngate.co",
          to: ["sponsors@pigmentshow.us"],
          cc: [],
          date: t - 3 * HOUR,
          snippet: "Could we reserve Corner Booth A1 again and add a 20A power drop?",
          bodyText:
            "Hi Maya,\n\nWe'd love to return as a Gold Sponsor for Pigment 2027! Could we reserve Corner Booth A1 again and add a 20A power drop for our airbrush demo station?\n\nBest,\nDana",
          bodySrcdoc:
            "<!doctype html><html><body style='font-family:Inter,sans-serif;font-size:13px;line-height:1.55;margin:0;color:inherit'><p>Hi Maya,</p><p>We'd love to return as a <strong>Gold Sponsor</strong> for Pigment 2027! Could we reserve Corner Booth A1 again and add a 20A power drop for our airbrush demo station?</p><p>Best,<br/>Dana</p></body></html>",
        },
      ],
      visibility: "allStaff",
      invitedHandles: [],
      assigneeHandle: "reyth",
      status: "open",
      comments: [
        {
          id: newCommentId(),
          sharedThreadId: sharedId,
          authorHandle: "maya",
          authorName: "Maya Chen",
          bodyText:
            "Hey @reyth and @devon — confirming whether Corner Booth A1 has a 20A drop on the 2027 venue map before I lock in the invoice.",
          mentions: ["reyth", "devon"],
          createdAt: t - 2 * HOUR,
        },
      ],
      draft: {
        sharedThreadId: sharedId,
        subject: "Re: Gold Sponsor corner booth + lighting drop request",
        to: ["dana@ferngate.co"],
        cc: [],
        bodyText:
          "Hi Dana,\n\nThrilled to have Ferngate back as a Gold Sponsor! Corner Booth A1 is yours and we've noted the 20A power drop with the venue ops team.\n\nWarmly,\nMaya",
        editorHandle: null,
        editorName: null,
        lockState: "reviewRequested",
        reviewRequestedBy: "maya",
        approvedBy: null,
        approvedAt: null,
        rev: 2,
        updatedAt: t - HOUR,
      },
      sharedAt: t - 2 * HOUR,
      updatedAt: t - HOUR,
    },
  ];
}

function summarizeShared(t: SharedThreadView): SharedThreadSummary {
  return {
    id: t.id,
    sourceThreadId: t.sourceThreadId,
    sourceAccountId: t.sourceAccountId,
    ownerEmail: t.ownerEmail,
    ownerName: t.ownerName,
    subject: t.subject,
    snippet: t.snippet,
    participants: t.participants.slice(),
    messageCount: t.messages.length,
    visibility: t.visibility,
    invitedHandles: t.invitedHandles.slice(),
    assigneeHandle: t.assigneeHandle,
    status: t.status,
    commentCount: t.comments.length,
    draftLockState: t.draft?.lockState ?? "unlocked",
    draftEditorName: t.draft?.editorName ?? null,
    sharedAt: t.sharedAt,
    updatedAt: t.updatedAt,
  };
}

export async function getStaffWorkspace(): Promise<StaffWorkspaceView> {
  seedMockStaffDemo();
  return { ...state.staffWorkspace };
}

export async function connectStaffWorkspace(
  input: ConnectStaffWorkspaceInput,
): Promise<StaffWorkspaceView> {
  const url = input.serverUrl.trim();
  if (!/^(https?:|local:)\/\//i.test(url)) {
    throw new Error("Server URL must start with https://, http://, or local://");
  }
  if (!input.token.trim()) throw new Error("Staff token is required");
  const handle = normalizeHandle(input.staffHandle);
  if (!handle) throw new Error("A valid @handle is required");
  const t = now();
  state.staffWorkspace = {
    connected: true,
    serverUrl: url,
    staffName: input.staffName.trim() || "Staff Member",
    staffHandle: handle,
    staffEmail: input.staffEmail.trim().toLowerCase(),
    staffRole: input.staffRole.trim() || "Pigment Staff",
    connectedAt: t,
    lastSyncAt: t,
    status: "connected",
    lastError: null,
    pendingOutboxCount: 0,
  };
  if (!state.staffDirectory.some((m) => m.handle === handle)) {
    state.staffDirectory.push({
      id: newStaffMemberId(),
      name: state.staffWorkspace.staffName,
      role: state.staffWorkspace.staffRole,
      handle,
      email: state.staffWorkspace.staffEmail,
      department: null,
      updatedAt: t,
    });
  }
  return { ...state.staffWorkspace };
}

export async function disconnectStaffWorkspace(): Promise<void> {
  state.staffWorkspace = emptyWorkspace();
}

export async function syncStaffWorkspace(): Promise<StaffWorkspaceView> {
  if (!state.staffWorkspace.connected) {
    throw new Error("Staff Sync workspace is not connected");
  }
  state.staffWorkspace = {
    ...state.staffWorkspace,
    lastSyncAt: now(),
    status: "connected",
    lastError: null,
    pendingOutboxCount: 0,
  };
  return { ...state.staffWorkspace };
}

export async function pendingStaffOutboxCount(): Promise<number> {
  seedMockStaffDemo();
  return state.staffWorkspace.pendingOutboxCount;
}

export async function listStaffDirectory(): Promise<StaffMemberView[]> {
  seedMockStaffDemo();
  return state.staffDirectory.slice();
}

export async function listSharedThreads(): Promise<SharedThreadSummary[]> {
  seedMockStaffDemo();
  return state.sharedThreads
    .slice()
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .map(summarizeShared);
}

export async function openSharedThread(
  sharedThreadId: SharedThreadId,
): Promise<SharedThreadView> {
  seedMockStaffDemo();
  const found = state.sharedThreads.find((t) => t.id === sharedThreadId);
  if (!found) throw new Error("Shared thread not found");
  return structuredClone(found);
}

export async function findSharedThreadForLocal(
  threadId: ThreadId,
): Promise<SharedThreadView | null> {
  seedMockStaffDemo();
  const found = state.sharedThreads.find((t) => t.sourceThreadId === threadId);
  return found ? structuredClone(found) : null;
}

export async function shareThread(input: ShareThreadInput): Promise<SharedThreadView> {
  seedMockStaffDemo();
  if (!state.staffWorkspace.connected) {
    throw new Error("Connect to Pigment Staff Sync in Settings first.");
  }
  const thread = threadById(input.threadId);
  if (!thread) throw new Error("Thread not found");
  const account = accountById(thread.accountId);
  if (!account) throw new Error("Account not found");
  if (account.spaceKind !== "pigmentStaff") {
    throw new Error(
      "Staff Sharing is disabled for Personal spaces — switch this account to Pigment Staff to share threads.",
    );
  }
  const t = now();
  const existing = state.sharedThreads.find((s) => s.sourceThreadId === thread.id);
  const sharedId = existing?.id ?? newSharedThreadId();
  const participants = Array.from(
    new Set(thread.messages.flatMap((m) => [m.fromEmail, ...m.to, ...m.cc]).filter(Boolean)),
  );
  const invitedHandles =
    input.visibility === "allStaff"
      ? []
      : Array.from(new Set(input.invitedHandles.map(normalizeHandle).filter(Boolean)));
  if (input.visibility === "specific" && invitedHandles.length === 0) {
    throw new Error("Select at least one staff member or choose All Staff");
  }
  const view: SharedThreadView = {
    id: sharedId,
    sourceThreadId: thread.id,
    sourceAccountId: account.id,
    ownerEmail: account.email,
    ownerName: state.staffWorkspace.staffName,
    subject: thread.subject,
    snippet: lastMessage(thread)?.snippet ?? "",
    participants,
    messages: thread.messages.map((m) => ({
      id: m.id,
      fromName: m.fromName,
      fromEmail: m.fromEmail,
      to: m.to.slice(),
      cc: m.cc.slice(),
      date: m.date,
      snippet: m.snippet,
      bodyText: m.bodyText,
      bodySrcdoc: messageView(m, "blockAll").bodySrcdoc,
    })),
    visibility: input.visibility,
    invitedHandles,
    assigneeHandle: input.assigneeHandle ? normalizeHandle(input.assigneeHandle) : null,
    status: existing?.status ?? "open",
    comments: existing?.comments ?? [],
    draft: existing?.draft ?? null,
    sharedAt: existing?.sharedAt ?? t,
    updatedAt: t,
  };
  if (input.initialNote?.trim()) {
    const body = input.initialNote.trim();
    view.comments.push({
      id: newCommentId(),
      sharedThreadId: sharedId,
      authorHandle: state.staffWorkspace.staffHandle,
      authorName: state.staffWorkspace.staffName,
      bodyText: body,
      mentions: extractMentions(body),
      createdAt: t,
    });
  }
  if (existing) {
    Object.assign(existing, view);
  } else {
    state.sharedThreads.unshift(view);
  }
  return structuredClone(view);
}

export async function updateSharedHandoff(
  input: UpdateHandoffInput,
): Promise<SharedThreadView> {
  const found = state.sharedThreads.find((t) => t.id === input.sharedThreadId);
  if (!found) throw new Error("Shared thread not found");
  found.assigneeHandle = input.assigneeHandle ? normalizeHandle(input.assigneeHandle) : null;
  found.status = input.status;
  found.updatedAt = now();
  return structuredClone(found);
}

export async function addSharedComment(
  input: AddSharedCommentInput,
): Promise<SharedCommentView> {
  if (!state.staffWorkspace.connected) {
    throw new Error("Connect to Pigment Staff Sync first.");
  }
  const found = state.sharedThreads.find((t) => t.id === input.sharedThreadId);
  if (!found) throw new Error("Shared thread not found");
  const body = input.bodyText.trim();
  if (!body) throw new Error("Comment cannot be empty");
  const comment: SharedCommentView = {
    id: newCommentId(),
    sharedThreadId: found.id,
    authorHandle: state.staffWorkspace.staffHandle,
    authorName: state.staffWorkspace.staffName,
    bodyText: body,
    mentions: extractMentions(body),
    createdAt: now(),
  };
  found.comments.push(comment);
  found.updatedAt = comment.createdAt;
  return { ...comment };
}

export async function updateSharedDraft(
  input: UpdateSharedDraftInput,
): Promise<SharedDraftView> {
  if (!state.staffWorkspace.connected) {
    throw new Error("Connect to Pigment Staff Sync first.");
  }
  const found = state.sharedThreads.find((t) => t.id === input.sharedThreadId);
  if (!found) throw new Error("Shared thread not found");
  const t = now();
  const me = state.staffWorkspace.staffHandle;
  const myName = state.staffWorkspace.staffName;
  const last = found.messages[found.messages.length - 1];
  const draft: SharedDraftView = found.draft ?? {
    sharedThreadId: found.id,
    subject: /^re:/i.test(found.subject) ? found.subject : `Re: ${found.subject}`,
    to: last ? [last.fromEmail] : [],
    cc: [],
    bodyText: "",
    editorHandle: null,
    editorName: null,
    lockState: "unlocked",
    reviewRequestedBy: null,
    approvedBy: null,
    approvedAt: null,
    rev: 1,
    updatedAt: t,
  };

  const action = input.action;
  if (action.kind === "claim") {
    if (draft.lockState === "editing" && draft.editorHandle !== me) {
      throw new Error(`Draft is currently locked by ${draft.editorName ?? draft.editorHandle}`);
    }
    draft.lockState = "editing";
    draft.editorHandle = me;
    draft.editorName = myName;
  } else if (action.kind === "takeOver") {
    draft.lockState = "editing";
    draft.editorHandle = me;
    draft.editorName = myName;
    draft.approvedBy = null;
    draft.approvedAt = null;
  } else if (action.kind === "save") {
    if (draft.lockState !== "editing" || draft.editorHandle !== me) {
      throw new Error("Claim or take over the draft lock before saving");
    }
    draft.subject = action.subject;
    draft.to = action.to;
    draft.cc = action.cc;
    draft.bodyText = action.bodyText;
  } else if (action.kind === "requestReview") {
    draft.lockState = "reviewRequested";
    draft.editorHandle = null;
    draft.editorName = null;
    draft.reviewRequestedBy = me;
    draft.approvedBy = null;
    draft.approvedAt = null;
    const handles = action.reviewerHandles.map(normalizeHandle).filter(Boolean);
    const ping =
      handles.length > 0
        ? `Requested review on the shared reply draft (${handles.map((h) => `@${h}`).join(" ")}).`
        : "Requested review on the shared reply draft.";
    const full = action.note?.trim() ? `${ping} ${action.note.trim()}` : ping;
    found.comments.push({
      id: newCommentId(),
      sharedThreadId: found.id,
      authorHandle: me,
      authorName: myName,
      bodyText: full,
      mentions: extractMentions(full),
      createdAt: t,
    });
  } else if (action.kind === "approve") {
    draft.lockState = "approved";
    draft.editorHandle = null;
    draft.editorName = null;
    draft.approvedBy = me;
    draft.approvedAt = t;
  } else if (action.kind === "release") {
    draft.lockState = "unlocked";
    draft.editorHandle = null;
    draft.editorName = null;
  }
  draft.rev += 1;
  draft.updatedAt = t;
  found.draft = draft;
  found.updatedAt = t;
  return structuredClone(draft);
}

export async function listSharedTemplates(): Promise<SharedTemplateView[]> {
  seedMockStaffDemo();
  return state.sharedTemplates.slice();
}

export async function saveSharedTemplate(
  input: SaveSharedTemplateInput,
): Promise<SharedTemplateView> {
  seedMockStaffDemo();
  const t = now();
  const actor = state.staffWorkspace.connected ? state.staffWorkspace.staffHandle : "staff";
  const existing = input.id ? state.sharedTemplates.find((s) => s.id === input.id) : undefined;
  if (existing) {
    existing.name = input.name.trim();
    existing.shortcut = input.shortcut?.trim() || null;
    existing.category = input.category.trim() || "general";
    existing.subject = input.subject;
    existing.bodyText = input.bodyText;
    existing.updatedBy = actor;
    existing.updatedAt = t;
    return { ...existing };
  }
  const created: SharedTemplateView = {
    id: newSharedTemplateId(),
    name: input.name.trim(),
    shortcut: input.shortcut?.trim() || null,
    category: input.category.trim() || "general",
    subject: input.subject,
    bodyText: input.bodyText,
    updatedBy: actor,
    updatedAt: t,
  };
  state.sharedTemplates.push(created);
  return { ...created };
}

export async function deleteSharedTemplate(templateId: SharedTemplateId): Promise<void> {
  state.sharedTemplates = state.sharedTemplates.filter((t) => t.id !== templateId);
}

export async function listShowContacts(): Promise<ShowContactView[]> {
  seedMockStaffDemo();
  return state.showContacts.slice();
}

export async function findContactByEmail(email: string): Promise<ShowContactView | null> {
  seedMockStaffDemo();
  const target = email.trim().toLowerCase();
  const found = state.showContacts.find((c) => c.emails.some((e) => e.toLowerCase() === target));
  return found ? { ...found, emails: found.emails.slice() } : null;
}

export async function saveShowContact(
  input: SaveShowContactInput,
): Promise<ShowContactView> {
  seedMockStaffDemo();
  const t = now();
  const emails = Array.from(
    new Set(
      input.emails
        .map((e) => e.trim().toLowerCase())
        .filter((e) => e.includes("@") && e.length >= 3),
    ),
  );
  if (emails.length === 0) throw new Error("At least one valid email address is required");
  const existing = input.id ? state.showContacts.find((c) => c.id === input.id) : undefined;
  if (existing) {
    existing.name = input.name.trim();
    existing.emails = emails;
    existing.category = input.category;
    existing.organization = input.organization?.trim() || null;
    existing.tier = input.tier?.trim() || null;
    existing.notes = input.notes.trim();
    existing.updatedAt = t;
    return { ...existing, emails: existing.emails.slice() };
  }
  const created: ShowContactView = {
    id: newContactId(),
    name: input.name.trim(),
    emails,
    category: input.category,
    organization: input.organization?.trim() || null,
    tier: input.tier?.trim() || null,
    notes: input.notes.trim(),
    updatedAt: t,
  };
  state.showContacts.push(created);
  return { ...created, emails: created.emails.slice() };
}

export async function deleteShowContact(contactId: ContactId): Promise<void> {
  state.showContacts = state.showContacts.filter((c) => c.id !== contactId);
}

export async function listPinnedLinks(): Promise<PinnedLinkView[]> {
  seedMockStaffDemo();
  return state.pinnedLinks.slice().sort((a, b) => a.position - b.position);
}

export async function savePinnedLink(input: SavePinnedLinkInput): Promise<PinnedLinkView> {
  seedMockStaffDemo();
  const url = input.url.trim();
  if (!/^(https?:|discord:)\/\//i.test(url)) {
    throw new Error("URL must start with https://, http://, or discord://");
  }
  const t = now();
  const existing = input.id ? state.pinnedLinks.find((l) => l.id === input.id) : undefined;
  if (existing) {
    existing.title = input.title.trim();
    existing.url = url;
    existing.category = input.category.trim() || "operations";
    existing.description = input.description?.trim() || null;
    existing.updatedAt = t;
    return { ...existing };
  }
  const created: PinnedLinkView = {
    id: newPinnedLinkId(),
    title: input.title.trim(),
    url,
    category: input.category.trim() || "operations",
    description: input.description?.trim() || null,
    position: state.pinnedLinks.length,
    updatedAt: t,
  };
  state.pinnedLinks.push(created);
  return { ...created };
}

export async function deletePinnedLink(linkId: PinnedLinkId): Promise<void> {
  state.pinnedLinks = state.pinnedLinks.filter((l) => l.id !== linkId);
}

// ── Phase C: Calendar & Meetings Mock Backend ───────────────────────────────

interface MockCalendarState {
  seeded: boolean;
  events: CalendarEventView[];
  meetingDocs: Map<CalendarEventId, MeetingDocView>;
  actionItems: ActionItemView[];
  invites: Map<string, CalendarInviteView>;
  overlays: IcsOverlayView[];
  overlayBlocks: OverlayBusyBlock[];
  discordWebhookConfigured: boolean;
}

const calState: MockCalendarState = {
  seeded: false,
  events: [],
  meetingDocs: new Map(),
  actionItems: [],
  invites: new Map(),
  overlays: [],
  overlayBlocks: [],
  discordWebhookConfigured: false,
};

function startOfTodayLocal(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function seedMockCalendarDemo(): void {
  if (calState.seeded) return;
  calState.seeded = true;
  const base = startOfTodayLocal();
  const t = now();
  const series = "series-pigment-weekly-staff";

  const prevSyncId = newCalendarEventId();
  const thisSyncId = newCalendarEventId();
  const sponsorId = newCalendarEventId();
  const roomId = newCalendarEventId();
  const workshopId = newCalendarEventId();

  calState.events = [
    {
      id: prevSyncId,
      seriesId: series,
      title: "Pigment Weekly Staff Sync",
      description: "Previous week's staff operations sync.",
      startAt: base - 7 * DAY + 19 * HOUR,
      endAt: base - 7 * DAY + 20 * HOUR,
      allDay: false,
      kind: "meeting",
      department: "operations",
      location: "Discord #staff-voice",
      recurrence: "none",
      discordUrl: "https://discord.com/channels/pigment-staff/staff-voice",
      attendees: ["reyth", "elena", "marcus", "priya"],
      sourceIcsUid: null,
      createdBy: "reyth",
      updatedAt: t,
    },
    {
      id: thisSyncId,
      seriesId: series,
      title: "Pigment Weekly Staff Sync",
      description: "Weekly operations, sponsor, and exhibition check-in.",
      startAt: base + DAY + 19 * HOUR,
      endAt: base + DAY + 20 * HOUR,
      allDay: false,
      kind: "meeting",
      department: "operations",
      location: "Discord #staff-voice",
      recurrence: "weekly",
      discordUrl: "https://discord.com/channels/pigment-staff/staff-voice",
      attendees: ["reyth", "elena", "marcus", "priya"],
      sourceIcsUid: null,
      createdBy: "reyth",
      updatedAt: t,
    },
    {
      id: sponsorId,
      seriesId: null,
      title: "Gold Sponsor Deposit Deadline",
      description: "Confirm Monument Hobbies & Vallejo booth deposits and logo assets.",
      startAt: base + 3 * DAY + 17 * HOUR,
      endAt: base + 3 * DAY + 18 * HOUR,
      allDay: false,
      kind: "deadline",
      department: "sponsors",
      location: null,
      recurrence: "none",
      discordUrl: null,
      attendees: ["elena", "reyth"],
      sourceIcsUid: null,
      createdBy: "elena",
      updatedAt: t,
    },
    {
      id: roomId,
      seriesId: null,
      title: "Ballroom A & Paint Lab AV Walkthrough",
      description: "Verify power drops, display case lighting rigs, and judging table layout.",
      startAt: base + 2 * DAY + 14 * HOUR,
      endAt: base + 2 * DAY + 15.5 * HOUR,
      allDay: false,
      kind: "roomSchedule",
      department: "exhibition",
      location: "Ballroom A",
      recurrence: "none",
      discordUrl: null,
      attendees: ["marcus", "reyth"],
      sourceIcsUid: null,
      createdBy: "marcus",
      updatedAt: t,
    },
    {
      id: workshopId,
      seriesId: null,
      title: "Airbrush Workshop Prep Check-in",
      description: "Confirm compressor count and booth ventilation with instructors.",
      startAt: base + 4 * DAY + 11 * HOUR,
      endAt: base + 4 * DAY + 12 * HOUR,
      allDay: false,
      kind: "meeting",
      department: "workshops",
      location: "Paint Lab 2",
      recurrence: "none",
      discordUrl: "https://discord.com/channels/pigment-staff/workshops",
      attendees: ["marcus", "priya"],
      sourceIcsUid: null,
      createdBy: "marcus",
      updatedAt: t,
    },
  ];

  const carryoverItem: ActionItemView = {
    id: newActionItemId(),
    title: "Confirm Ballroom A power drop map with hotel AV",
    department: "operations",
    milestone: "PMS 2027",
    assigneeHandle: "marcus",
    status: "open",
    dueAt: base + 2 * DAY,
    startTime: "14:00",
    endTime: "15:30",
    location: "Ballroom A",
    costNote: "$450 AV drop",
    calendarEventId: roomId,
    sourceEventId: prevSyncId,
    sourceSeriesId: series,
    sourceThreadId: null,
    createdBy: "reyth",
    createdAt: base - 7 * DAY,
    updatedAt: t,
  };
  calState.actionItems.push(
    carryoverItem,
    {
      id: newActionItemId(),
      title: "Collect Gold Sponsor logos for program print",
      department: "sponsors",
      milestone: "PMS 2027",
      assigneeHandle: "maya",
      status: "open",
      dueAt: base + 3 * DAY,
      startTime: null,
      endTime: null,
      location: null,
      costNote: null,
      calendarEventId: sponsorId,
      sourceEventId: thisSyncId,
      sourceSeriesId: series,
      sourceThreadId: null,
      createdBy: "reyth",
      createdAt: base - 2 * DAY,
      updatedAt: t,
    },
    {
      id: newActionItemId(),
      title: "Order 24 acrylic display plinths & velvet risers",
      department: "shopping-list",
      milestone: "PMS 2027",
      assigneeHandle: null,
      status: "open",
      dueAt: base - DAY,
      startTime: null,
      endTime: null,
      location: "Exhibition Hall",
      costNote: "$680",
      calendarEventId: null,
      sourceEventId: null,
      sourceSeriesId: null,
      sourceThreadId: null,
      createdBy: "reyth",
      createdAt: base - 5 * DAY,
      updatedAt: t,
    },
    {
      id: newActionItemId(),
      title: "Publish workshop registration banner & Discord announcement",
      department: "marketing",
      milestone: "PMS 2027",
      assigneeHandle: "reyth",
      status: "open",
      dueAt: base + 5 * DAY,
      startTime: "10:00",
      endTime: "11:00",
      location: null,
      costNote: null,
      calendarEventId: null,
      sourceEventId: null,
      sourceSeriesId: null,
      sourceThreadId: null,
      createdBy: "reyth",
      createdAt: base - 3 * DAY,
      updatedAt: t,
    },
    {
      id: newActionItemId(),
      title: "Book promo table & badges for ReaperCon outreach",
      department: "marketing",
      milestone: "ReaperCon 2027",
      assigneeHandle: "tessa",
      status: "done",
      dueAt: base - 4 * DAY,
      startTime: null,
      endTime: null,
      location: "Denton Convention Center",
      costNote: "$250",
      calendarEventId: null,
      sourceEventId: null,
      sourceSeriesId: null,
      sourceThreadId: null,
      createdBy: "reyth",
      createdAt: base - 10 * DAY,
      updatedAt: t,
    },
  );

  calState.meetingDocs.set(thisSyncId, {
    eventId: thisSyncId,
    seriesId: series,
    agenda: [
      {
        id: "ag-1",
        text: "Review Monument Hobbies & Vallejo booth deposits",
        checked: false,
        addedBy: "elena",
        promotedActionId: null,
      },
      {
        id: "ag-2",
        text: "Finalize volunteer shift schedule for Friday setup",
        checked: false,
        addedBy: "priya",
        promotedActionId: null,
      },
    ],
    notesMarkdown:
      "## Quick Updates\n- Exhibition display cases confirmed for Ballroom A.\n- Workshop registration opens next Monday.",
    notesEditorHandle: null,
    notesEditorName: null,
    rev: 1,
    updatedAt: t,
    actionItems: [],
    carryoverItems: [],
  });

  const personalOverlayId = newIcsOverlayId();
  calState.overlays.push({
    id: personalOverlayId,
    name: "Personal (Google .ics)",
    url: "local://personal-demo",
    color: "violet",
    enabled: true,
    lastFetchedAt: t,
    lastError: null,
    eventCount: 2,
  });
  calState.overlayBlocks.push(
    {
      overlayId: personalOverlayId,
      overlayName: "Personal (Google .ics)",
      overlayColor: "violet",
      uid: "personal-busy-1",
      summary: "Busy (Personal Calendar)",
      startAt: base + DAY + 14 * HOUR,
      endAt: base + DAY + 15 * HOUR,
      allDay: false,
    },
    {
      overlayId: personalOverlayId,
      overlayName: "Personal (Google .ics)",
      overlayColor: "violet",
      uid: "personal-busy-2",
      summary: "Busy (Family Pickup)",
      startAt: base + 2 * DAY + 16 * HOUR,
      endAt: base + 2 * DAY + 17 * HOUR,
      allDay: false,
    },
  );
}

function mockInvitesForMessage(message: StoredMessage): CalendarInviteView[] {
  seedMockCalendarDemo();
  // Surface a realistic .ics invite on the first message of the first Pigment Staff thread
  // (or any thread mentioning "booth" / "walkthrough" / "invoice").
  const existing = Array.from(calState.invites.values()).filter(
    (inv) => inv.messageId === message.id,
  );
  if (existing.length > 0) return existing;

  const subjectMatch =
    /booth|sponsor|venue|walkthrough|retainer|pigment/i.test(message.snippet) ||
    /monument|pigment|ferngate/i.test(message.fromEmail);
  if (subjectMatch && calState.invites.size === 0) {
    const base = startOfTodayLocal();
    const uid = `ics-${message.id}`;
    const inv: CalendarInviteView = {
      messageId: message.id,
      uid,
      summary: "Pigment 2027 Sponsor & Booth Floor Walkthrough",
      description: "Quick walkthrough of Gold Sponsor booth placement and electrical drops.",
      location: "Ballroom A · Conference Line",
      organizerName: message.fromName,
      organizerEmail: message.fromEmail,
      startAt: base + 2 * DAY + 11 * HOUR,
      endAt: base + 2 * DAY + 12 * HOUR,
      allDay: false,
      rsvpStatus: "needsAction",
      addedEventId: null,
    };
    calState.invites.set(`${message.id}:${uid}`, inv);
    return [inv];
  }
  return [];
}

function expandMockOccurrences(
  events: CalendarEventView[],
  rangeStart: UnixMs,
  rangeEnd: UnixMs,
): CalendarOccurrence[] {
  const out: CalendarOccurrence[] = [];
  for (const ev of events) {
    const dur = Math.max(ev.endAt - ev.startAt, 15 * MINUTE);
    if (ev.recurrence === "none") {
      if (ev.endAt >= rangeStart && ev.startAt <= rangeEnd) {
        out.push({
          event: ev,
          occurrenceStart: ev.startAt,
          occurrenceEnd: ev.endAt,
          isRecurringInstance: false,
        });
      }
      continue;
    }
    let cursor = new Date(ev.startAt);
    let idx = 0;
    while (cursor.getTime() <= rangeEnd && idx < 120) {
      const s = cursor.getTime();
      const e = s + dur;
      if (e >= rangeStart && s <= rangeEnd) {
        out.push({
          event: ev,
          occurrenceStart: s,
          occurrenceEnd: e,
          isRecurringInstance: idx > 0,
        });
      }
      const next = new Date(cursor.getTime());
      if (ev.recurrence === "weekly") next.setDate(next.getDate() + 7);
      else if (ev.recurrence === "biweekly") next.setDate(next.getDate() + 14);
      else if (ev.recurrence === "monthly") next.setMonth(next.getMonth() + 1);
      cursor = next;
      idx += 1;
    }
  }
  return out.sort((a, b) => a.occurrenceStart - b.occurrenceStart);
}

export async function listCalendarEvents(
  query: CalendarRangeQuery,
): Promise<CalendarOccurrence[]> {
  seedMockCalendarDemo();
  return expandMockOccurrences(calState.events, query.rangeStart, query.rangeEnd);
}

export async function saveCalendarEvent(
  input: SaveCalendarEventInput,
): Promise<CalendarEventView> {
  seedMockCalendarDemo();
  const title = input.title.trim();
  if (!title) throw new Error("Event title is required");
  if (input.endAt <= input.startAt) throw new Error("End time must be after start time");
  const t = now();
  const id = input.id ?? newCalendarEventId();
  const seriesId =
    input.seriesId?.trim() ||
    (input.recurrence !== "none" ? `series-${id}` : null);
  const view: CalendarEventView = {
    id,
    seriesId,
    title,
    description: input.description.trim(),
    startAt: input.startAt,
    endAt: input.endAt,
    allDay: input.allDay,
    kind: input.kind,
    department: input.department?.trim().replace(/^#/, "").toLowerCase() || null,
    location: input.location?.trim() || null,
    recurrence: input.recurrence,
    discordUrl: input.discordUrl?.trim() || null,
    attendees: input.attendees,
    sourceIcsUid: input.sourceIcsUid ?? null,
    createdBy: state.staffWorkspace.staffHandle || "reyth",
    updatedAt: t,
  };
  const idx = calState.events.findIndex((e) => e.id === id);
  if (idx >= 0) calState.events[idx] = view;
  else calState.events.push(view);
  return { ...view };
}

export async function quickAddCalendarEvent(text: string): Promise<CalendarEventView> {
  seedMockCalendarDemo();
  const trimmed = text.trim();
  if (!trimmed) throw new Error('Enter an event like "Staff sync Thu 7pm #operations"');
  const d = new Date();
  let hour = 10;
  let minute = 0;
  let durationMins = 60;
  let department: string | null = null;
  let recurrence: RecurrenceRule = "none";
  let kind: CalendarEventKind = "meeting";
  const locationTokens: string[] = [];
  let collectingLoc = false;
  const titleTokens: string[] = [];
  const weekdays: Record<string, number> = {
    sun: 0, sunday: 0, mon: 1, monday: 1, tue: 2, tues: 2, tuesday: 2,
    wed: 3, wednesday: 3, thu: 4, thur: 4, thurs: 4, thursday: 4,
    fri: 5, friday: 5, sat: 6, saturday: 6,
  };
  for (const tok of trimmed.split(/\s+/)) {
    const low = tok.toLowerCase();
    if (tok.startsWith("#")) {
      collectingLoc = false;
      department = tok.slice(1).toLowerCase();
      continue;
    }
    if (tok.startsWith("@")) {
      collectingLoc = true;
      if (tok.length > 1) locationTokens.push(tok.slice(1));
      continue;
    }
    if (low === "weekly" || low === "biweekly" || low === "monthly") {
      collectingLoc = false;
      recurrence = low;
      continue;
    }
    if (low === "tomorrow" || low === "tmrw") {
      collectingLoc = false;
      d.setDate(d.getDate() + 1);
      continue;
    }
    if (low === "today") {
      collectingLoc = false;
      continue;
    }
    const cleanDay = low.replace(/,$/, "");
    if (cleanDay in weekdays) {
      collectingLoc = false;
      const target = weekdays[cleanDay]!;
      let delta = (target - d.getDay() + 7) % 7;
      if (delta === 0) delta = 7;
      d.setDate(d.getDate() + delta);
      continue;
    }
    const timeMatch = /^(\d{1,2})(?::(\d{2}))?(am|pm)$/i.exec(low);
    if (timeMatch) {
      collectingLoc = false;
      let h = Number(timeMatch[1]);
      const m = Number(timeMatch[2] ?? "0");
      const mer = timeMatch[3]!.toLowerCase();
      if (mer === "pm" && h !== 12) h += 12;
      if (mer === "am" && h === 12) h = 0;
      hour = h;
      minute = m;
      continue;
    }
    const durMatch = /^(\d+)(h|m)$/i.exec(low);
    if (durMatch) {
      collectingLoc = false;
      const val = Number(durMatch[1]);
      durationMins = durMatch[2]!.toLowerCase() === "h" ? val * 60 : val;
      continue;
    }
    if (low.includes("deadline")) kind = "deadline";
    else if (low.includes("milestone")) kind = "milestone";
    if (collectingLoc) locationTokens.push(tok);
    else titleTokens.push(tok);
  }
  d.setHours(hour, minute, 0, 0);
  const startAt = d.getTime();
  return saveCalendarEvent({
    id: null,
    title: titleTokens.join(" ") || "Staff Event",
    description: "",
    startAt,
    endAt: startAt + durationMins * MINUTE,
    allDay: false,
    kind,
    department,
    location: locationTokens.join(" ") || null,
    recurrence,
    discordUrl: null,
    attendees: [],
  });
}

export async function deleteCalendarEvent(eventId: CalendarEventId): Promise<void> {
  seedMockCalendarDemo();
  calState.events = calState.events.filter((e) => e.id !== eventId);
  calState.meetingDocs.delete(eventId);
}

export async function getMeetingDoc(eventId: CalendarEventId): Promise<MeetingDocView> {
  seedMockCalendarDemo();
  const ev = calState.events.find((e) => e.id === eventId);
  if (!ev) throw new Error("Calendar event not found");
  const existing = calState.meetingDocs.get(eventId) ?? {
    eventId,
    seriesId: ev.seriesId,
    agenda: [],
    notesMarkdown: "",
    notesEditorHandle: null,
    notesEditorName: null,
    rev: 1,
    updatedAt: now(),
    actionItems: [],
    carryoverItems: [],
  };
  const actionItems = calState.actionItems.filter((a) => a.sourceEventId === eventId);
  const carryoverItems = ev.seriesId
    ? calState.actionItems.filter(
        (a) =>
          a.sourceSeriesId === ev.seriesId &&
          a.status === "open" &&
          a.sourceEventId !== eventId,
      )
    : [];
  return {
    ...existing,
    seriesId: ev.seriesId,
    agenda: existing.agenda.map((a) => ({ ...a })),
    actionItems,
    carryoverItems,
  };
}

export async function updateMeetingDoc(input: UpdateMeetingDocInput): Promise<MeetingDocView> {
  const doc = await getMeetingDoc(input.eventId);
  const actorHandle = state.staffWorkspace.staffHandle || "reyth";
  const actorName = state.staffWorkspace.staffName || "Reyth";
  if (input.action.kind === "updateAgenda") {
    doc.agenda = input.action.agenda;
  } else if (input.action.kind === "claimNotesLock") {
    doc.notesEditorHandle = actorHandle;
    doc.notesEditorName = actorName;
  } else if (input.action.kind === "saveNotes") {
    doc.notesMarkdown = input.action.notesMarkdown;
    if (input.action.releaseLock) {
      doc.notesEditorHandle = null;
      doc.notesEditorName = null;
    } else {
      doc.notesEditorHandle = actorHandle;
      doc.notesEditorName = actorName;
    }
  } else if (input.action.kind === "releaseNotesLock") {
    doc.notesEditorHandle = null;
    doc.notesEditorName = null;
  }
  doc.rev += 1;
  doc.updatedAt = now();
  calState.meetingDocs.set(input.eventId, doc);
  return getMeetingDoc(input.eventId);
}

export async function promoteToActionItem(
  input: PromoteActionItemInput,
): Promise<ActionItemView> {
  seedMockCalendarDemo();
  const ev = calState.events.find((e) => e.id === input.eventId);
  if (!ev) throw new Error("Calendar event not found");
  const t = now();
  const item: ActionItemView = {
    id: newActionItemId(),
    title: input.title.trim(),
    department: input.department ?? ev.department,
    milestone: "PMS 2027",
    assigneeHandle: input.assigneeHandle?.trim().replace(/^@/, "") || null,
    status: "open",
    dueAt: input.dueAt,
    startTime: null,
    endTime: null,
    location: ev.location,
    costNote: null,
    calendarEventId: null,
    sourceEventId: ev.id,
    sourceSeriesId: ev.seriesId,
    sourceThreadId: null,
    createdBy: state.staffWorkspace.staffHandle || "reyth",
    createdAt: t,
    updatedAt: t,
  };
  calState.actionItems.push(item);
  if (input.agendaItemId) {
    const doc = await getMeetingDoc(input.eventId);
    const bullet = doc.agenda.find((a) => a.id === input.agendaItemId);
    if (bullet) {
      bullet.promotedActionId = item.id;
      calState.meetingDocs.set(input.eventId, doc);
    }
  }
  return item;
}

export async function toggleActionItem(
  actionItemId: ActionItemId,
  status: ActionItemStatus,
): Promise<ActionItemView> {
  seedMockCalendarDemo();
  const item = calState.actionItems.find((a) => a.id === actionItemId);
  if (!item) throw new Error("Action item not found");
  item.status = status;
  item.updatedAt = now();
  return { ...item };
}

export async function listActionItems(): Promise<ActionItemView[]> {
  seedMockCalendarDemo();
  return calState.actionItems.slice();
}

export async function rsvpCalendarInvite(input: RsvpInviteInput): Promise<CalendarInviteView> {
  seedMockCalendarDemo();
  const key = `${input.messageId}:${input.uid}`;
  const inv = calState.invites.get(key);
  if (!inv) throw new Error("Calendar invite not found");
  inv.rsvpStatus = input.status;
  return { ...inv };
}

export async function addInviteToPigmentCalendar(
  input: AddInviteToCalendarInput,
): Promise<CalendarEventView> {
  seedMockCalendarDemo();
  const key = `${input.messageId}:${input.uid}`;
  const inv = calState.invites.get(key);
  if (!inv) throw new Error("Calendar invite not found");
  const ev = await saveCalendarEvent({
    id: inv.addedEventId,
    title: inv.summary,
    description: inv.description,
    startAt: inv.startAt,
    endAt: inv.endAt,
    allDay: inv.allDay,
    kind: "meeting",
    department: input.department,
    location: inv.location,
    recurrence: "none",
    discordUrl: null,
    attendees: inv.organizerEmail ? [inv.organizerEmail] : [],
    sourceIcsUid: inv.uid,
  });
  inv.addedEventId = ev.id;
  return ev;
}

export async function listIcsOverlays(): Promise<IcsOverlayView[]> {
  seedMockCalendarDemo();
  return calState.overlays.slice();
}

export async function saveIcsOverlay(input: SaveIcsOverlayInput): Promise<IcsOverlayView> {
  seedMockCalendarDemo();
  const existing = input.id ? calState.overlays.find((o) => o.id === input.id) : undefined;
  if (existing) {
    existing.name = input.name.trim();
    existing.url = input.url.trim();
    existing.color = input.color;
    existing.enabled = input.enabled;
    return { ...existing };
  }
  const created: IcsOverlayView = {
    id: newIcsOverlayId(),
    name: input.name.trim(),
    url: input.url.trim(),
    color: input.color || "violet",
    enabled: input.enabled,
    lastFetchedAt: null,
    lastError: null,
    eventCount: 0,
  };
  calState.overlays.push(created);
  return { ...created };
}

export async function deleteIcsOverlay(overlayId: IcsOverlayId): Promise<void> {
  calState.overlays = calState.overlays.filter((o) => o.id !== overlayId);
  calState.overlayBlocks = calState.overlayBlocks.filter((b) => b.overlayId !== overlayId);
}

export async function refreshIcsOverlay(overlayId: IcsOverlayId): Promise<IcsOverlayView> {
  seedMockCalendarDemo();
  const ov = calState.overlays.find((o) => o.id === overlayId);
  if (!ov) throw new Error("Overlay not found");
  const base = startOfTodayLocal();
  calState.overlayBlocks = calState.overlayBlocks.filter((b) => b.overlayId !== overlayId);
  calState.overlayBlocks.push({
    overlayId: ov.id,
    overlayName: ov.name,
    overlayColor: ov.color,
    uid: `refreshed-${ov.id}`,
    summary: `Busy (${ov.name})`,
    startAt: base + DAY + 13 * HOUR,
    endAt: base + DAY + 14 * HOUR,
    allDay: false,
  });
  ov.lastFetchedAt = now();
  ov.lastError = null;
  ov.eventCount = 1;
  return { ...ov };
}

export async function listOverlayBusyBlocks(
  query: CalendarRangeQuery,
): Promise<OverlayBusyBlock[]> {
  seedMockCalendarDemo();
  const enabledIds = new Set(calState.overlays.filter((o) => o.enabled).map((o) => o.id));
  return calState.overlayBlocks.filter(
    (b) => enabledIds.has(b.overlayId) && b.endAt >= query.rangeStart && b.startAt <= query.rangeEnd,
  );
}

export async function findAvailabilitySlots(
  query: AvailabilityQuery,
): Promise<AvailabilitySlot[]> {
  seedMockCalendarDemo();
  const occs = await listCalendarEvents({
    rangeStart: query.rangeStart,
    rangeEnd: query.rangeEnd,
  });
  const blocks = await listOverlayBusyBlocks({
    rangeStart: query.rangeStart,
    rangeEnd: query.rangeEnd,
  });
  const busy: Array<[number, number]> = [
    ...occs.map((o): [number, number] => [o.occurrenceStart, o.occurrenceEnd]),
    ...blocks.map((b): [number, number] => [b.startAt, b.endAt]),
  ].sort((a, b) => a[0] - b[0]);

  const slotMs = Math.max(15, query.slotMinutes) * MINUTE;
  const slots: AvailabilitySlot[] = [];
  const day = new Date(query.rangeStart);
  day.setHours(0, 0, 0, 0);
  for (let i = 0; i < 7 && slots.length < 18; i += 1) {
    const dayStart = new Date(day.getTime());
    dayStart.setHours(query.workdayStartHour, 0, 0, 0);
    const dayEnd = new Date(day.getTime());
    dayEnd.setHours(query.workdayEndHour, 0, 0, 0);
    let cursor = Math.max(dayStart.getTime(), query.rangeStart);
    while (cursor + slotMs <= dayEnd.getTime() && cursor <= query.rangeEnd && slots.length < 18) {
      const end = cursor + slotMs;
      const hit = busy.find(([bs, be]) => bs < end && be > cursor);
      if (hit) {
        cursor = Math.max(hit[1], cursor + 15 * MINUTE);
      } else {
        const sDate = new Date(cursor);
        const eDate = new Date(end);
        const dayLabel = sDate.toLocaleDateString(undefined, {
          weekday: "short",
          month: "short",
          day: "numeric",
        });
        const sTime = sDate.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
        const eTime = eDate.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
        slots.push({ startAt: cursor, endAt: end, label: `${dayLabel} · ${sTime} – ${eTime}` });
        cursor = end;
      }
    }
    day.setDate(day.getDate() + 1);
  }
  return slots;
}

export async function getDiscordWebhookStatus(): Promise<DiscordWebhookStatus> {
  return { configured: calState.discordWebhookConfigured };
}

export async function setDiscordWebhook(url: string | null): Promise<DiscordWebhookStatus> {
  const cleaned = url?.trim() ?? "";
  if (!cleaned) {
    calState.discordWebhookConfigured = false;
    return { configured: false };
  }
  if (!/^https:\/\/(ptb\.|canary\.)?discord(app)?\.com\/api\/webhooks\//i.test(cleaned)) {
    throw new Error("Discord webhook URL must start with https://discord.com/api/webhooks/");
  }
  calState.discordWebhookConfigured = true;
  return { configured: true };
}

export async function formatMeetingRecap(eventId: CalendarEventId): Promise<string> {
  const ev = calState.events.find((e) => e.id === eventId);
  if (!ev) throw new Error("Calendar event not found");
  const doc = await getMeetingDoc(eventId);
  const when = new Date(ev.startAt).toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
  const lines = [`**Meeting Recap: ${ev.title}**`];
  const meta = [when];
  if (ev.department) meta.push(`#${ev.department}`);
  if (ev.location) meta.push(`📍 ${ev.location}`);
  lines.push(`_${meta.join(" · ")}_`);
  if (doc.notesMarkdown.trim()) {
    lines.push("", "**Notes**", doc.notesMarkdown.trim());
  }
  if (doc.agenda.length > 0) {
    lines.push("", "**Agenda**");
    for (const a of doc.agenda) lines.push(`- [${a.checked ? "x" : " "}] ${a.text}`);
  }
  const allActions = [...doc.actionItems, ...doc.carryoverItems];
  if (allActions.length > 0) {
    lines.push("", "**Action Items**");
    for (const item of allActions) {
      const owner = item.assigneeHandle ? ` (@${item.assigneeHandle})` : "";
      lines.push(`- ${item.status === "done" ? "✅" : "⬜"} ${item.title}${owner}`);
    }
  }
  return lines.join("\n");
}

export async function postMeetingRecapToDiscord(
  eventId: CalendarEventId,
  customMarkdown: string | null,
): Promise<void> {
  if (!calState.discordWebhookConfigured) {
    throw new Error("Configure a Discord webhook URL first.");
  }
  if (!customMarkdown?.trim()) {
    await formatMeetingRecap(eventId);
  }
}

// ── Phase D: Action Items CRUD & Operations Workbook CSV Import/Export ──────

function parseMockHhmm(raw: string | null): [number, number] | null {
  if (!raw?.trim()) return null;
  const m = /^(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$/i.exec(raw.trim());
  if (!m) return null;
  let h = Number(m[1]);
  const mins = Number(m[2] ?? "0");
  const mer = m[3]?.toLowerCase();
  if (mer === "pm" && h !== 12) h += 12;
  if (mer === "am" && h === 12) h = 0;
  if (h < 0 || h > 23 || mins < 0 || mins > 59) return null;
  return [h, mins];
}

export async function saveActionItem(input: SaveActionItemInput): Promise<ActionItemView> {
  seedMockCalendarDemo();
  const title = input.title.trim();
  if (!title) throw new Error("Action item title is required.");
  const t = now();
  const existingIdx = input.id ? calState.actionItems.findIndex((a) => a.id === input.id) : -1;
  const existing = existingIdx >= 0 ? calState.actionItems[existingIdx] : undefined;
  let calendarEventId = existing?.calendarEventId ?? null;

  if (input.syncToCalendar && input.dueAt !== null) {
    const baseDay = new Date(input.dueAt);
    baseDay.setHours(0, 0, 0, 0);
    const startParsed = parseMockHhmm(input.startTime);
    const endParsed = parseMockHhmm(input.endTime);
    let startAt = baseDay.getTime();
    let endAt = startAt + DAY;
    let allDay = true;
    if (startParsed) {
      startAt = baseDay.getTime() + (startParsed[0] * 60 + startParsed[1]) * MINUTE;
      endAt = endParsed
        ? Math.max(startAt + HOUR, baseDay.getTime() + (endParsed[0] * 60 + endParsed[1]) * MINUTE)
        : startAt + HOUR;
      allDay = false;
    }
    const ev = await saveCalendarEvent({
      id: calendarEventId,
      title,
      description: input.milestone
        ? `Event / Milestone: ${input.milestone}\nSynced from Pigment Action Items`
        : "Synced from Pigment Action Items",
      startAt,
      endAt,
      allDay,
      kind: input.location && startParsed ? "roomSchedule" : "milestone",
      department: input.department?.trim().replace(/^#/, "").toLowerCase() || null,
      location: input.location?.trim() || null,
      recurrence: "none",
      discordUrl: null,
      attendees: input.assigneeHandle ? [normalizeHandle(input.assigneeHandle)] : [],
    });
    calendarEventId = ev.id;
  }

  const saved: ActionItemView = {
    id: existing?.id ?? newActionItemId(),
    title,
    department: input.department?.trim().replace(/^#/, "").toLowerCase() || null,
    milestone: input.milestone?.trim() || null,
    assigneeHandle: input.assigneeHandle ? normalizeHandle(input.assigneeHandle) || null : null,
    status: input.status,
    dueAt: input.dueAt,
    startTime: input.startTime?.trim() || null,
    endTime: input.endTime?.trim() || null,
    location: input.location?.trim() || null,
    costNote: input.costNote?.trim() || null,
    calendarEventId,
    sourceEventId: input.sourceEventId ?? existing?.sourceEventId ?? null,
    sourceSeriesId: input.sourceSeriesId ?? existing?.sourceSeriesId ?? null,
    sourceThreadId: input.sourceThreadId ?? existing?.sourceThreadId ?? null,
    createdBy: existing?.createdBy ?? (state.staffWorkspace.staffHandle || "reyth"),
    createdAt: existing?.createdAt ?? t,
    updatedAt: t,
  };

  if (existingIdx >= 0) calState.actionItems[existingIdx] = saved;
  else calState.actionItems.unshift(saved);
  return { ...saved };
}

export async function deleteActionItem(actionItemId: ActionItemId): Promise<void> {
  seedMockCalendarDemo();
  calState.actionItems = calState.actionItems.filter((a) => a.id !== actionItemId);
}

function parseMockCsv(raw: string): string[][] {
  const text = raw.replace(/^\uFEFF/, "");
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let inQuotes = false;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i]!;
    if (inQuotes) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i += 1;
        } else {
          inQuotes = false;
        }
      } else {
        cell += ch;
      }
    } else if (ch === '"' && cell.length === 0) {
      inQuotes = true;
    } else if (ch === ",") {
      row.push(cell);
      cell = "";
    } else if (ch === "\r" || ch === "\n") {
      if (ch === "\r" && text[i + 1] === "\n") i += 1;
      row.push(cell);
      cell = "";
      if (row.some((c) => c.trim().length > 0)) rows.push(row);
      row = [];
    } else {
      cell += ch;
    }
  }
  if (cell.length > 0 || row.length > 0) {
    row.push(cell);
    if (row.some((c) => c.trim().length > 0)) rows.push(row);
  }
  return rows;
}

function csvEscapeCell(val: string): string {
  if (/[",\r\n]/.test(val)) return `"${val.replace(/"/g, '""')}"`;
  return val;
}

export async function exportActionItemsCsv(): Promise<string> {
  seedMockCalendarDemo();
  const header = "Dept.,What,Title / Task,Assignee,Done,Date,Start Time,End Time,Where,$$$,Source";
  const lines = [header];
  for (const item of calState.actionItems) {
    const due = item.dueAt ? new Date(item.dueAt).toISOString().slice(0, 10) : "";
    const source = item.sourceThreadId ? "email" : item.sourceEventId ? "meeting" : "";
    lines.push(
      [
        item.department ?? "",
        item.milestone ?? "",
        item.title,
        item.assigneeHandle ? `@${item.assigneeHandle}` : "",
        item.status === "done" ? "TRUE" : "FALSE",
        due,
        item.startTime ?? "",
        item.endTime ?? "",
        item.location ?? "",
        item.costNote ?? "",
        source,
      ]
        .map(csvEscapeCell)
        .join(","),
    );
  }
  return `${lines.join("\r\n")}\r\n`;
}

export async function exportShowContactsCsv(): Promise<string> {
  seedMockStaffDemo();
  const lines = ["Name,Category,Organization,Tier,Emails,Notes"];
  for (const c of state.showContacts) {
    lines.push(
      [
        c.name,
        c.category,
        c.organization ?? "",
        c.tier ?? "",
        c.emails.join("; "),
        c.notes,
      ]
        .map(csvEscapeCell)
        .join(","),
    );
  }
  return `${lines.join("\r\n")}\r\n`;
}

export async function importOperationsWorkbook(
  input: ImportWorkbookInput,
): Promise<WorkbookImportReport> {
  seedMockCalendarDemo();
  seedMockStaffDemo();
  const lowerName = input.filename.toLowerCase();
  if (!lowerName.endsWith(".csv") && !lowerName.endsWith(".txt")) {
    throw new Error(
      "Browser preview supports CSV import. Use the Ferrule desktop app for native .xlsx Operations Workbook import.",
    );
  }
  const binary = atob(input.dataBase64);
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  const text = new TextDecoder("utf-8").decode(bytes);
  const rows = parseMockCsv(text);
  const report: WorkbookImportReport = {
    dryRun: input.dryRun,
    actionItemsCreated: 0,
    actionItemsUpdated: 0,
    actionItemsSkipped: 0,
    calendarEventsCreated: 0,
    contactsCreated: 0,
    contactsUpdated: 0,
    contactsSkipped: 0,
    skippedSheets: [],
    warnings: [],
    previewRows: [],
  };
  if (rows.length < 2) return report;

  const headers = rows[0]!.map((h) => h.toLowerCase().replace(/[^a-z0-9$]/g, ""));
  const findCol = (names: string[]): number => {
    for (const n of names) {
      const exact = headers.indexOf(n);
      if (exact >= 0) return exact;
    }
    for (const n of names) {
      const partial = headers.findIndex((h) => h.includes(n));
      if (partial >= 0) return partial;
    }
    return -1;
  };

  const hasEmail = findCol(["emails", "email", "emailaddress"]) >= 0;
  const hasTask = findCol(["titletask", "task", "title", "actionitem", "dept", "department"]) >= 0;
  const mode =
    input.target === "contacts" || (input.target === "auto" && hasEmail && !hasTask)
      ? "contacts"
      : "actionItems";

  const sheetName = input.filename.replace(/\.csv$/i, "");
  if (mode === "contacts") {
    const cName = findCol(["name", "contact", "fullname"]);
    const cEmail = findCol(["emails", "email", "emailaddress"]);
    const cCat = findCol(["category", "type", "role"]);
    const cOrg = findCol(["organization", "company", "studio"]);
    const cTier = findCol(["tier", "level", "booth"]);
    const cNotes = findCol(["notes", "note"]);
    for (const r of rows.slice(1)) {
      const name = (r[cName] ?? "").trim();
      const emails = (r[cEmail] ?? "")
        .split(/[;,\s]+/)
        .map((e) => e.trim().toLowerCase())
        .filter((e) => e.includes("@"));
      if (!name || emails.length === 0) {
        report.contactsSkipped += 1;
        continue;
      }
      const existing = state.showContacts.find(
        (c) =>
          c.emails.some((e) => emails.includes(e.toLowerCase())) ||
          c.name.toLowerCase() === name.toLowerCase(),
      );
      const rawCat = (r[cCat] ?? "").toLowerCase();
      const category: ContactCategory = rawCat.includes("sponsor")
        ? "sponsor"
        : rawCat.includes("vendor")
          ? "vendor"
          : rawCat.includes("instructor")
            ? "instructor"
            : rawCat.includes("vip")
              ? "vip"
              : rawCat.includes("volunteer")
                ? "volunteer"
                : rawCat.includes("venue")
                  ? "venue"
                  : "other";
      if (existing) report.contactsUpdated += 1;
      else report.contactsCreated += 1;
      if (!input.dryRun) {
        await saveShowContact({
          id: existing?.id ?? null,
          name,
          emails: Array.from(new Set([...(existing?.emails ?? []), ...emails])),
          category,
          organization: (r[cOrg] ?? "").trim() || existing?.organization || null,
          tier: (r[cTier] ?? "").trim() || existing?.tier || null,
          notes: (r[cNotes] ?? "").trim() || existing?.notes || "",
        });
      }
      if (report.previewRows.length < 50) {
        const preview: WorkbookPreviewRow = {
          kind: "contact",
          sheet: sheetName,
          title: name,
          detail: `${category} · ${emails.join(", ")}`,
          status: existing ? "update" : "create",
        };
        report.previewRows.push(preview);
      }
    }
    return report;
  }

  const cTitle = findCol(["titletask", "task", "title", "actionitem", "item"]);
  const cDept = findCol(["dept", "department", "team"]);
  const cWhat = findCol(["what", "event", "milestone"]);
  const cWho = findCol(["assignee", "owner", "who"]);
  const cDone = findCol(["done", "status", "complete"]);
  const cDate = findCol(["date", "duedate", "due"]);
  const cStart = findCol(["starttime", "start"]);
  const cEnd = findCol(["endtime", "end"]);
  const cWhere = findCol(["where", "location", "room"]);
  const cCost = findCol(["$$$", "$$", "$", "cost", "budget"]);

  for (const r of rows.slice(1)) {
    const title = (r[cTitle] ?? "").trim();
    if (!title) {
      report.actionItemsSkipped += 1;
      continue;
    }
    const dept = (r[cDept] ?? "").trim().toLowerCase().replace(/[\s_]+/g, "-") || null;
    const milestone = (r[cWhat] ?? "").trim() || null;
    const assigneeHandle = normalizeHandle(r[cWho] ?? "") || null;
    const doneRaw = (r[cDone] ?? "").trim().toLowerCase();
    const status: ActionItemStatus = ["true", "yes", "1", "x", "done", "complete"].includes(doneRaw)
      ? "done"
      : "open";
    const dateRaw = (r[cDate] ?? "").trim();
    const parsedDate = dateRaw ? Date.parse(dateRaw) : Number.NaN;
    const dueAt = Number.isFinite(parsedDate) ? parsedDate : null;
    const startTime = (r[cStart] ?? "").trim() || null;
    const endTime = (r[cEnd] ?? "").trim() || null;
    const location = (r[cWhere] ?? "").trim() || null;
    const costNote = (r[cCost] ?? "").trim() || null;

    const existing = calState.actionItems.find(
      (a) =>
        a.title.toLowerCase() === title.toLowerCase() &&
        (a.milestone ?? "").toLowerCase() === (milestone ?? "").toLowerCase() &&
        (a.department ?? "").toLowerCase() === (dept ?? "").toLowerCase(),
    );
    if (existing) report.actionItemsUpdated += 1;
    else report.actionItemsCreated += 1;
    if (input.syncToCalendar && dueAt !== null && !existing?.calendarEventId) {
      report.calendarEventsCreated += 1;
    }

    if (!input.dryRun) {
      await saveActionItem({
        id: existing?.id ?? null,
        title,
        department: dept,
        milestone,
        assigneeHandle,
        status,
        dueAt,
        startTime,
        endTime,
        location,
        costNote,
        sourceEventId: existing?.sourceEventId ?? null,
        sourceSeriesId: existing?.sourceSeriesId ?? null,
        sourceThreadId: existing?.sourceThreadId ?? null,
        syncToCalendar: input.syncToCalendar,
      });
    }
    if (report.previewRows.length < 50) {
      report.previewRows.push({
        kind: "actionItem",
        sheet: sheetName,
        title,
        detail: [dept ? `#${dept}` : "", milestone ?? "", assigneeHandle ? `@${assigneeHandle}` : ""]
          .filter(Boolean)
          .join(" · "),
        status: existing ? "update" : "create",
      });
    }
  }
  return report;
}
