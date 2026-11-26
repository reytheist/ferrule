import { useEffect, useState } from "react";
import {
  CheckCircle2,
  ChevronLeft,
  Edit3,
  FileEdit,
  HelpCircle,
  Lock,
  Mail,
  MessageSquare,
  RefreshCw,
  Send,
  ShieldAlert,
  ShieldCheck,
  Users,
} from "lucide-react";
import type { SharedMessageSnapshot } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { applyReaderTextSize } from "../reading/readerTextSize";
import { formatFull, formatThreadTime, initialsOf } from "../ui/format";
import { StaffCollabSidebar } from "./StaffCollabSidebar";

function snapshotSender(msg: SharedMessageSnapshot): string {
  return msg.fromName ?? msg.fromEmail;
}

function SharedDraftGuide() {
  return (
    <div className="shared-draft-guide">
      <div className="shared-draft-guide-title">How Shared Drafts work</div>
      <ol className="shared-draft-steps">
        <li>
          <strong>Share a thread</strong> — Open an email in a <em>Pigment Staff</em> space and
          click <strong>Share with Staff</strong>. Only a sanitized snapshot is shared; your mailbox
          stays private.
        </li>
        <li>
          <strong>Claim the lock &amp; write</strong> — Click <strong>Start Shared Reply Draft</strong>{" "}
          (or <strong>Edit Shared Draft</strong>) so one person edits at a time, then click{" "}
          <strong>Save Draft</strong>.
        </li>
        <li>
          <strong>Review &amp; approve</strong> — Click <strong>Request Review</strong> to unlock
          and ping teammates, or click <strong>Approve Draft</strong> when the wording is ready.
        </li>
        <li>
          <strong>Send from your mailbox</strong> — Only the original mailbox owner can send. Once
          approved, click <strong>Open in Mailbox &amp; Load Reply</strong> to send it from your
          own address.
        </li>
      </ol>
    </div>
  );
}

export function SharedThreadsList({ app }: { app: MailApp }) {
  const ws = app.staffWorkspace;
  const outboxBadge =
    ws.connected && (ws.status === "offline" || ws.status === "error" || ws.pendingOutboxCount > 0)
      ? ws.status === "offline"
        ? ws.pendingOutboxCount > 0
          ? `Offline (${ws.pendingOutboxCount} pending)`
          : "Offline"
        : ws.status === "error"
          ? ws.pendingOutboxCount > 0
            ? `Sync error (${ws.pendingOutboxCount} pending)`
            : "Sync error"
          : `${ws.pendingOutboxCount} pending`
      : null;

  const draftThreads = app.sharedThreads.filter(
    (item) => item.hasDraft || item.draftLockState !== "unlocked",
  );
  const visibleThreads =
    app.sharedListTab === "drafts" ? draftThreads : app.sharedThreads;

  return (
    <section className="list-col">
      <div className="list-tabs">
        <button
          type="button"
          className={`list-tab${app.sharedListTab === "all" ? " active" : ""}`}
          onClick={() => app.setSharedListTab("all")}
        >
          Shared
          <span className="tab-badge">{app.sharedThreads.length}</span>
        </button>
        <button
          type="button"
          className={`list-tab${app.sharedListTab === "drafts" ? " active" : ""}`}
          onClick={() => app.setSharedListTab("drafts")}
        >
          Drafts
          <span className="tab-badge">{draftThreads.length}</span>
        </button>
        <span style={{ flex: 1 }} />
        {outboxBadge && (
          <span
            className="badge"
            title={ws.lastError ?? "Edits queued locally and will sync automatically on reconnect"}
          >
            {outboxBadge}
          </span>
        )}
        {ws.connected && (
          <button
            className="t-btn"
            onClick={() => void app.syncStaffWorkspace()}
            title="Sync Pigment Staff workspace"
          >
            <RefreshCw />
            Sync
          </button>
        )}
      </div>

      <div className="thread-list">
        {visibleThreads.length === 0 ? (
          <div className="empty-state">
            {app.sharedListTab === "drafts" ? <FileEdit /> : <Users />}
            <strong>
              {app.sharedListTab === "drafts"
                ? "No active shared drafts"
                : "No shared threads yet"}
            </strong>
            <span>
              {app.sharedListTab === "drafts"
                ? "Open any shared thread and start a shared reply draft to review and collaborate here."
                : "Open a thread in a Pigment Staff space and click Share with Staff to collaborate here."}
            </span>
          </div>
        ) : (
          visibleThreads.map((item) => {
            const selected = app.selectedSharedId === item.id;
            return (
              <div
                key={item.id}
                className={`thread-row${selected ? " selected" : ""}`}
                onClick={() =>
                  void app.openSharedThread(
                    item.id,
                    app.sharedListTab === "drafts" ? "draft" : "thread",
                  )
                }
                role="button"
                tabIndex={0}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void app.openSharedThread(
                      item.id,
                      app.sharedListTab === "drafts" ? "draft" : "thread",
                    );
                  }
                }}
              >
                <div className="row-top">
                  <span className="thread-sender">{item.ownerName}</span>
                  {item.messageCount > 1 && (
                    <span className="thread-count">{item.messageCount}</span>
                  )}
                  <span className="thread-time">{formatThreadTime(item.updatedAt)}</span>
                </div>
                <div className="thread-subject">{item.subject}</div>
                <div className="thread-snippet">{item.snippet}</div>
                <div className="row-badges">
                  <span className={`handoff-pill status-${item.status}`}>{item.status}</span>
                  {item.assigneeHandle && (
                    <span className="badge badge-staff">@{item.assigneeHandle}</span>
                  )}
                  {item.commentCount > 0 && (
                    <span className="badge">
                      <MessageSquare style={{ width: 9, height: 9 }} />
                      {item.commentCount}
                    </span>
                  )}
                  {item.draftLockState === "editing" && (
                    <span className="draft-lock-pill editing">
                      <Lock />
                      Editing: {item.draftEditorName ?? "Staff"}
                    </span>
                  )}
                  {item.draftLockState === "reviewRequested" && (
                    <span className="draft-lock-pill review">Review</span>
                  )}
                  {item.draftLockState === "approved" && (
                    <span className="draft-lock-pill approved">
                      <CheckCircle2 />
                      Approved
                    </span>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}

export function SharedThreadReadingPane({ app }: { app: MailApp }) {
  const shared = app.activeSharedThread;
  const draft = shared?.draft ?? null;
  const myHandle = app.staffWorkspace.staffHandle;

  const [draftBody, setDraftBody] = useState("");
  const [takeOverConfirm, setTakeOverConfirm] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewHandle, setReviewHandle] = useState("");
  const [reviewNote, setReviewNote] = useState("");
  const [showGuide, setShowGuide] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setDraftBody(draft?.bodyText ?? "");
    setTakeOverConfirm(false);
    setReviewOpen(false);
  }, [shared?.id, draft?.rev, draft?.bodyText]);

  if (!shared) {
    return (
      <main className="read-col">
        <div className="read-card">
          <div className="read-placeholder">
            {app.sharedListTab === "drafts" ? <FileEdit /> : <Users />}
            <strong>
              {app.sharedListTab === "drafts" ? "No shared draft open" : "No shared thread open"}
            </strong>
            <span>
              {app.sharedListTab === "drafts"
                ? "Pick a shared draft on the left to read, review, approve, or edit it in full."
                : "Pick a shared thread on the left to review snapshots, comments, and shared reply drafts."}
            </span>
            {app.sharedListTab === "drafts" && <SharedDraftGuide />}
          </div>
        </div>
      </main>
    );
  }

  const last = shared.messages[shared.messages.length - 1];
  const ownsSourceMailbox =
    shared.sourceAccountId !== null &&
    app.accounts.some((a) => a.id === shared.sourceAccountId);

  const isEditingMine =
    draft?.lockState === "editing" &&
    draft.editorHandle !== null &&
    draft.editorHandle === myHandle;
  const isLockedByOther =
    draft?.lockState === "editing" &&
    draft.editorHandle !== null &&
    draft.editorHandle !== myHandle;

  const draftSubject =
    draft?.subject ??
    (/^re:/i.test(shared.subject) ? shared.subject : `Re: ${shared.subject}`);

  async function handleDraftAction(
    action: Parameters<MailApp["mutateSharedDraft"]>[0]["action"],
  ): Promise<void> {
    if (!shared || busy) return;
    setBusy(true);
    try {
      await app.mutateSharedDraft({ sharedThreadId: shared.id, action });
      setTakeOverConfirm(false);
      setReviewOpen(false);
      setReviewNote("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="read-col">
      <div className="read-card">
        <div className="read-toolbar">
          {app.layout === "classic" && (
            <button className="t-btn" onClick={app.backToList}>
              <ChevronLeft />
              Shared
            </button>
          )}
          <div className="segmented-mini" role="tablist" aria-label="Shared reading mode">
            <button
              type="button"
              role="tab"
              aria-selected={app.sharedPaneTab === "thread"}
              className={app.sharedPaneTab === "thread" ? "active" : ""}
              onClick={() => app.setSharedPaneTab("thread")}
            >
              Thread ({shared.messages.length})
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={app.sharedPaneTab === "draft"}
              className={app.sharedPaneTab === "draft" ? "active" : ""}
              onClick={() => app.setSharedPaneTab("draft")}
            >
              Shared Draft
              {draft?.lockState === "approved"
                ? " · Approved"
                : draft?.lockState === "reviewRequested"
                  ? " · Review"
                  : draft?.lockState === "editing"
                    ? " · Editing"
                    : ""}
            </button>
          </div>
          <span className="badge badge-staff">
            <Users style={{ width: 10, height: 10 }} />
            {shared.ownerEmail}
          </span>
          <span style={{ flex: 1 }} />
          {ownsSourceMailbox && shared.sourceThreadId && (
            <button
              className="t-btn on"
              onClick={() => {
                app.selectView({ kind: "inbox", category: "all" });
                if (shared.sourceThreadId) app.openThread(shared.sourceThreadId);
              }}
            >
              Open in my mailbox
            </button>
          )}
        </div>

        <div className="read-split-body">
          {app.sharedPaneTab === "draft" ? (
            <div className="read-scroll">
              <div className="read-header">
                <div className="read-subject">{draftSubject}</div>
                <div className="pane-badge-strip">
                  {draft?.lockState === "editing" && (
                    <span className="draft-lock-pill editing">
                      <Lock />
                      Editing: {draft.editorName ?? `@${draft.editorHandle}`}
                    </span>
                  )}
                  {draft?.lockState === "reviewRequested" && (
                    <span className="draft-lock-pill review">
                      Review requested
                      {draft.reviewRequestedBy ? ` by @${draft.reviewRequestedBy}` : ""}
                    </span>
                  )}
                  {draft?.lockState === "approved" && (
                    <span className="draft-lock-pill approved">
                      <CheckCircle2 />
                      Approved{draft.approvedBy ? ` by @${draft.approvedBy}` : ""}
                    </span>
                  )}
                  {(!draft || draft.lockState === "unlocked") && (
                    <span className="badge">Unlocked</span>
                  )}
                  {draft && <span className="kbd">Rev #{draft.rev}</span>}
                  {draft && (
                    <span className="hint-text">Updated {formatFull(draft.updatedAt)}</span>
                  )}
                  <button
                    type="button"
                    className={`sidebar-mini-btn${showGuide ? " active" : ""}`}
                    onClick={() => setShowGuide((v) => !v)}
                    title="How Shared Drafts work"
                  >
                    <HelpCircle style={{ width: 11, height: 11 }} />
                    {showGuide ? "Hide guide" : "How it works"}
                  </button>
                </div>
                {showGuide && <SharedDraftGuide />}
                {draft && (draft.to.length > 0 || draft.cc.length > 0) && (
                  <div className="draft-recipients-bar">
                    {draft.to.length > 0 && (
                      <span>
                        <strong>To:</strong> {draft.to.join(", ")}
                      </span>
                    )}
                    {draft.cc.length > 0 && (
                      <span>
                        <strong>Cc:</strong> {draft.cc.join(", ")}
                      </span>
                    )}
                  </div>
                )}
              </div>

              <div className="shared-draft-reader-card">
                {isEditingMine ? (
                  <>
                    <textarea
                      className="textarea shared-draft-reader-editor reader-prose"
                      rows={12}
                      placeholder="Write the shared reply draft here…"
                      value={draftBody}
                      onChange={(event) => setDraftBody(event.target.value)}
                    />
                    <div className="collab-actions">
                      <button
                        type="button"
                        className="btn compact primary"
                        disabled={busy}
                        onClick={() =>
                          void handleDraftAction({
                            kind: "save",
                            subject: draftSubject,
                            to: draft?.to ?? [],
                            cc: draft?.cc ?? [],
                            bodyText: draftBody,
                          })
                        }
                      >
                        Save Draft
                      </button>
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busy}
                        onClick={() => setReviewOpen((v) => !v)}
                      >
                        Request Review
                      </button>
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busy}
                        onClick={() => void handleDraftAction({ kind: "approve" })}
                      >
                        <CheckCircle2 style={{ width: 12, height: 12 }} />
                        Approve
                      </button>
                    </div>

                    {reviewOpen && (
                      <div className="review-request-box">
                        <select
                          className="select compact-select"
                          value={reviewHandle}
                          onChange={(event) => setReviewHandle(event.target.value)}
                        >
                          <option value="">Ping all staff</option>
                          {app.staffDirectory
                            .filter((m) => m.handle !== myHandle)
                            .map((m) => (
                              <option key={m.id} value={m.handle}>
                                Ping {m.name} (@{m.handle})
                              </option>
                            ))}
                        </select>
                        <input
                          className="input compact-input"
                          placeholder="Optional review note…"
                          value={reviewNote}
                          onChange={(event) => setReviewNote(event.target.value)}
                        />
                        <button
                          type="button"
                          className="btn compact primary"
                          disabled={busy}
                          onClick={() =>
                            void handleDraftAction({
                              kind: "save",
                              subject: draftSubject,
                              to: draft?.to ?? [],
                              cc: draft?.cc ?? [],
                              bodyText: draftBody,
                            }).then(() =>
                              handleDraftAction({
                                kind: "requestReview",
                                reviewerHandles: reviewHandle ? [reviewHandle] : [],
                                note: reviewNote.trim() || null,
                              }),
                            )
                          }
                        >
                          Release Lock &amp; Request Review
                        </button>
                      </div>
                    )}
                  </>
                ) : (
                  <>
                    {draft && draft.bodyText.trim() !== "" ? (
                      <div className="shared-draft-reader-body reader-prose">{draft.bodyText}</div>
                    ) : (
                      <div className="shared-draft-empty">
                        <FileEdit style={{ width: 20, height: 20, opacity: 0.45 }} />
                        <strong>No shared reply draft yet</strong>
                        <span className="hint-text">
                          Start a shared draft so Pigment teammates can review and approve the reply
                          before the mailbox owner sends it.
                        </span>
                        <SharedDraftGuide />
                      </div>
                    )}

                    <div className="collab-actions">
                      {isLockedByOther ? (
                        takeOverConfirm ? (
                          <div className="takeover-confirm">
                            <span className="hint-text">
                              <ShieldAlert style={{ width: 12, height: 12 }} /> Take lock from{" "}
                              {draft?.editorName ?? `@${draft?.editorHandle}`}?
                            </span>
                            <button
                              type="button"
                              className="btn compact primary"
                              disabled={busy}
                              onClick={() => void handleDraftAction({ kind: "takeOver" })}
                            >
                              Confirm Take Over
                            </button>
                            <button
                              type="button"
                              className="btn compact"
                              onClick={() => setTakeOverConfirm(false)}
                            >
                              Cancel
                            </button>
                          </div>
                        ) : (
                          <button
                            type="button"
                            className="btn compact"
                            disabled={busy}
                            onClick={() => setTakeOverConfirm(true)}
                          >
                            Take Over Lock
                          </button>
                        )
                      ) : (
                        <button
                          type="button"
                          className="btn compact primary"
                          disabled={busy}
                          onClick={() => void handleDraftAction({ kind: "claim" })}
                        >
                          <Edit3 style={{ width: 12, height: 12 }} />
                          {draft ? "Edit Shared Draft" : "Start Shared Reply Draft"}
                        </button>
                      )}

                      {draft && draft.lockState !== "approved" && !isLockedByOther && (
                        <button
                          type="button"
                          className="btn compact"
                          disabled={busy}
                          onClick={() => void handleDraftAction({ kind: "approve" })}
                        >
                          <CheckCircle2 style={{ width: 12, height: 12 }} />
                          Approve Draft
                        </button>
                      )}

                      {draft?.lockState === "approved" &&
                        ownsSourceMailbox &&
                        shared.sourceThreadId &&
                        draft.bodyText.trim() !== "" && (
                          <button
                            type="button"
                            className="btn compact primary"
                            onClick={() => {
                              const body = draft.bodyText;
                              app.selectView({ kind: "inbox", category: "all" });
                              if (shared.sourceThreadId) app.openThread(shared.sourceThreadId);
                              app.loadApprovedDraftToReply(body);
                            }}
                          >
                            <Send style={{ width: 12, height: 12 }} />
                            Open in Mailbox &amp; Load Reply
                          </button>
                        )}
                    </div>
                  </>
                )}
              </div>

              {last && (
                <div className="detail-links-card">
                  <div className="meeting-section-head">
                    <span className="meeting-section-title">
                      <Mail style={{ width: 12, height: 12 }} />
                      Originating Thread · {snapshotSender(last)}
                    </span>
                    <button
                      type="button"
                      className="sidebar-mini-btn"
                      onClick={() => app.setSharedPaneTab("thread")}
                    >
                      View full thread
                    </button>
                  </div>
                  <p className="hint-text" style={{ margin: 0 }}>
                    {shared.subject} — {last.snippet}
                  </p>
                </div>
              )}
            </div>
          ) : (
            <div className="read-scroll">
              <div className="read-header">
                <div className="read-subject">{shared.subject}</div>
                <div className="privacy-ribbon">
                  <span className="pr-left">
                    <ShieldCheck />
                    Sanitized Staff Sync snapshot · trackers stripped · no IMAP access shared
                  </span>
                  {draft && draft.bodyText.trim() !== "" && (
                    <button onClick={() => app.setSharedPaneTab("draft")}>
                      Open Shared Draft
                    </button>
                  )}
                </div>
              </div>

              {shared.messages.map((message) => (
                <div key={message.id} className="msg-bubble">
                  <div className="msg-meta">
                    <span className="avatar">{initialsOf(snapshotSender(message))}</span>
                    <div>
                      <div className="msg-from">{snapshotSender(message)}</div>
                      <div className="msg-to">to {message.to.join(", ")}</div>
                    </div>
                    <span className="msg-time">{formatFull(message.date)}</span>
                  </div>
                  <iframe
                    className="msg-frame"
                    sandbox=""
                    srcDoc={applyReaderTextSize(message.bodySrcdoc, app.readerTextSize)}
                    title={`Shared message from ${snapshotSender(message)}`}
                  />
                </div>
              ))}
            </div>
          )}

          <StaffCollabSidebar
            app={app}
            shared={shared}
            participantEmails={shared.participants}
            senderName={last?.fromName ?? null}
            senderEmail={last?.fromEmail ?? null}
            canLoadApprovedToReply={false}
          />
        </div>
      </div>
    </main>
  );
}
