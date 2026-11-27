use serde::{Deserialize, Serialize};

use crate::auth::{ProviderAuth, ProviderKind};
use crate::ids::{
    AccountId, AttachmentId, DraftId, IdentityId, JobId, LabelId, MessageId, RuleId, SavedSearchId,
    SignatureId, TemplateId, ThreadId, UnixMs,
};
use crate::rules::{RuleAction, RuleCondition};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpaceColor {
    Azure,
    Berry,
    Teal,
    Amber,
    Violet,
    Emerald,
}

impl SpaceColor {
    pub const ALL: [Self; 6] = [
        Self::Azure,
        Self::Berry,
        Self::Teal,
        Self::Amber,
        Self::Violet,
        Self::Emerald,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Azure => "azure",
            Self::Berry => "berry",
            Self::Teal => "teal",
            Self::Amber => "amber",
            Self::Violet => "violet",
            Self::Emerald => "emerald",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "berry" => Self::Berry,
            "teal" => Self::Teal,
            "amber" => Self::Amber,
            "violet" => Self::Violet,
            "emerald" => Self::Emerald,
            _ => Self::Azure,
        }
    }

    /// Pick the first unused color in palette order, or the least-used color once all
    /// six palette slots are taken.
    pub fn pick_unique(used: &[Self]) -> Self {
        let mut best = Self::ALL[0];
        let mut best_count = usize::MAX;
        for candidate in Self::ALL {
            let count = used.iter().filter(|&&c| c == candidate).count();
            if count == 0 {
                return candidate;
            }
            if count < best_count {
                best = candidate;
                best_count = count;
            }
        }
        best
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Scope {
    Unified,
    Account { account_id: AccountId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InboxCategory {
    Primary,
    Receipts,
    Newsletters,
    All,
}

impl InboxCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Receipts => "receipts",
            Self::Newsletters => "newsletters",
            Self::All => "all",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "receipts" => Self::Receipts,
            "newsletters" => Self::Newsletters,
            _ => Self::Primary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MailboxView {
    Inbox { category: InboxCategory },
    Snoozed,
    FollowUps,
    Scheduled,
    Sent,
    Drafts,
    Starred,
    Archive,
    Spam,
    Trash,
    Label { label_id: LabelId },
    Search { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncState {
    Idle { last_ok: Option<UnixMs> },
    Connecting,
    Syncing { done: u32, total: Option<u32> },
    Backoff {
        retry_at: UnixMs,
        attempt: u32,
        #[serde(default)]
        reason: Option<String>,
    },
    Blocked { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityView {
    pub id: IdentityId,
    pub address: String,
    pub display_name: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub developer_tools_enabled: bool,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub tour_completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: AccountId,
    pub display_name: String,
    pub provider: ProviderKind,
    pub color: SpaceColor,
    pub email: String,
    pub identities: Vec<IdentityView>,
    pub sync: SyncState,
    pub unread: u32,
    pub position: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAccountInput {
    pub display_name: String,
    pub color: SpaceColor,
    pub auth: ProviderAuth,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSignInInput {
    pub provider: ProviderKind,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub color: Option<SpaceColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountInput {
    pub account_id: AccountId,
    pub display_name: String,
    pub color: SpaceColor,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedOAuthClients {
    pub gmail_client_id: Option<String>,
    pub outlook_client_id: Option<String>,
    pub outlook_tenant: Option<String>,
    #[serde(default)]
    pub gmail_built_in: bool,
    #[serde(default)]
    pub outlook_built_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadQuery {
    pub scope: Scope,
    pub view: MailboxView,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelBadge {
    pub id: LabelId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub account_label: String,
    pub account_color: SpaceColor,
    pub account_email: String,
    pub subject: String,
    pub snippet: String,
    pub sender_name: String,
    pub sender_email: String,
    pub last_at: UnixMs,
    pub unread_count: u32,
    pub message_count: u32,
    pub has_attachments: bool,
    pub starred: bool,
    pub category: InboxCategory,
    pub snoozed_until: Option<UnixMs>,
    pub follow_up_due: Option<UnixMs>,
    pub trackers_blocked: u32,
    pub labels: Vec<LabelBadge>,
    #[serde(default)]
    pub is_vip: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPage {
    pub items: Vec<ThreadSummary>,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteContent {
    BlockAll,
    AllowImagesStripTrackers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoteImagesStatus {
    None,
    Blocked { count: u32 },
    Allowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackerKind {
    Pixel1x1,
    HiddenImage,
    KnownDomain,
    CssBeacon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerHit {
    pub host: String,
    pub kind: TrackerKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyReport {
    pub remote_images: RemoteImagesStatus,
    pub trackers: Vec<TrackerHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    pub id: AttachmentId,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub cached: bool,
    #[serde(default)]
    pub is_inline: bool,
    #[serde(default)]
    pub content_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentData {
    pub id: AttachmentId,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachmentInput {
    pub filename: String,
    pub mime: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachmentView {
    pub id: AttachmentId,
    pub filename: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsubscribeInfo {
    pub one_click: bool,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: MessageId,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: UnixMs,
    pub snippet: String,
    pub body_srcdoc: String,
    pub body_text: String,
    pub privacy: PrivacyReport,
    pub attachments: Vec<AttachmentMeta>,
    pub unsubscribe: Option<UnsubscribeInfo>,
    #[serde(default)]
    pub invites: Vec<crate::calendar::CalendarInviteView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadView {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub account_label: String,
    pub account_color: SpaceColor,
    pub account_email: String,
    pub subject: String,
    pub starred: bool,
    pub snoozed_until: Option<UnixMs>,
    pub follow_up_due: Option<UnixMs>,
    pub messages: Vec<MessageView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MailAction {
    Archive,
    Unarchive,
    Trash,
    ReportSpam,
    MarkRead { read: bool },
    Star { on: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    pub updated_ids: Vec<ThreadId>,
    pub undo_action: Option<MailAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DraftOrigin {
    Human,
    Agent { agent_id: String, rationale: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DraftState {
    Editing,
    PendingApproval,
    Queued { job_id: JobId },
    Sent { at: UnixMs },
    Rejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftView {
    pub id: DraftId,
    pub account_id: AccountId,
    pub space_color: SpaceColor,
    pub from_identity: IdentityView,
    pub from_locked: bool,
    pub reply_to_thread: Option<ThreadId>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<DraftAttachmentView>,
    pub rev: u32,
    pub origin: DraftOrigin,
    pub state: DraftState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDraftInput {
    pub account_id: AccountId,
    pub reply_to_thread: Option<ThreadId>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<DraftAttachmentInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SendMode {
    Undoable { undo_secs: u32 },
    Scheduled { send_at: UnixMs },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReceipt {
    pub job_id: JobId,
    pub draft_id: DraftId,
    pub due_at: UnixMs,
    pub scheduled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UndoOutcome {
    Cancelled,
    TooLate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: JobId,
    pub kind: String,
    pub account_id: AccountId,
    pub thread_id: Option<ThreadId>,
    pub due_at: UnixMs,
    pub state: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateView {
    pub id: TemplateId,
    pub name: String,
    pub shortcut: Option<String>,
    pub subject: String,
    pub body_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTemplateInput {
    pub id: Option<TemplateId>,
    pub name: String,
    pub shortcut: Option<String>,
    pub subject: String,
    pub body_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureView {
    pub id: SignatureId,
    pub account_id: AccountId,
    pub name: String,
    pub body_html: String,
    pub body_text: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSignatureInput {
    pub id: Option<SignatureId>,
    pub account_id: AccountId,
    pub name: String,
    pub body_html: String,
    pub body_text: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxCounts {
    pub inbox: u32,
    pub snoozed: u32,
    pub follow_ups: u32,
    pub scheduled: u32,
    pub primary: u32,
    pub receipts: u32,
    pub newsletters: u32,
    #[serde(default)]
    pub vip: u32,
    #[serde(default)]
    pub screener: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearchView {
    pub id: SavedSearchId,
    pub name: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSavedSearchInput {
    pub id: Option<SavedSearchId>,
    pub name: String,
    pub query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenerDecision {
    Inbox,
    PaperTrail,
    Feed,
    Blocked,
}

impl ScreenerDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::PaperTrail => "paper_trail",
            Self::Feed => "feed",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "paper_trail" => Self::PaperTrail,
            "feed" => Self::Feed,
            "blocked" => Self::Blocked,
            _ => Self::Inbox,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenerSenderView {
    pub address: String,
    pub display_name: String,
    pub sample_subject: String,
    pub thread_count: u32,
    pub latest_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VipSenderView {
    pub address: String,
    pub display_name: Option<String>,
    pub added_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelView {
    pub id: LabelId,
    pub name: String,
    pub color: SpaceColor,
    pub thread_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLabelInput {
    pub id: Option<LabelId>,
    pub name: String,
    pub color: SpaceColor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailRuleView {
    pub id: RuleId,
    pub name: String,
    pub enabled: bool,
    pub match_all: bool,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMailRuleInput {
    pub id: Option<RuleId>,
    pub name: String,
    pub enabled: bool,
    pub match_all: bool,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
}
