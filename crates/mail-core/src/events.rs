use serde::{Deserialize, Serialize};

use crate::ids::{AccountId, DraftId, JobId, ThreadId};
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
    Resync,
}
