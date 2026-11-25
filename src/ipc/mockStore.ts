// Stateful in-memory backend for the Vite browser preview. Every function here
// mirrors one Tauri command from src/ipc/core.ts so the full UI stays
// interactive without the Rust mail-core.

import {
  newAccountId,
  newAttachmentId,
  newDraftId,
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
  AttachmentMeta,
  CreateDraftInput,
  DraftId,
  DraftView,
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
  PrivacyReport,
  ProviderKind,
  RemoteContent,
  RemoteImagesStatus,
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
}

function emptyState(): MockState {
  return { accounts: [], threads: [], drafts: new Map(), jobs: [], templates: [] };
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
      return thread.starred && !thread.trashed;
    case "archive":
      return thread.archived && !thread.trashed;
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
        break;
      case "unarchive":
        thread.archived = false;
        break;
      case "trash":
        thread.trashed = true;
        thread.archived = false;
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
