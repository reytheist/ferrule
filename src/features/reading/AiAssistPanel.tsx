import { useEffect, useState } from "react";
import { Bot, CheckCircle2, FileEdit, HelpCircle, Send, Sparkles, X } from "lucide-react";
import type { AiSummaryResult, ThreadView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export function AiAssistPanel({
  app,
  thread,
  onClose,
}: {
  app: MailApp;
  thread: ThreadView;
  onClose: () => void;
}) {
  const [summary, setSummary] = useState<AiSummaryResult | null>(null);
  const [question, setQuestion] = useState("");
  const [selectedExcerpt, setSelectedExcerpt] = useState("");
  const [answer, setAnswer] = useState<string | null>(null);
  const [draftInstructions, setDraftInstructions] = useState("");
  const [busy, setBusy] = useState<"summary" | "ask" | "draft" | null>(null);
  const [errMsg, setErrMsg] = useState<string | null>(null);

  useEffect(() => {
    setSummary(null);
    setAnswer(null);
    setErrMsg(null);
  }, [thread.id]);

  function captureSelection(): void {
    const sel = window.getSelection()?.toString().trim() ?? "";
    if (sel) {
      setSelectedExcerpt(sel);
    } else {
      app.showToast("Select text in the message first, or paste an excerpt below.");
    }
  }

  async function runSummary(): Promise<void> {
    setBusy("summary");
    setErrMsg(null);
    try {
      const res = await app.aiSummarizeThread(thread.id);
      setSummary(res);
    } catch (err) {
      setErrMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function runAsk(): Promise<void> {
    if (!question.trim()) return;
    setBusy("ask");
    setErrMsg(null);
    try {
      const res = await app.aiAskThread(
        thread.id,
        question.trim(),
        selectedExcerpt.trim() ? selectedExcerpt.trim() : null,
      );
      setAnswer(res.answer);
    } catch (err) {
      setErrMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function runDraft(stageForApproval: boolean): Promise<void> {
    setBusy("draft");
    setErrMsg(null);
    try {
      const res = await app.aiDraftReply(thread.id, draftInstructions.trim(), stageForApproval);
      if (stageForApproval && res.stagedDraft) {
        app.showToast("AI reply staged in PendingApproval");
      } else {
        app.setReplyText(res.bodyText);
        app.focusReply();
        app.showToast("AI draft inserted into reply box");
      }
    } catch (err) {
      setErrMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="ai-assist-panel">
      <div className="ai-assist-head">
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <Sparkles style={{ width: 14, height: 14, color: "var(--app-accent)" }} />
          <strong>In-Pane AI</strong>
          {app.aiConfig.enabled && (
            <span className="kbd">{app.aiConfig.model || "configured"}</span>
          )}
        </div>
        <button type="button" className="icon-btn" onClick={onClose} title="Close AI panel">
          <X />
        </button>
      </div>

      {!app.aiConfig.enabled ? (
        <div className="ai-disabled-note">
          <span className="hint-text">
            In-pane AI is disabled. Configure an OpenAI-compatible endpoint (Hermes, Ollama, or
            hosted API) in Settings.
          </span>
          <button type="button" className="btn compact primary" onClick={app.openSettings}>
            Configure AI in Settings
          </button>
        </div>
      ) : (
        <div className="ai-assist-body">
          {errMsg && <div className="ai-error">{errMsg}</div>}

          <div className="ai-card">
            <div className="ai-card-head">
              <span>Thread Summary</span>
              <button
                type="button"
                className="btn compact primary"
                disabled={busy !== null}
                onClick={() => void runSummary()}
              >
                <Sparkles />
                {busy === "summary" ? "Summarizing…" : "Summarize"}
              </button>
            </div>
            {summary && (
              <div className="ai-summary-out">
                <p>{summary.summary}</p>
                {summary.keyPoints.length > 0 && (
                  <ul>
                    {summary.keyPoints.map((pt, i) => (
                      <li key={i}>{pt}</li>
                    ))}
                  </ul>
                )}
                {summary.actionItems.length > 0 && (
                  <div className="ai-actions-list">
                    <strong>Action items:</strong>
                    <ul>
                      {summary.actionItems.map((act, i) => (
                        <li key={i}>
                          <CheckCircle2 style={{ width: 11, height: 11, marginRight: 4 }} />
                          {act}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            )}
          </div>

          <div className="ai-card">
            <div className="ai-card-head">
              <span>
                <HelpCircle style={{ width: 12, height: 12, marginRight: 4, verticalAlign: "-1px" }} />
                Selection &amp; Thread Q&amp;A
              </span>
              <button type="button" className="btn compact" onClick={captureSelection}>
                Use Selection
              </button>
            </div>
            {selectedExcerpt && (
              <div className="ai-excerpt">
                <span>Excerpt: “{selectedExcerpt.slice(0, 140)}”</span>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => setSelectedExcerpt("")}
                  title="Clear excerpt"
                >
                  <X />
                </button>
              </div>
            )}
            <div style={{ display: "flex", gap: 6 }}>
              <input
                className="input"
                placeholder="Ask a question about this thread or selection…"
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !busy) void runAsk();
                }}
              />
              <button
                type="button"
                className="btn compact primary"
                disabled={busy !== null || !question.trim()}
                onClick={() => void runAsk()}
              >
                {busy === "ask" ? "…" : "Ask"}
              </button>
            </div>
            {answer && <div className="ai-answer">{answer}</div>}
          </div>

          <div className="ai-card">
            <div className="ai-card-head">
              <span>
                <Bot style={{ width: 12, height: 12, marginRight: 4, verticalAlign: "-1px" }} />
                One-Click Reply Draft
              </span>
            </div>
            <input
              className="input"
              placeholder="Optional instructions (e.g. politely decline and propose Tuesday)…"
              value={draftInstructions}
              onChange={(e) => setDraftInstructions(e.target.value)}
            />
            <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
              <button
                type="button"
                className="btn compact primary"
                disabled={busy !== null}
                onClick={() => void runDraft(true)}
              >
                <Send />
                {busy === "draft" ? "Drafting…" : "Stage for Approval"}
              </button>
              <button
                type="button"
                className="btn compact"
                disabled={busy !== null}
                onClick={() => void runDraft(false)}
              >
                <FileEdit />
                Insert into Reply
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
