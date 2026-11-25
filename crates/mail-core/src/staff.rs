//! Phase B — Pigment Staff Sync & Email Collaboration domain types.

use serde::{Deserialize, Serialize};

use crate::ids::{
    AccountId, CommentId, ContactId, PinnedLinkId, SharedTemplateId, SharedThreadId,
    StaffMemberId, ThreadId, UnixMs,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StaffSyncStatus {
    Disconnected,
    Connected,
    Offline,
    Error,
}

impl StaffSyncStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connected => "connected",
            Self::Offline => "offline",
            Self::Error => "error",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "connected" => Self::Connected,
            "offline" => Self::Offline,
            "error" => Self::Error,
            _ => Self::Disconnected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffWorkspaceView {
    pub connected: bool,
    pub server_url: String,
    pub staff_name: String,
    pub staff_handle: String,
    pub staff_email: String,
    pub staff_role: String,
    pub connected_at: Option<UnixMs>,
    pub last_sync_at: Option<UnixMs>,
    pub status: StaffSyncStatus,
    pub last_error: Option<String>,
    #[serde(default)]
    pub pending_outbox_count: u32,
}

impl Default for StaffWorkspaceView {
    fn default() -> Self {
        Self {
            connected: false,
            server_url: String::new(),
            staff_name: String::new(),
            staff_handle: String::new(),
            staff_email: String::new(),
            staff_role: String::new(),
            connected_at: None,
            last_sync_at: None,
            status: StaffSyncStatus::Disconnected,
            last_error: None,
            pending_outbox_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectStaffWorkspaceInput {
    pub server_url: String,
    pub token: String,
    pub staff_name: String,
    pub staff_handle: String,
    pub staff_email: String,
    pub staff_role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffMemberView {
    pub id: StaffMemberId,
    pub name: String,
    pub role: String,
    pub handle: String,
    pub email: String,
    pub department: Option<String>,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShareVisibility {
    AllStaff,
    Specific,
}

impl ShareVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllStaff => "all_staff",
            Self::Specific => "specific",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "specific" => Self::Specific,
            _ => Self::AllStaff,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HandoffStatus {
    #[default]
    Open,
    Waiting,
    Done,
}

impl HandoffStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Waiting => "waiting",
            Self::Done => "done",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "waiting" => Self::Waiting,
            "done" => Self::Done,
            _ => Self::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftLockState {
    #[default]
    Unlocked,
    Editing,
    ReviewRequested,
    Approved,
}

impl DraftLockState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlocked => "unlocked",
            Self::Editing => "editing",
            Self::ReviewRequested => "review_requested",
            Self::Approved => "approved",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "editing" => Self::Editing,
            "review_requested" => Self::ReviewRequested,
            "approved" => Self::Approved,
            _ => Self::Unlocked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMessageSnapshot {
    pub id: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: UnixMs,
    pub snippet: String,
    pub body_text: String,
    pub body_srcdoc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedCommentView {
    pub id: CommentId,
    pub shared_thread_id: SharedThreadId,
    pub author_handle: String,
    pub author_name: String,
    pub body_text: String,
    pub mentions: Vec<String>,
    pub created_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedDraftView {
    pub shared_thread_id: SharedThreadId,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub body_text: String,
    pub editor_handle: Option<String>,
    pub editor_name: Option<String>,
    pub lock_state: DraftLockState,
    pub review_requested_by: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<UnixMs>,
    pub rev: u32,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedThreadSummary {
    pub id: SharedThreadId,
    pub source_thread_id: Option<ThreadId>,
    pub source_account_id: Option<AccountId>,
    pub owner_email: String,
    pub owner_name: String,
    pub subject: String,
    pub snippet: String,
    pub participants: Vec<String>,
    pub message_count: u32,
    pub visibility: ShareVisibility,
    pub invited_handles: Vec<String>,
    pub assignee_handle: Option<String>,
    pub status: HandoffStatus,
    pub comment_count: u32,
    pub draft_lock_state: DraftLockState,
    pub draft_editor_name: Option<String>,
    pub shared_at: UnixMs,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedThreadView {
    pub id: SharedThreadId,
    pub source_thread_id: Option<ThreadId>,
    pub source_account_id: Option<AccountId>,
    pub owner_email: String,
    pub owner_name: String,
    pub subject: String,
    pub snippet: String,
    pub participants: Vec<String>,
    pub messages: Vec<SharedMessageSnapshot>,
    pub visibility: ShareVisibility,
    pub invited_handles: Vec<String>,
    pub assignee_handle: Option<String>,
    pub status: HandoffStatus,
    pub comments: Vec<SharedCommentView>,
    pub draft: Option<SharedDraftView>,
    pub shared_at: UnixMs,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareThreadInput {
    pub thread_id: ThreadId,
    pub visibility: ShareVisibility,
    #[serde(default)]
    pub invited_handles: Vec<String>,
    #[serde(default)]
    pub assignee_handle: Option<String>,
    #[serde(default)]
    pub initial_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHandoffInput {
    pub shared_thread_id: SharedThreadId,
    pub assignee_handle: Option<String>,
    pub status: HandoffStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSharedCommentInput {
    pub shared_thread_id: SharedThreadId,
    pub body_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DraftLockAction {
    Claim,
    TakeOver,
    Save {
        subject: String,
        to: Vec<String>,
        cc: Vec<String>,
        body_text: String,
    },
    RequestReview {
        reviewer_handles: Vec<String>,
        note: Option<String>,
    },
    Approve,
    Release,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSharedDraftInput {
    pub shared_thread_id: SharedThreadId,
    pub action: DraftLockAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedTemplateView {
    pub id: SharedTemplateId,
    pub name: String,
    pub shortcut: Option<String>,
    pub category: String,
    pub subject: String,
    pub body_text: String,
    pub updated_by: String,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSharedTemplateInput {
    pub id: Option<SharedTemplateId>,
    pub name: String,
    pub shortcut: Option<String>,
    pub category: String,
    pub subject: String,
    pub body_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContactCategory {
    Sponsor,
    Vendor,
    Instructor,
    Vip,
    Volunteer,
    Venue,
    Other,
}

impl ContactCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sponsor => "sponsor",
            Self::Vendor => "vendor",
            Self::Instructor => "instructor",
            Self::Vip => "vip",
            Self::Volunteer => "volunteer",
            Self::Venue => "venue",
            Self::Other => "other",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw {
            "sponsor" => Self::Sponsor,
            "vendor" => Self::Vendor,
            "instructor" => Self::Instructor,
            "vip" => Self::Vip,
            "volunteer" => Self::Volunteer,
            "venue" => Self::Venue,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowContactView {
    pub id: ContactId,
    pub name: String,
    pub emails: Vec<String>,
    pub category: ContactCategory,
    pub organization: Option<String>,
    pub tier: Option<String>,
    pub notes: String,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveShowContactInput {
    pub id: Option<ContactId>,
    pub name: String,
    pub emails: Vec<String>,
    pub category: ContactCategory,
    pub organization: Option<String>,
    pub tier: Option<String>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedLinkView {
    pub id: PinnedLinkId,
    pub title: String,
    pub url: String,
    pub category: String,
    pub description: Option<String>,
    pub position: u32,
    pub updated_at: UnixMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePinnedLinkInput {
    pub id: Option<PinnedLinkId>,
    pub title: String,
    pub url: String,
    pub category: String,
    pub description: Option<String>,
}

/// Extract normalized `@handle` mentions from a comment body.
pub fn extract_mentions(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.');
            if prev_ok {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'_'
                        || bytes[end] == b'-')
                {
                    end += 1;
                }
                if end > start {
                    let handle = body[start..end].to_ascii_lowercase();
                    if !out.contains(&handle) {
                        out.push(handle);
                    }
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

pub fn normalize_handle(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('@')
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}
