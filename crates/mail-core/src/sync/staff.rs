//! Decoupled HTTP + SSE client for the standalone Pigment Staff Sync service.
//!
//! ## Wire Contract (`/v1`)
//!
//! All requests carry `Authorization: Bearer <staff_token>` (read from `SecretStore`,
//! never persisted to SQLite or `localStorage`).
//!
//! - `GET  /v1/snapshot?since=<unix_ms>` -> `StaffSyncSnapshot`
//! - `POST /v1/shared-threads` -> upserts a sanitized `SharedThreadView`
//! - `PATCH /v1/shared-threads/{id}/handoff` -> updates assignee + `HandoffStatus`
//! - `POST /v1/shared-threads/{id}/comments` -> appends a `SharedCommentView`
//! - `PUT  /v1/shared-threads/{id}/draft` -> updates `SharedDraftView` lock/body
//! - `PUT  /v1/shared-templates/{id}` / `DELETE /v1/shared-templates/{id}`
//! - `PUT  /v1/contacts/{id}` / `DELETE /v1/contacts/{id}`
//! - `PUT  /v1/pinned-links/{id}` / `DELETE /v1/pinned-links/{id}`
//! - `GET  /v1/events` (`Accept: text/event-stream`) -> SSE stream of `StaffSyncEvent`

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::calendar::{ActionItemView, CalendarEventView, MeetingDocView};
use crate::error::CoreError;
use crate::ids::{
    ActionItemId, CalendarEventId, ContactId, PinnedLinkId, SharedTemplateId, SharedThreadId,
    UnixMs,
};
use crate::staff::{
    HandoffStatus, PinnedLinkView, SharedCommentView, SharedDraftView, SharedTemplateView,
    SharedThreadView, ShowContactView, StaffMemberView,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffSyncSnapshot {
    #[serde(default)]
    pub members: Vec<StaffMemberView>,
    #[serde(default)]
    pub shared_threads: Vec<SharedThreadView>,
    #[serde(default)]
    pub shared_templates: Vec<SharedTemplateView>,
    #[serde(default)]
    pub contacts: Vec<ShowContactView>,
    #[serde(default)]
    pub pinned_links: Vec<PinnedLinkView>,
    #[serde(default)]
    pub calendar_events: Vec<CalendarEventView>,
    #[serde(default)]
    pub meeting_docs: Vec<MeetingDocView>,
    #[serde(default)]
    pub action_items: Vec<ActionItemView>,
    pub server_time: Option<UnixMs>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StaffSyncEvent {
    SnapshotInvalidated { since: Option<UnixMs> },
    ThreadUpdated { shared_thread_id: SharedThreadId },
    CommentAdded { shared_thread_id: SharedThreadId },
    DraftUpdated { shared_thread_id: SharedThreadId },
    CalendarUpdated { event_id: CalendarEventId },
    MeetingDocUpdated { event_id: CalendarEventId },
    ActionItemUpdated { action_item_id: ActionItemId },
    DirectoryUpdated,
    ResourcesUpdated,
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum StaffOutboxOp {
    UpsertMember {
        member: StaffMemberView,
    },
    UpsertSharedThread {
        thread: SharedThreadView,
    },
    UpdateHandoff {
        shared_thread_id: SharedThreadId,
        assignee_handle: Option<String>,
        status: HandoffStatus,
    },
    AddComment {
        comment: SharedCommentView,
    },
    UpsertDraft {
        draft: SharedDraftView,
    },
    UpsertSharedTemplate {
        template: SharedTemplateView,
    },
    DeleteSharedTemplate {
        id: SharedTemplateId,
    },
    UpsertContact {
        contact: ShowContactView,
    },
    DeleteContact {
        id: ContactId,
    },
    UpsertPinnedLink {
        link: PinnedLinkView,
    },
    DeletePinnedLink {
        id: PinnedLinkId,
    },
    UpsertCalendarEvent {
        event: CalendarEventView,
    },
    DeleteCalendarEvent {
        id: CalendarEventId,
    },
    UpsertMeetingDoc {
        doc: MeetingDocView,
    },
    UpsertActionItem {
        item: ActionItemView,
    },
    DeleteActionItem {
        id: ActionItemId,
    },
}

impl StaffOutboxOp {
    pub(crate) fn kind_key(&self) -> &'static str {
        match self {
            Self::UpsertMember { .. } => "upsert_member",
            Self::UpsertSharedThread { .. } => "upsert_shared_thread",
            Self::UpdateHandoff { .. } => "update_handoff",
            Self::AddComment { .. } => "add_comment",
            Self::UpsertDraft { .. } => "upsert_draft",
            Self::UpsertSharedTemplate { .. } => "upsert_shared_template",
            Self::DeleteSharedTemplate { .. } => "delete_shared_template",
            Self::UpsertContact { .. } => "upsert_contact",
            Self::DeleteContact { .. } => "delete_contact",
            Self::UpsertPinnedLink { .. } => "upsert_pinned_link",
            Self::DeletePinnedLink { .. } => "delete_pinned_link",
            Self::UpsertCalendarEvent { .. } => "upsert_calendar_event",
            Self::DeleteCalendarEvent { .. } => "delete_calendar_event",
            Self::UpsertMeetingDoc { .. } => "upsert_meeting_doc",
            Self::UpsertActionItem { .. } => "upsert_action_item",
            Self::DeleteActionItem { .. } => "delete_action_item",
        }
    }

    pub(crate) fn entity_key(&self) -> String {
        match self {
            Self::UpsertMember { member } => member.id.to_string(),
            Self::UpsertSharedThread { thread } => thread.id.to_string(),
            Self::UpdateHandoff { shared_thread_id, .. } => shared_thread_id.to_string(),
            Self::AddComment { comment } => comment.id.to_string(),
            Self::UpsertDraft { draft } => draft.shared_thread_id.to_string(),
            Self::UpsertSharedTemplate { template } => template.id.to_string(),
            Self::DeleteSharedTemplate { id } => id.to_string(),
            Self::UpsertContact { contact } => contact.id.to_string(),
            Self::DeleteContact { id } => id.to_string(),
            Self::UpsertPinnedLink { link } => link.id.to_string(),
            Self::DeletePinnedLink { id } => id.to_string(),
            Self::UpsertCalendarEvent { event } => event.id.to_string(),
            Self::DeleteCalendarEvent { id } => id.to_string(),
            Self::UpsertMeetingDoc { doc } => doc.event_id.to_string(),
            Self::UpsertActionItem { item } => item.id.to_string(),
            Self::DeleteActionItem { id } => id.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StaffSyncClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

pub(crate) fn is_loopback_http(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "http" {
        return false;
    }
    matches!(
        parsed.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    )
}

impl StaffSyncClient {
    pub(crate) fn new(base_url: &str, token: &str) -> Result<Self, CoreError> {
        let trimmed = base_url.trim().trim_end_matches('/').to_string();
        if !(trimmed.starts_with("https://") || is_loopback_http(&trimmed)) {
            return Err(CoreError::InvalidInput {
                field: "serverUrl".to_string(),
                reason: "Remote Staff Sync URL must use https:// (or http://localhost for local dev)"
                    .to_string(),
            });
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|e| CoreError::Network {
                reason: format!("init Staff Sync HTTP client: {e}"),
                retryable: true,
            })?;
        Ok(Self {
            base_url: trimmed,
            token: token.to_string(),
            http,
        })
    }

    pub(crate) async fn pull_snapshot(
        &self,
        since: Option<UnixMs>,
    ) -> Result<StaffSyncSnapshot, CoreError> {
        let url = match since {
            Some(ts) => format!("{}/v1/snapshot?since={}", self.base_url, ts.0),
            None => format!("{}/v1/snapshot", self.base_url),
        };
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("Staff Sync unreachable: {e}"),
                retryable: true,
            })?;
        if !resp.status().is_success() {
            return Err(CoreError::Network {
                reason: format!("Staff Sync returned HTTP {}", resp.status().as_u16()),
                retryable: resp.status().is_server_error(),
            });
        }
        resp.json::<StaffSyncSnapshot>()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("invalid Staff Sync snapshot JSON: {e}"),
                retryable: false,
            })
    }

    pub(crate) async fn push_member(&self, member: &StaffMemberView) -> Result<(), CoreError> {
        let url = format!("{}/v1/members/{}", self.base_url, member.id);
        self.put_json(&url, member).await
    }

    pub(crate) async fn execute_op(&self, op: &StaffOutboxOp) -> Result<(), CoreError> {
        match op {
            StaffOutboxOp::UpsertMember { member } => self.push_member(member).await,
            StaffOutboxOp::UpsertSharedThread { thread } => self.push_shared_thread(thread).await,
            StaffOutboxOp::UpdateHandoff {
                shared_thread_id,
                assignee_handle,
                status,
            } => {
                self.push_handoff(*shared_thread_id, assignee_handle.as_deref(), *status)
                    .await
            }
            StaffOutboxOp::AddComment { comment } => self.push_comment(comment).await,
            StaffOutboxOp::UpsertDraft { draft } => self.push_draft(draft).await,
            StaffOutboxOp::UpsertSharedTemplate { template } => {
                self.push_shared_template(template).await
            }
            StaffOutboxOp::DeleteSharedTemplate { id } => self.delete_shared_template(*id).await,
            StaffOutboxOp::UpsertContact { contact } => self.push_contact(contact).await,
            StaffOutboxOp::DeleteContact { id } => self.delete_contact(*id).await,
            StaffOutboxOp::UpsertPinnedLink { link } => self.push_pinned_link(link).await,
            StaffOutboxOp::DeletePinnedLink { id } => self.delete_pinned_link(*id).await,
            StaffOutboxOp::UpsertCalendarEvent { event } => self.push_calendar_event(event).await,
            StaffOutboxOp::DeleteCalendarEvent { id } => self.delete_calendar_event(*id).await,
            StaffOutboxOp::UpsertMeetingDoc { doc } => self.push_meeting_doc(doc).await,
            StaffOutboxOp::UpsertActionItem { item } => self.push_action_item(item).await,
            StaffOutboxOp::DeleteActionItem { id } => self.delete_action_item(*id).await,
        }
    }

    /// Open a long-lived Server-Sent Events (`text/event-stream`) connection without
    /// applying the short request timeout used for unary JSON calls.
    pub(crate) async fn open_event_stream(&self) -> Result<reqwest::Response, CoreError> {
        let sse_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| CoreError::Network {
                reason: format!("init Staff Sync SSE client: {e}"),
                retryable: true,
            })?;
        let url = format!("{}/v1/events", self.base_url);
        let resp = sse_client
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("Staff Sync SSE unreachable: {e}"),
                retryable: true,
            })?;
        if !resp.status().is_success() {
            return Err(CoreError::Network {
                reason: format!("Staff Sync SSE returned HTTP {}", resp.status().as_u16()),
                retryable: resp.status().is_server_error(),
            });
        }
        Ok(resp)
    }

    pub(crate) async fn push_shared_thread(&self, thread: &SharedThreadView) -> Result<(), CoreError> {
        self.post_json(&format!("{}/v1/shared-threads", self.base_url), thread)
            .await
    }

    pub(crate) async fn push_handoff(
        &self,
        id: SharedThreadId,
        assignee_handle: Option<&str>,
        status: HandoffStatus,
    ) -> Result<(), CoreError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            assignee_handle: Option<&'a str>,
            status: HandoffStatus,
        }
        let url = format!("{}/v1/shared-threads/{id}/handoff", self.base_url);
        let resp = self
            .http
            .patch(&url)
            .bearer_auth(&self.token)
            .json(&Payload {
                assignee_handle,
                status,
            })
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("push handoff: {e}"),
                retryable: true,
            })?;
        check_status(resp.status())
    }

    pub(crate) async fn push_comment(&self, comment: &SharedCommentView) -> Result<(), CoreError> {
        let url = format!(
            "{}/v1/shared-threads/{}/comments",
            self.base_url, comment.shared_thread_id
        );
        self.post_json(&url, comment).await
    }

    pub(crate) async fn push_draft(&self, draft: &SharedDraftView) -> Result<(), CoreError> {
        let url = format!(
            "{}/v1/shared-threads/{}/draft",
            self.base_url, draft.shared_thread_id
        );
        self.put_json(&url, draft).await
    }

    pub(crate) async fn push_shared_template(
        &self,
        template: &SharedTemplateView,
    ) -> Result<(), CoreError> {
        let url = format!("{}/v1/shared-templates/{}", self.base_url, template.id);
        self.put_json(&url, template).await
    }

    pub(crate) async fn delete_shared_template(&self, id: SharedTemplateId) -> Result<(), CoreError> {
        let url = format!("{}/v1/shared-templates/{id}", self.base_url);
        self.delete_url(&url).await
    }

    pub(crate) async fn push_contact(&self, contact: &ShowContactView) -> Result<(), CoreError> {
        let url = format!("{}/v1/contacts/{}", self.base_url, contact.id);
        self.put_json(&url, contact).await
    }

    pub(crate) async fn delete_contact(&self, id: ContactId) -> Result<(), CoreError> {
        let url = format!("{}/v1/contacts/{id}", self.base_url);
        self.delete_url(&url).await
    }

    pub(crate) async fn push_pinned_link(&self, link: &PinnedLinkView) -> Result<(), CoreError> {
        let url = format!("{}/v1/pinned-links/{}", self.base_url, link.id);
        self.put_json(&url, link).await
    }

    pub(crate) async fn delete_pinned_link(&self, id: PinnedLinkId) -> Result<(), CoreError> {
        let url = format!("{}/v1/pinned-links/{id}", self.base_url);
        self.delete_url(&url).await
    }

    pub(crate) async fn push_calendar_event(
        &self,
        event: &CalendarEventView,
    ) -> Result<(), CoreError> {
        let url = format!("{}/v1/calendar/events/{}", self.base_url, event.id);
        self.put_json(&url, event).await
    }

    pub(crate) async fn delete_calendar_event(&self, id: CalendarEventId) -> Result<(), CoreError> {
        let url = format!("{}/v1/calendar/events/{id}", self.base_url);
        self.delete_url(&url).await
    }

    pub(crate) async fn push_meeting_doc(&self, doc: &MeetingDocView) -> Result<(), CoreError> {
        let url = format!("{}/v1/calendar/events/{}/doc", self.base_url, doc.event_id);
        self.put_json(&url, doc).await
    }

    pub(crate) async fn push_action_item(&self, item: &ActionItemView) -> Result<(), CoreError> {
        let url = format!("{}/v1/action-items/{}", self.base_url, item.id);
        self.put_json(&url, item).await
    }

    pub(crate) async fn delete_action_item(&self, id: ActionItemId) -> Result<(), CoreError> {
        let url = format!("{}/v1/action-items/{id}", self.base_url);
        self.delete_url(&url).await
    }

    async fn post_json<T: Serialize>(&self, url: &str, body: &T) -> Result<(), CoreError> {
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("Staff Sync POST failed: {e}"),
                retryable: true,
            })?;
        check_status(resp.status())
    }

    async fn put_json<T: Serialize>(&self, url: &str, body: &T) -> Result<(), CoreError> {
        let resp = self
            .http
            .put(url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("Staff Sync PUT failed: {e}"),
                retryable: true,
            })?;
        check_status(resp.status())
    }

    async fn delete_url(&self, url: &str) -> Result<(), CoreError> {
        let resp = self
            .http
            .delete(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("Staff Sync DELETE failed: {e}"),
                retryable: true,
            })?;
        check_status(resp.status())
    }
}

fn check_status(status: reqwest::StatusCode) -> Result<(), CoreError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(CoreError::Network {
            reason: format!("Staff Sync returned HTTP {}", status.as_u16()),
            retryable: status.is_server_error(),
        })
    }
}

/// Parse one Server-Sent Events (`text/event-stream`) frame block into a [`StaffSyncEvent`].
/// Comments (`: keep-alive`) yield `None`.
pub fn parse_sse_frame(frame: &str) -> Option<StaffSyncEvent> {
    let mut data_lines = Vec::new();
    for line in frame.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let payload = data_lines.join("\n");
    serde_json::from_str::<StaffSyncEvent>(&payload).ok()
}
