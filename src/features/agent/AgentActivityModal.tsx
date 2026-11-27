import { useState } from "react";
import { Bot, Check, CornerUpLeft, RotateCcw, ShieldCheck, X } from "lucide-react";
import type { DraftView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { formatFull } from "../ui/format";

function draftRationale(draft: DraftView): { agentId: string; rationale: string } {
  if (draft.origin.kind === "agent") {
    return { agentId: draft.origin.agentId, rationale: draft.origin.rationale };
  }
  return { agentId: "agent", rationale: "" };
}

export function AgentActivityModal({ app }: { app: MailApp }) {
  const [busyId, setBusyId] = useState<string | null>(null);

  if (!app.agentModalOpen) return null;

  async function handleApprove(draft: DraftView): Promise<void> {
    setBusyId(draft.id);
    try {
      await app.approveAgentDraft(draft.id, { kind: "undoable", undoSecs: 7 });
    } finally {
      setBusyId(null);
    }
  }

  async function handleReject(draft: DraftView): Promise<void> {
    setBusyId(draft.id);
    try {
      await app.rejectAgentDraft(draft.id, "Rejected from Agent Queue");
    } finally {
      setBusyId(null);
    }
  }

  async function handleUndo(activityId: typeof app.agentActivity[number]["id"]): Promise<void> {
    setBusyId(activityId);
    try {
      await app.undoAgentActivity(activityId);
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busyId) app.closeAgentModal();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Hermes Agent Activity & Approvals">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Bot style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Hermes Agent Activity &amp; Approvals</h2>
          </div>
          <button
            className="icon-btn"
            disabled={busyId !== null}
            onClick={app.closeAgentModal}
            title="Close (Esc)"
          >
            <X />
          </button>
        </div>

        <div className="modal-body">
          <section className="settings-section">
            <div className="settings-section-title">
              Pending Approval Drafts ({app.pendingAgentDrafts.length})
            </div>
            <span className="hint-text">
              Agents can stage drafts with a rationale, but can never send mail without your
              explicit approval here.
            </span>

            {app.pendingAgentDrafts.length === 0 ? (
              <div className="dev-status-card">
                <span className="hint-text">No agent drafts awaiting approval.</span>
              </div>
            ) : (
              <div className="agent-list">
                {app.pendingAgentDrafts.map((draft) => {
                  const meta = draftRationale(draft);
                  return (
                    <div key={draft.id} className="agent-draft-banner">
                      <div className="agent-draft-head">
                        <span className="kbd">{meta.agentId}</span>
                        <strong>{draft.subject || "(no subject)"}</strong>
                        <span className="hint-text">To: {draft.to.join(", ") || "(none)"}</span>
                      </div>
                      {meta.rationale && (
                        <div className="agent-rationale">Rationale: {meta.rationale}</div>
                      )}
                      <pre className="agent-draft-preview">{draft.bodyText}</pre>
                      <div className="agent-draft-actions">
                        <button
                          type="button"
                          className="btn compact primary"
                          disabled={busyId !== null}
                          onClick={() => void handleApprove(draft)}
                        >
                          <Check />
                          Approve &amp; Send (7s undo)
                        </button>
                        {draft.replyToThread && (
                          <button
                            type="button"
                            className="btn compact"
                            disabled={busyId !== null}
                            onClick={() => {
                              app.openThread(draft.replyToThread!);
                              app.setReplyText(draft.bodyText);
                              app.closeAgentModal();
                            }}
                          >
                            <CornerUpLeft />
                            Open &amp; Edit
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn compact"
                          disabled={busyId !== null}
                          onClick={() => void handleReject(draft)}
                        >
                          Reject
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </section>

          <section className="settings-section">
            <div className="settings-section-title">
              Recent Agent Triage &amp; Actions ({app.agentActivity.length})
            </div>
            {app.agentActivity.length === 0 ? (
              <div className="dev-status-card">
                <span className="hint-text">
                  No Hermes MCP activity recorded yet. Connect Hermes via{" "}
                  <code>ferrule mcp</code> to inspect and undo mutations here.
                </span>
              </div>
            ) : (
              <div className="agent-list">
                {app.agentActivity.map((item) => (
                  <div key={item.id} className="activity-row">
                    <div className="activity-main">
                      <div className="activity-top">
                        <span className="kbd">{item.agentId}</span>
                        <code>{item.toolName}</code>
                        <span className="hint-text">{formatFull(item.createdAt)}</span>
                      </div>
                      <div className="activity-summary">{item.summary}</div>
                    </div>
                    {item.undone ? (
                      <span className="kbd">Undone</span>
                    ) : item.canUndo ? (
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busyId !== null}
                        onClick={() => void handleUndo(item.id)}
                      >
                        <RotateCcw />
                        Undo
                      </button>
                    ) : (
                      <span className="hint-text">Logged</span>
                    )}
                  </div>
                ))}
              </div>
            )}
          </section>
        </div>

        <div className="modal-foot">
          <span className="hint-text" style={{ flex: 1, display: "flex", alignItems: "center", gap: 6 }}>
            <ShieldCheck style={{ width: 13, height: 13 }} />
            Compile-time Sendable guard prevents any agent from sending mail directly.
          </span>
          <button type="button" className="btn primary" onClick={app.closeAgentModal}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
