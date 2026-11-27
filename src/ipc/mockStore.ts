// Stateful in-memory backend for the Vite browser preview. Every function here
// mirrors one Tauri command from src/ipc/core.ts so the full UI stays
// interactive without the Rust mail-core.

import {
  newAccountId,
  newAttachmentId,
  newCalendarEventId,
  newDraftId,
  newIcsOverlayId,
  newIdentityId,
  newJobId,
  newLabelId,
  newMessageId,
  newTemplateId,
  newThreadId,
} from "../types/ids";
import type {
  AccountId,
  AccountView,
  ActionOutcome,
  AddInviteToCalendarInput,
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
  CreateDraftInput,
  DraftId,
  DraftView,
  IcsOverlayId,
  IcsOverlayView,
  IdentityView,
  InboxCategory,
  JobId,
  JobView,
  LabelBadge,
  MailAction,
  MailboxCounts,
  MailboxView,
  MessageId,
  MessageView,
  NewAccountInput,
  OAuthSignInInput,
  OverlayBusyBlock,
  PrivacyReport,
  ProviderKind,
  RecurrenceRule,
  RemoteContent,
  RemoteImagesStatus,
  RsvpInviteInput,
  SaveCalendarEventInput,
  SaveIcsOverlayInput,
  SaveTemplateInput,
  SavedOAuthClients,
  Scope,
  SendMode,
  SendReceipt,
  SpaceColor,
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
    `html{box-sizing:border-box;padding:14px 16px!important;}` +
    `body{margin:0;background:${bg};color:${ink};` +
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
    gmailBuiltIn: true,
    outlookBuiltIn: false,
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

// ── Personal Calendar Mock Backend ──────────────────────────────────────────

interface MockCalendarState {
  seeded: boolean;
  events: CalendarEventView[];
  invites: Map<string, CalendarInviteView>;
  overlays: IcsOverlayView[];
  overlayBlocks: OverlayBusyBlock[];
}

const calState: MockCalendarState = {
  seeded: false,
  events: [],
  invites: new Map(),
  overlays: [],
  overlayBlocks: [],
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
  return Array.from(calState.invites.values()).filter((inv) => inv.messageId === message.id);
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
    input.seriesId?.trim() || (input.recurrence !== "none" ? `series-${id}` : null);
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
    createdBy: "me",
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
  if (!trimmed) throw new Error('Enter an event like "Dentist Thu 2pm @Downtown"');
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
    title: titleTokens.join(" ") || "Calendar Event",
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
}

export async function rsvpCalendarInvite(input: RsvpInviteInput): Promise<CalendarInviteView> {
  seedMockCalendarDemo();
  const key = `${input.messageId}:${input.uid}`;
  const inv = calState.invites.get(key);
  if (!inv) throw new Error("Calendar invite not found");
  inv.rsvpStatus = input.status;
  return { ...inv };
}

export async function addInviteToCalendar(
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

// ── Mock Attachments & Named Signatures ─────────────────────────────────────

const mockSignatures: import("../types/mail").SignatureView[] = [];
let mockSigSeq = 1;

export async function downloadAttachment(
  attachmentId: import("../types/mail").AttachmentId,
): Promise<import("../types/mail").AttachmentData> {
  for (const thread of state.threads) {
    for (const msg of thread.messages) {
      const hit = msg.attachments.find((a) => a.id === attachmentId);
      if (hit) {
        const sample = `Preview content for ${hit.filename} (${hit.mime})`;
        return {
          id: hit.id,
          filename: hit.filename,
          mime: hit.mime,
          size: sample.length,
          dataBase64: btoa(sample),
        };
      }
    }
  }
  throw new Error("Attachment not found");
}

export async function listSignatures(
  accountId: import("../types/mail").AccountId | null,
): Promise<import("../types/mail").SignatureView[]> {
  return accountId
    ? mockSignatures.filter((s) => s.accountId === accountId)
    : [...mockSignatures];
}

export async function saveSignature(
  input: import("../types/mail").SaveSignatureInput,
): Promise<import("../types/mail").SignatureView> {
  if (input.isDefault) {
    for (const sig of mockSignatures) {
      if (sig.accountId === input.accountId) sig.isDefault = false;
    }
  }
  if (input.id) {
    const existing = mockSignatures.find((s) => s.id === input.id);
    if (existing) {
      existing.name = input.name.trim();
      existing.bodyHtml = input.bodyHtml;
      existing.bodyText = input.bodyText;
      existing.isDefault = input.isDefault;
      return { ...existing };
    }
  }
  const created: import("../types/mail").SignatureView = {
    id: `sig-${mockSigSeq++}` as import("../types/mail").SignatureId,
    accountId: input.accountId,
    name: input.name.trim(),
    bodyHtml: input.bodyHtml,
    bodyText: input.bodyText,
    isDefault: input.isDefault,
  };
  mockSignatures.push(created);
  return { ...created };
}

export async function deleteSignature(
  signatureId: import("../types/mail").SignatureId,
): Promise<void> {
  const idx = mockSignatures.findIndex((s) => s.id === signatureId);
  if (idx >= 0) mockSignatures.splice(idx, 1);
}

// ── Mock Phase 3: Saved Searches, Screener, VIP, Labels & Rules ─────────────

const mockSavedSearches: import("../types/mail").SavedSearchView[] = [];
const mockScreened = new Map<string, import("../types/mail").ScreenerDecision>();
const mockVips: import("../types/mail").VipSenderView[] = [];
const mockLabels: import("../types/mail").LabelView[] = [];
const mockRules: import("../types/mail").MailRuleView[] = [];
let mockPhase3Seq = 1;

export async function listSavedSearches(): Promise<import("../types/mail").SavedSearchView[]> {
  return [...mockSavedSearches];
}

export async function saveSavedSearch(
  input: import("../types/mail").SaveSavedSearchInput,
): Promise<import("../types/mail").SavedSearchView> {
  const existing = input.id ? mockSavedSearches.find((s) => s.id === input.id) : undefined;
  if (existing) {
    existing.name = input.name.trim();
    existing.query = input.query.trim();
    return { ...existing };
  }
  const created: import("../types/mail").SavedSearchView = {
    id: `ss-${mockPhase3Seq++}` as import("../types/mail").SavedSearchId,
    name: input.name.trim(),
    query: input.query.trim(),
  };
  mockSavedSearches.push(created);
  return { ...created };
}

export async function deleteSavedSearch(
  id: import("../types/mail").SavedSearchId,
): Promise<void> {
  const idx = mockSavedSearches.findIndex((s) => s.id === id);
  if (idx >= 0) mockSavedSearches.splice(idx, 1);
}

export async function listScreenerSenders(): Promise<import("../types/mail").ScreenerSenderView[]> {
  const byAddr = new Map<string, import("../types/mail").ScreenerSenderView>();
  for (const t of state.threads) {
    const addr = t.senderEmail.toLowerCase();
    if (!addr || mockScreened.has(addr)) continue;
    const prev = byAddr.get(addr);
    if (!prev) {
      byAddr.set(addr, {
        address: addr,
        displayName: t.senderName,
        sampleSubject: t.subject,
        threadCount: 1,
        latestAt: t.messages[t.messages.length - 1]?.date ?? now(),
      });
    } else {
      prev.threadCount += 1;
    }
  }
  return Array.from(byAddr.values());
}

export async function screenSender(
  address: string,
  _displayName: string | null,
  decision: import("../types/mail").ScreenerDecision,
): Promise<number> {
  const addr = address.toLowerCase();
  mockScreened.set(addr, decision);
  let count = 0;
  for (const t of state.threads) {
    if (t.senderEmail.toLowerCase() === addr) {
      if (decision === "paperTrail") t.category = "receipts";
      else if (decision === "feed") t.category = "newsletters";
      else if (decision === "blocked") t.trashed = true;
      else t.category = "primary";
      count += 1;
    }
  }
  return count;
}

export async function listVipSenders(): Promise<import("../types/mail").VipSenderView[]> {
  return [...mockVips];
}

export async function addVipSender(
  address: string,
  displayName: string | null,
): Promise<import("../types/mail").VipSenderView> {
  const addr = address.toLowerCase();
  const existing = mockVips.find((v) => v.address === addr);
  if (existing) return { ...existing };
  const created: import("../types/mail").VipSenderView = {
    address: addr,
    displayName,
    addedAt: now(),
  };
  mockVips.push(created);
  return { ...created };
}

export async function removeVipSender(address: string): Promise<void> {
  const idx = mockVips.findIndex((v) => v.address === address.toLowerCase());
  if (idx >= 0) mockVips.splice(idx, 1);
}

export async function listLabels(): Promise<import("../types/mail").LabelView[]> {
  return [...mockLabels];
}

export async function saveLabel(
  input: import("../types/mail").SaveLabelInput,
): Promise<import("../types/mail").LabelView> {
  const existing = input.id ? mockLabels.find((l) => l.id === input.id) : undefined;
  if (existing) {
    existing.name = input.name.trim();
    existing.color = input.color;
    return { ...existing };
  }
  const created: import("../types/mail").LabelView = {
    id: `lbl-${mockPhase3Seq++}` as import("../types/mail").LabelId,
    name: input.name.trim(),
    color: input.color,
    threadCount: 0,
  };
  mockLabels.push(created);
  return { ...created };
}

export async function deleteLabel(id: import("../types/mail").LabelId): Promise<void> {
  const idx = mockLabels.findIndex((l) => l.id === id);
  if (idx >= 0) mockLabels.splice(idx, 1);
}

export async function labelThreads(
  labelId: import("../types/mail").LabelId,
  threadIds: import("../types/mail").ThreadId[],
  add: boolean,
): Promise<void> {
  const lbl = mockLabels.find((l) => l.id === labelId);
  if (!lbl) return;
  for (const tid of threadIds) {
    const t = state.threads.find((th) => th.id === tid);
    if (!t) continue;
    if (add && !t.labels.some((l) => l.id === labelId)) {
      t.labels.push({ id: lbl.id, name: lbl.name });
    } else if (!add) {
      t.labels = t.labels.filter((l) => l.id !== labelId);
    }
  }
}

export async function listRules(): Promise<import("../types/mail").MailRuleView[]> {
  return [...mockRules];
}

export async function saveRule(
  input: import("../types/mail").SaveMailRuleInput,
): Promise<import("../types/mail").MailRuleView> {
  const existing = input.id ? mockRules.find((r) => r.id === input.id) : undefined;
  if (existing) {
    Object.assign(existing, input, { name: input.name.trim() });
    return { ...existing };
  }
  const created: import("../types/mail").MailRuleView = {
    id: `rule-${mockPhase3Seq++}` as import("../types/mail").RuleId,
    name: input.name.trim(),
    enabled: input.enabled,
    matchAll: input.matchAll,
    conditions: input.conditions,
    actions: input.actions,
  };
  mockRules.push(created);
  return { ...created };
}

export async function deleteRule(id: import("../types/mail").RuleId): Promise<void> {
  const idx = mockRules.findIndex((r) => r.id === id);
  if (idx >= 0) mockRules.splice(idx, 1);
}

export async function runRuleNow(id: import("../types/mail").RuleId): Promise<number> {
  return mockRules.some((r) => r.id === id) ? 0 : 0;
}
