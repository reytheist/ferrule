import {
  CheckCircle2,
  ChevronLeft,
  Lock,
  MessageSquare,
  RefreshCw,
  ShieldCheck,
  Users,
} from "lucide-react";
import type { SharedMessageSnapshot } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { formatFull, formatThreadTime, initialsOf } from "../ui/format";
import { StaffCollabSidebar } from "./StaffCollabSidebar";

function snapshotSender(msg: SharedMessageSnapshot): string {
  return msg.fromName ?? msg.fromEmail;
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

  return (
    <section className="list-col">
      <div className="list-tabs">
        <span className="list-title" style={{ flex: 1 }}>
          Shared with Me
        </span>
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
        {app.sharedThreads.length === 0 ? (
          <div className="empty-state">
            <Users />
            <strong>No shared threads yet</strong>
            <span>
              Open a thread in a Pigment Staff space and click Share with Staff to collaborate here.
            </span>
          </div>
        ) : (
          app.sharedThreads.map((item) => {
            const selected = app.selectedSharedId === item.id;
            return (
              <div
                key={item.id}
                className={`thread-row${selected ? " selected" : ""}`}
                onClick={() => void app.openSharedThread(item.id)}
                role="button"
                tabIndex={0}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void app.openSharedThread(item.id);
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

  if (!shared) {
    return (
      <main className="read-col">
        <div className="read-card">
          <div className="read-placeholder">
            <Users />
            <strong>No shared thread open</strong>
            <span>Pick a shared thread on the left to review snapshots, comments, and reply drafts.</span>
          </div>
        </div>
      </main>
    );
  }

  const last = shared.messages[shared.messages.length - 1];
  const ownsSourceMailbox =
    shared.sourceAccountId !== null &&
    app.accounts.some((a) => a.id === shared.sourceAccountId);

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
          <span className="badge badge-staff">
            <Users style={{ width: 10, height: 10 }} />
            Shared snapshot · {shared.ownerEmail}
          </span>
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
          <div className="read-scroll">
            <div className="read-header">
              <div className="read-subject">{shared.subject}</div>
              <div className="privacy-ribbon">
                <span className="pr-left">
                  <ShieldCheck />
                  Sanitized Staff Sync snapshot · trackers stripped · no IMAP access shared
                </span>
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
                  srcDoc={message.bodySrcdoc}
                  title={`Shared message from ${snapshotSender(message)}`}
                />
              </div>
            ))}
          </div>

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
