//! Phase-2 agent capability boundary.
//!
//! [`AgentPort`] exposes read / search / stage_draft only. It holds no send or approve
//! method, and the crate-private `Sendable` token cannot be obtained through it, so an
//! MCP agent can never dispatch mail without a human approving the staged draft.

use crate::error::CoreError;
use crate::ids::{AgentId, ThreadId};
use crate::types::{DraftView, MailboxView, Scope, ThreadQuery, ThreadSummary};
use crate::MailCore;

/// Draft staged by an agent, awaiting human approval.
#[derive(Debug, Clone)]
pub struct StageAgentDraft {
    pub thread_id: ThreadId,
    pub body_text: String,
    pub rationale: String,
}

/// Phase-2 capability handle for MCP agents.
/// Intentionally exposes read/search/stage_draft only — never `send` or `approve`.
#[derive(Clone)]
pub struct AgentPort {
    agent_id: AgentId,
    core: MailCore,
}

impl AgentPort {
    pub(crate) fn new(agent_id: AgentId, core: MailCore) -> Self {
        Self { agent_id, core }
    }

    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// Full-text search across every configured account.
    pub async fn search(&self, text: &str, limit: u32) -> Result<Vec<ThreadSummary>, CoreError> {
        let query = ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Search {
                text: text.to_string(),
            },
            limit: limit.clamp(1, 50),
            offset: 0,
        };
        Ok(self.core.list_threads(query).await?.items)
    }

    /// Plain-text rendering of a thread: headers plus `body_text` only, never raw HTML.
    pub async fn read_thread_plain(&self, thread_id: ThreadId) -> Result<String, CoreError> {
        self.core.read_thread_plain(thread_id).await
    }

    /// Stage a reply draft on the thread. The draft lands in
    /// [`crate::types::DraftState::PendingApproval`] with a [`crate::types::DraftOrigin::Agent`];
    /// only [`MailCore::approve_agent_draft`] (a human path) can ever queue it for send.
    pub async fn stage_draft(&self, input: StageAgentDraft) -> Result<DraftView, CoreError> {
        self.core
            .stage_agent_draft(
                self.agent_id.as_str(),
                input.thread_id,
                input.body_text,
                input.rationale,
            )
            .await
    }
}
