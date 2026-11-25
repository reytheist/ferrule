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
