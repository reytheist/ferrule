export type Brand<T, B extends string> = T & { readonly __brand: B };

export type AccountId = Brand<string, "AccountId">;
export type IdentityId = Brand<string, "IdentityId">;
export type LabelId = Brand<string, "LabelId">;
export type ThreadId = Brand<string, "ThreadId">;
export type MessageId = Brand<string, "MessageId">;
export type AttachmentId = Brand<string, "AttachmentId">;
export type DraftId = Brand<string, "DraftId">;
export type JobId = Brand<string, "JobId">;
export type TemplateId = Brand<string, "TemplateId">;
export type SignatureId = Brand<string, "SignatureId">;
export type SavedSearchId = Brand<string, "SavedSearchId">;
export type RuleId = Brand<string, "RuleId">;
export type CalendarEventId = Brand<string, "CalendarEventId">;
export type IcsOverlayId = Brand<string, "IcsOverlayId">;
export type UnixMs = number;

export type SpaceColor =
  | "azure"
  | "berry"
  | "teal"
  | "amber"
  | "violet"
  | "emerald";

export interface AppSettings {
  developerToolsEnabled: boolean;
  onboardingCompleted: boolean;
  tourCompleted: boolean;
}

export type ProviderKind = "gmail" | "outlook" | "imap";

export interface Endpoint {
  host: string;
  port: number;
  tls: boolean;
}

export interface OAuthClient {
  clientId: string;
  hasClientSecret: boolean;
}

export type ProviderAuth =
  | { kind: "gmailAppPassword"; address: string }
  | { kind: "gmailOAuth"; address: string; client: OAuthClient }
  | { kind: "outlookOAuth"; address: string; clientId: string; tenant: string }
  | {
      kind: "imapPassword";
      address: string;
      username: string;
      imap: Endpoint;
      smtp: Endpoint;
    };

export type Scope =
  | { kind: "unified" }
  | { kind: "account"; accountId: AccountId };

export type InboxCategory = "primary" | "receipts" | "newsletters" | "all";

export type MailboxView =
  | { kind: "inbox"; category: InboxCategory }
  | { kind: "snoozed" }
  | { kind: "followUps" }
  | { kind: "scheduled" }
  | { kind: "sent" }
  | { kind: "drafts" }
  | { kind: "starred" }
  | { kind: "archive" }
  | { kind: "spam" }
  | { kind: "trash" }
  | { kind: "label"; labelId: LabelId }
  | { kind: "search"; text: string };

export type SyncState =
  | { kind: "idle"; lastOk: UnixMs | null }
  | { kind: "connecting" }
  | { kind: "syncing"; done: number; total: number | null }
  | { kind: "backoff"; retryAt: UnixMs; attempt: number; reason?: string | null }
  | { kind: "blocked"; reason: string };

export interface IdentityView {
  id: IdentityId;
  address: string;
  displayName: string | null;
  isPrimary: boolean;
}

export interface AccountView {
  id: AccountId;
  displayName: string;
  provider: ProviderKind;
  color: SpaceColor;
  email: string;
  identities: IdentityView[];
  sync: SyncState;
  unread: number;
  position: number;
}

export interface NewAccountInput {
  displayName: string;
  color: SpaceColor;
  auth: ProviderAuth;
  secret: string | null;
}

export interface OAuthSignInInput {
  provider: ProviderKind;
  clientId: string;
  clientSecret: string | null;
  tenant: string | null;
  displayName: string | null;
  color: SpaceColor | null;
}

export interface UpdateAccountInput {
  accountId: AccountId;
  displayName: string;
  color: SpaceColor;
}

export interface SavedOAuthClients {
  gmailClientId: string | null;
  outlookClientId: string | null;
  outlookTenant: string | null;
  gmailBuiltIn: boolean;
  outlookBuiltIn: boolean;
}

export interface ThreadQuery {
  scope: Scope;
  view: MailboxView;
  limit: number;
  offset: number;
}

export interface LabelBadge {
  id: LabelId;
  name: string;
}

export interface ThreadSummary {
  id: ThreadId;
  accountId: AccountId;
  accountLabel: string;
  accountColor: SpaceColor;
  accountEmail: string;
  subject: string;
  snippet: string;
  senderName: string;
  senderEmail: string;
  lastAt: UnixMs;
  unreadCount: number;
  messageCount: number;
  hasAttachments: boolean;
  starred: boolean;
  category: InboxCategory;
  snoozedUntil: UnixMs | null;
  followUpDue: UnixMs | null;
  trackersBlocked: number;
  labels: LabelBadge[];
  isVip?: boolean;
}

export interface ThreadPage {
  items: ThreadSummary[];
  total: number;
}

export type RemoteContent = "blockAll" | "allowImagesStripTrackers";

export type RemoteImagesStatus =
  | { kind: "none" }
  | { kind: "blocked"; count: number }
  | { kind: "allowed" };

export type TrackerKind =
  | "pixel1x1"
  | "hiddenImage"
  | "knownDomain"
  | "cssBeacon";

export interface TrackerHit {
  host: string;
  kind: TrackerKind;
}

export interface PrivacyReport {
  remoteImages: RemoteImagesStatus;
  trackers: TrackerHit[];
}

export interface AttachmentMeta {
  id: AttachmentId;
  filename: string;
  mime: string;
  size: number;
  cached: boolean;
  isInline?: boolean;
  contentId?: string | null;
}

export interface AttachmentData {
  id: AttachmentId;
  filename: string;
  mime: string;
  size: number;
  dataBase64: string;
}

export interface DraftAttachmentInput {
  filename: string;
  mime: string;
  dataBase64: string;
}

export interface DraftAttachmentView {
  id: AttachmentId;
  filename: string;
  mime: string;
  size: number;
}

export interface UnsubscribeInfo {
  oneClick: boolean;
  target: string;
}

export interface MessageView {
  id: MessageId;
  fromName: string | null;
  fromEmail: string;
  to: string[];
  cc: string[];
  date: UnixMs;
  snippet: string;
  bodySrcdoc: string;
  bodyText: string;
  privacy: PrivacyReport;
  attachments: AttachmentMeta[];
  unsubscribe: UnsubscribeInfo | null;
  invites?: CalendarInviteView[];
}

export interface ThreadView {
  id: ThreadId;
  accountId: AccountId;
  accountLabel: string;
  accountColor: SpaceColor;
  accountEmail: string;
  subject: string;
  starred: boolean;
  snoozedUntil: UnixMs | null;
  followUpDue: UnixMs | null;
  messages: MessageView[];
}

export type MailAction =
  | { kind: "archive" }
  | { kind: "unarchive" }
  | { kind: "trash" }
  | { kind: "reportSpam" }
  | { kind: "markRead"; read: boolean }
  | { kind: "star"; on: boolean };

export interface ActionOutcome {
  updatedIds: ThreadId[];
  undoAction: MailAction | null;
}

export type DraftOrigin =
  | { kind: "human" }
  | { kind: "agent"; agentId: string; rationale: string };

export type DraftState =
  | { kind: "editing" }
  | { kind: "pendingApproval" }
  | { kind: "queued"; jobId: JobId }
  | { kind: "sent"; at: UnixMs }
  | { kind: "rejected"; reason: string };

export interface DraftView {
  id: DraftId;
  accountId: AccountId;
  spaceColor: SpaceColor;
  fromIdentity: IdentityView;
  fromLocked: boolean;
  replyToThread: ThreadId | null;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  bodyText: string;
  bodyHtml?: string | null;
  attachments?: DraftAttachmentView[];
  rev: number;
  origin: DraftOrigin;
  state: DraftState;
}

export interface CreateDraftInput {
  accountId: AccountId;
  replyToThread: ThreadId | null;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  bodyText: string;
  bodyHtml?: string | null;
  attachments?: DraftAttachmentInput[];
}

export type SendMode =
  | { kind: "undoable"; undoSecs: number }
  | { kind: "scheduled"; sendAt: UnixMs };

export interface SendReceipt {
  jobId: JobId;
  draftId: DraftId;
  dueAt: UnixMs;
  scheduled: boolean;
}

export type UndoOutcome = "cancelled" | "tooLate";

export interface JobView {
  id: JobId;
  kind: string;
  accountId: AccountId;
  threadId: ThreadId | null;
  dueAt: UnixMs;
  state: string;
  summary: string;
}

export interface TemplateView {
  id: TemplateId;
  name: string;
  shortcut: string | null;
  subject: string;
  bodyText: string;
}

export interface SaveTemplateInput {
  id: TemplateId | null;
  name: string;
  shortcut: string | null;
  subject: string;
  bodyText: string;
}

export interface SignatureView {
  id: SignatureId;
  accountId: AccountId;
  name: string;
  bodyHtml: string;
  bodyText: string;
  isDefault: boolean;
}

export interface SaveSignatureInput {
  id: SignatureId | null;
  accountId: AccountId;
  name: string;
  bodyHtml: string;
  bodyText: string;
  isDefault: boolean;
}

export interface MailboxCounts {
  inbox: number;
  snoozed: number;
  followUps: number;
  scheduled: number;
  primary: number;
  receipts: number;
  newsletters: number;
  vip?: number;
  screener?: number;
}

// ── Phase 3: Saved Searches, Screener, VIP, Labels & Rules ──────────────────

export interface SavedSearchView {
  id: SavedSearchId;
  name: string;
  query: string;
}

export interface SaveSavedSearchInput {
  id: SavedSearchId | null;
  name: string;
  query: string;
}

export type ScreenerDecision = "inbox" | "paperTrail" | "feed" | "blocked";

export interface ScreenerSenderView {
  address: string;
  displayName: string;
  sampleSubject: string;
  threadCount: number;
  latestAt: UnixMs;
}

export interface VipSenderView {
  address: string;
  displayName: string | null;
  addedAt: UnixMs;
}

export interface LabelView {
  id: LabelId;
  name: string;
  color: SpaceColor;
  threadCount: number;
}

export interface SaveLabelInput {
  id: LabelId | null;
  name: string;
  color: SpaceColor;
}

export type RuleCondition =
  | { kind: "fromContains"; value: string }
  | { kind: "domainEquals"; value: string }
  | { kind: "subjectContains"; value: string }
  | { kind: "bodyContains"; value: string }
  | { kind: "hasAttachment" };

export type RuleAction =
  | { kind: "applyLabel"; labelId: LabelId }
  | { kind: "archive" }
  | { kind: "trash" }
  | { kind: "markRead" }
  | { kind: "star" }
  | { kind: "setCategory"; category: InboxCategory };

export interface MailRuleView {
  id: RuleId;
  name: string;
  enabled: boolean;
  matchAll: boolean;
  conditions: RuleCondition[];
  actions: RuleAction[];
}

export interface SaveMailRuleInput {
  id: RuleId | null;
  name: string;
  enabled: boolean;
  matchAll: boolean;
  conditions: RuleCondition[];
  actions: RuleAction[];
}

// ── Personal Calendar ───────────────────────────────────────────────────────

export type CalendarEventKind =
  | "meeting"
  | "milestone"
  | "deadline"
  | "roomSchedule"
  | "showEvent";

export type RecurrenceRule = "none" | "weekly" | "biweekly" | "monthly";

export interface CalendarEventView {
  id: CalendarEventId;
  seriesId: string | null;
  title: string;
  description: string;
  startAt: UnixMs;
  endAt: UnixMs;
  allDay: boolean;
  kind: CalendarEventKind;
  department: string | null;
  location: string | null;
  recurrence: RecurrenceRule;
  discordUrl: string | null;
  attendees: string[];
  sourceIcsUid: string | null;
  createdBy: string;
  updatedAt: UnixMs;
}

export interface SaveCalendarEventInput {
  id: CalendarEventId | null;
  seriesId?: string | null;
  title: string;
  description: string;
  startAt: UnixMs;
  endAt: UnixMs;
  allDay: boolean;
  kind: CalendarEventKind;
  department: string | null;
  location: string | null;
  recurrence: RecurrenceRule;
  discordUrl: string | null;
  attendees: string[];
  sourceIcsUid?: string | null;
}

export interface CalendarOccurrence {
  event: CalendarEventView;
  occurrenceStart: UnixMs;
  occurrenceEnd: UnixMs;
  isRecurringInstance: boolean;
}

export interface CalendarRangeQuery {
  rangeStart: UnixMs;
  rangeEnd: UnixMs;
}

export type RsvpStatus = "needsAction" | "accepted" | "tentative" | "declined";

export interface CalendarInviteView {
  messageId: MessageId;
  uid: string;
  summary: string;
  description: string;
  location: string | null;
  organizerName: string | null;
  organizerEmail: string;
  startAt: UnixMs;
  endAt: UnixMs;
  allDay: boolean;
  rsvpStatus: RsvpStatus;
  addedEventId: CalendarEventId | null;
}

export interface RsvpInviteInput {
  messageId: MessageId;
  uid: string;
  status: RsvpStatus;
}

export interface AddInviteToCalendarInput {
  messageId: MessageId;
  uid: string;
  department: string | null;
}

export interface IcsOverlayView {
  id: IcsOverlayId;
  name: string;
  url: string;
  color: string;
  enabled: boolean;
  lastFetchedAt: UnixMs | null;
  lastError: string | null;
  eventCount: number;
}

export interface SaveIcsOverlayInput {
  id: IcsOverlayId | null;
  name: string;
  url: string;
  color: string;
  enabled: boolean;
}

export interface OverlayBusyBlock {
  overlayId: IcsOverlayId;
  overlayName: string;
  overlayColor: string;
  uid: string;
  summary: string;
  startAt: UnixMs;
  endAt: UnixMs;
  allDay: boolean;
}

export interface AvailabilityQuery {
  rangeStart: UnixMs;
  rangeEnd: UnixMs;
  slotMinutes: number;
  workdayStartHour: number;
  workdayEndHour: number;
}

export interface AvailabilitySlot {
  startAt: UnixMs;
  endAt: UnixMs;
  label: string;
}

// ── Core Domain Events (mail://event) ───────────────────────────────────────

export type CoreEvent =
  | { kind: "mailChanged"; accountId: AccountId; threadIds: ThreadId[] }
  | { kind: "syncChanged"; accountId: AccountId; state: SyncState }
  | { kind: "jobChanged"; job: JobView }
  | { kind: "draftChanged"; draftId: DraftId }
  | { kind: "attention"; alert: { kind: string } }
  | { kind: "calendarChanged" }
  | { kind: "resync" };
