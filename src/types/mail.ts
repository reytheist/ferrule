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
export type StaffMemberId = Brand<string, "StaffMemberId">;
export type SharedThreadId = Brand<string, "SharedThreadId">;
export type CommentId = Brand<string, "CommentId">;
export type SharedTemplateId = Brand<string, "SharedTemplateId">;
export type ContactId = Brand<string, "ContactId">;
export type PinnedLinkId = Brand<string, "PinnedLinkId">;
export type CalendarEventId = Brand<string, "CalendarEventId">;
export type ActionItemId = Brand<string, "ActionItemId">;
export type IcsOverlayId = Brand<string, "IcsOverlayId">;
export type UnixMs = number;

export type SpaceColor =
  | "azure"
  | "berry"
  | "teal"
  | "amber"
  | "violet"
  | "emerald";

export type SpaceKind = "personal" | "pigmentStaff";

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
  spaceKind: SpaceKind;
  email: string;
  identities: IdentityView[];
  sync: SyncState;
  unread: number;
  position: number;
}

export interface NewAccountInput {
  displayName: string;
  color: SpaceColor;
  spaceKind?: SpaceKind;
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
  spaceKind?: SpaceKind;
}

export interface UpdateAccountInput {
  accountId: AccountId;
  displayName: string;
  color: SpaceColor;
  spaceKind?: SpaceKind | null;
}

export interface SavedOAuthClients {
  gmailClientId: string | null;
  outlookClientId: string | null;
  outlookTenant: string | null;
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

export interface MailboxCounts {
  inbox: number;
  snoozed: number;
  followUps: number;
  scheduled: number;
  primary: number;
  receipts: number;
  newsletters: number;
}

// ── Phase B: Staff Sync & Email Collaboration ───────────────────────────────

export type StaffSyncStatus = "disconnected" | "connected" | "offline" | "error";

export interface StaffWorkspaceView {
  connected: boolean;
  serverUrl: string;
  staffName: string;
  staffHandle: string;
  staffEmail: string;
  staffRole: string;
  connectedAt: UnixMs | null;
  lastSyncAt: UnixMs | null;
  status: StaffSyncStatus;
  lastError: string | null;
  pendingOutboxCount: number;
}

export interface ConnectStaffWorkspaceInput {
  serverUrl: string;
  token: string;
  staffName: string;
  staffHandle: string;
  staffEmail: string;
  staffRole: string;
}

export interface StaffMemberView {
  id: StaffMemberId;
  name: string;
  role: string;
  handle: string;
  email: string;
  department: string | null;
  updatedAt: UnixMs;
}

export type ShareVisibility = "allStaff" | "specific";
export type HandoffStatus = "open" | "waiting" | "done";
export type DraftLockState = "unlocked" | "editing" | "reviewRequested" | "approved";

export interface SharedMessageSnapshot {
  id: string;
  fromName: string | null;
  fromEmail: string;
  to: string[];
  cc: string[];
  date: UnixMs;
  snippet: string;
  bodyText: string;
  bodySrcdoc: string;
}

export interface SharedCommentView {
  id: CommentId;
  sharedThreadId: SharedThreadId;
  authorHandle: string;
  authorName: string;
  bodyText: string;
  mentions: string[];
  createdAt: UnixMs;
}

export interface SharedDraftView {
  sharedThreadId: SharedThreadId;
  subject: string;
  to: string[];
  cc: string[];
  bodyText: string;
  editorHandle: string | null;
  editorName: string | null;
  lockState: DraftLockState;
  reviewRequestedBy: string | null;
  approvedBy: string | null;
  approvedAt: UnixMs | null;
  rev: number;
  updatedAt: UnixMs;
}

export interface SharedThreadSummary {
  id: SharedThreadId;
  sourceThreadId: ThreadId | null;
  sourceAccountId: AccountId | null;
  ownerEmail: string;
  ownerName: string;
  subject: string;
  snippet: string;
  participants: string[];
  messageCount: number;
  visibility: ShareVisibility;
  invitedHandles: string[];
  assigneeHandle: string | null;
  status: HandoffStatus;
  commentCount: number;
  draftLockState: DraftLockState;
  draftEditorName: string | null;
  sharedAt: UnixMs;
  updatedAt: UnixMs;
}

export interface SharedThreadView {
  id: SharedThreadId;
  sourceThreadId: ThreadId | null;
  sourceAccountId: AccountId | null;
  ownerEmail: string;
  ownerName: string;
  subject: string;
  snippet: string;
  participants: string[];
  messages: SharedMessageSnapshot[];
  visibility: ShareVisibility;
  invitedHandles: string[];
  assigneeHandle: string | null;
  status: HandoffStatus;
  comments: SharedCommentView[];
  draft: SharedDraftView | null;
  sharedAt: UnixMs;
  updatedAt: UnixMs;
}

export interface ShareThreadInput {
  threadId: ThreadId;
  visibility: ShareVisibility;
  invitedHandles: string[];
  assigneeHandle: string | null;
  initialNote: string | null;
}

export interface UpdateHandoffInput {
  sharedThreadId: SharedThreadId;
  assigneeHandle: string | null;
  status: HandoffStatus;
}

export interface AddSharedCommentInput {
  sharedThreadId: SharedThreadId;
  bodyText: string;
}

export type DraftLockAction =
  | { kind: "claim" }
  | { kind: "takeOver" }
  | { kind: "save"; subject: string; to: string[]; cc: string[]; bodyText: string }
  | { kind: "requestReview"; reviewerHandles: string[]; note: string | null }
  | { kind: "approve" }
  | { kind: "release" };

export interface UpdateSharedDraftInput {
  sharedThreadId: SharedThreadId;
  action: DraftLockAction;
}

export interface SharedTemplateView {
  id: SharedTemplateId;
  name: string;
  shortcut: string | null;
  category: string;
  subject: string;
  bodyText: string;
  updatedBy: string;
  updatedAt: UnixMs;
}

export interface SaveSharedTemplateInput {
  id: SharedTemplateId | null;
  name: string;
  shortcut: string | null;
  category: string;
  subject: string;
  bodyText: string;
}

export type ContactCategory =
  | "sponsor"
  | "vendor"
  | "instructor"
  | "vip"
  | "volunteer"
  | "venue"
  | "other";

export interface ShowContactView {
  id: ContactId;
  name: string;
  emails: string[];
  category: ContactCategory;
  organization: string | null;
  tier: string | null;
  notes: string;
  updatedAt: UnixMs;
}

export interface SaveShowContactInput {
  id: ContactId | null;
  name: string;
  emails: string[];
  category: ContactCategory;
  organization: string | null;
  tier: string | null;
  notes: string;
}

export interface PinnedLinkView {
  id: PinnedLinkId;
  title: string;
  url: string;
  category: string;
  description: string | null;
  position: number;
  updatedAt: UnixMs;
}

export interface SavePinnedLinkInput {
  id: PinnedLinkId | null;
  title: string;
  url: string;
  category: string;
  description: string | null;
}

// ── Phase C: Calendar & Meetings ────────────────────────────────────────────

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

export interface AgendaItem {
  id: string;
  text: string;
  checked: boolean;
  addedBy: string;
  promotedActionId: ActionItemId | null;
}

export type ActionItemStatus = "open" | "done";

export interface ActionItemView {
  id: ActionItemId;
  title: string;
  department: string | null;
  milestone: string | null;
  assigneeHandle: string | null;
  status: ActionItemStatus;
  dueAt: UnixMs | null;
  startTime: string | null;
  endTime: string | null;
  location: string | null;
  costNote: string | null;
  calendarEventId: CalendarEventId | null;
  sourceEventId: CalendarEventId | null;
  sourceSeriesId: string | null;
  sourceThreadId: ThreadId | null;
  createdBy: string;
  createdAt: UnixMs;
  updatedAt: UnixMs;
}

export interface SaveActionItemInput {
  id: ActionItemId | null;
  title: string;
  department: string | null;
  milestone: string | null;
  assigneeHandle: string | null;
  status: ActionItemStatus;
  dueAt: UnixMs | null;
  startTime: string | null;
  endTime: string | null;
  location: string | null;
  costNote: string | null;
  sourceEventId: CalendarEventId | null;
  sourceSeriesId: string | null;
  sourceThreadId: ThreadId | null;
  syncToCalendar: boolean;
}

export type WorkbookImportTarget = "auto" | "actionItems" | "contacts";

export interface ImportWorkbookInput {
  filename: string;
  dataBase64: string;
  target: WorkbookImportTarget;
  syncToCalendar: boolean;
  dryRun: boolean;
}

export type PreviewRowKind = "actionItem" | "contact";
export type PreviewRowStatus = "create" | "update" | "skip";

export interface WorkbookPreviewRow {
  kind: PreviewRowKind;
  sheet: string;
  title: string;
  detail: string;
  status: PreviewRowStatus;
}

export interface WorkbookImportReport {
  dryRun: boolean;
  actionItemsCreated: number;
  actionItemsUpdated: number;
  actionItemsSkipped: number;
  calendarEventsCreated: number;
  contactsCreated: number;
  contactsUpdated: number;
  contactsSkipped: number;
  skippedSheets: string[];
  warnings: string[];
  previewRows: WorkbookPreviewRow[];
}

export interface MeetingDocView {
  eventId: CalendarEventId;
  seriesId: string | null;
  agenda: AgendaItem[];
  notesMarkdown: string;
  notesEditorHandle: string | null;
  notesEditorName: string | null;
  rev: number;
  updatedAt: UnixMs;
  actionItems: ActionItemView[];
  carryoverItems: ActionItemView[];
}

export type MeetingDocAction =
  | { kind: "updateAgenda"; agenda: AgendaItem[] }
  | { kind: "claimNotesLock" }
  | { kind: "saveNotes"; notesMarkdown: string; releaseLock: boolean }
  | { kind: "releaseNotesLock" };

export interface UpdateMeetingDocInput {
  eventId: CalendarEventId;
  action: MeetingDocAction;
}

export interface PromoteActionItemInput {
  eventId: CalendarEventId;
  agendaItemId: string | null;
  title: string;
  department: string | null;
  assigneeHandle: string | null;
  dueAt: UnixMs | null;
}

export interface DiscordWebhookStatus {
  configured: boolean;
}
