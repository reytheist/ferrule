use serde::{Deserialize, Serialize};

use crate::ids::{AccountId, DraftId, JobId, MessageId, ThreadId, UnixMs};
use crate::types::{JobView, SyncState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Alert {
    SnoozeWoke { thread_id: ThreadId },
    FollowUpDue { thread_id: ThreadId },
    SendFailed { job_id: JobId, reason: String },
    ApprovalRequested { draft_id: DraftId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CoreEvent {
    MailChanged {
        account_id: AccountId,
        thread_ids: Vec<ThreadId>,
    },
    SyncChanged {
        account_id: AccountId,
        state: SyncState,
    },
    JobChanged {
        job: JobView,
    },
    DraftChanged {
        draft_id: DraftId,
    },
    Attention {
        alert: Alert,
    },
    AgentActivityChanged,
    CalendarChanged,
    Resync,
}

/// Safe metadata-only push event emitted to Hermes over local SSE and optional
/// localhost webhooks. Never contains raw HTML or credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "event",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum HermesPushEvent {
    NewMessage {
        account_id: AccountId,
        thread_id: ThreadId,
        message_id: MessageId,
        from_email: String,
        from_name: Option<String>,
        subject: String,
        date: UnixMs,
        folder: String,
    },
    NewInvite {
        account_id: AccountId,
        thread_id: ThreadId,
        message_id: MessageId,
        uid: String,
        summary: String,
        organizer_email: String,
        start_at: UnixMs,
        end_at: UnixMs,
    },
}

impl HermesPushEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::NewMessage { .. } => "new_message",
            Self::NewInvite { .. } => "new_invite",
        }
    }

    pub fn account_id(&self) -> AccountId {
        match self {
            Self::NewMessage { account_id, .. } | Self::NewInvite { account_id, .. } => *account_id,
        }
    }
}
