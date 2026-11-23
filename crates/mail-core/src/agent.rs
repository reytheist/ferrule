use crate::ids::{AgentId, ThreadId};

#[derive(Debug, Clone)]
pub struct StageAgentDraft {
    pub thread_id: ThreadId,
    pub body_text: String,
    pub rationale: String,
}

/// Phase-2 capability handle for MCP agents.
/// Intentionally exposes read/search/stage_draft only — never `send` or `approve`.
#[derive(Debug, Clone)]
pub struct AgentPort {
    agent_id: AgentId,
}

impl AgentPort {
    pub(crate) fn new(agent_id: AgentId) -> Self {
        Self { agent_id }
    }

    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
}
