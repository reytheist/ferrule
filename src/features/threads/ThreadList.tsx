import { AlertTriangle, Archive, Clock, ExternalLink, Inbox, RefreshCw, Star, Trash2 } from "lucide-react";
import type { MouseEvent, ReactNode } from "react";
import type { InboxCategory, ThreadSummary } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { anchorBelow } from "../ui/anchor";
import { categoryLabel, formatThreadTime, viewTitle } from "../ui/format";
import { assertNever } from "../../lib/assertNever";

const TABS: InboxCategory[] = ["primary", "receipts", "newsletters"];

function tabCount(app: MailApp, category: InboxCategory): number {
  const counts = app.counts;
  if (!counts) return 0;
  switch (category) {
    case "primary":
      return counts.primary;
    case "receipts":
      return counts.receipts;
    case "newsletters":
      return counts.newsletters;
    case "all":
      return counts.inbox;
    default:
      return assertNever(category);
  }
}

function RowBadges({ thread }: { thread: ThreadSummary }) {
  const badges: ReactNode[] = [
    <span key="acct" className={`badge badge-acct ${spaceClass(thread.accountColor)}`}>
      <span className="acct-dot" />
      {thread.accountLabel}
    </span>,
  ];
  if (thread.snoozedUntil !== null) {
    badges.push(
      <span key="snooze" className="badge badge-snooze">
        Snoozed
      </span>,
    );
  }
  if (thread.followUpDue !== null) {
    badges.push(
      <span key="follow" className="badge badge-followup">
        Follow-up due
      </span>,
    );
  }
  if (thread.trackersBlocked > 0) {
    badges.push(
      <span key="shield" className="badge badge-shield">
        {thread.trackersBlocked} pixel{thread.trackersBlocked > 1 ? "s" : ""} blocked
      </span>,
    );
  }
  return <div className="row-badges">{badges}</div>;
}

function ThreadRow({ app, thread }: { app: MailApp; thread: ThreadSummary }) {
  const selected = app.selectedId === thread.id;
  const unread = thread.unreadCount > 0;

  const snoozeAt = (event: MouseEvent<HTMLElement>) => {
    event.stopPropagation();
    app.requestSnooze(thread.id, anchorBelow(event.currentTarget));
  };

  return (
    <div
      className={`thread-row${selected ? " selected" : ""}${unread ? " unread" : ""}`}
      onClick={() => app.openThread(thread.id)}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          app.openThread(thread.id);
        }
      }}
    >
      <div className="row-top">
        {unread && <span className="unread-dot" />}
        <span className="thread-sender">{thread.senderName}</span>
        {thread.messageCount > 1 && <span className="thread-count">{thread.messageCount}</span>}
        <span className="thread-time">{formatThreadTime(thread.lastAt)}</span>
      </div>
      <div className="thread-subject">{thread.subject}</div>
      <div className="thread-snippet">{thread.snippet}</div>
      <RowBadges thread={thread} />

      <div className="hover-actions">
        <button
          className="icon-act"
          title="Archive (E)"
          onClick={(event) => {
            event.stopPropagation();
            void app.archiveThread(thread.id);
          }}
        >
          <Archive />
        </button>
        <button className="icon-act" title="Snooze (H)" onClick={snoozeAt}>
          <Clock />
        </button>
        <button
          className={`icon-act${thread.starred ? " starred" : ""}`}
          title="Star (S)"
          onClick={(event) => {
            event.stopPropagation();
            void app.starThread(thread.id);
          }}
        >
          <Star />
        </button>
      </div>
    </div>
  );
}

function emptyTitle(app: MailApp): string {
  if (app.view.kind === "inbox") return "Inbox zero in this view.";
  if (app.view.kind === "search") return "No matching mail.";
  return `Nothing in ${viewTitle(app.view)}.`;
}

function emptyHint(app: MailApp): string {
  if (app.view.kind === "search") return "Try a shorter query or another keyword.";
  if (app.view.kind === "inbox") return "New mail lands here as it syncs.";
  return "Threads you move here appear in this list.";
}

export function ThreadList({ app }: { app: MailApp }) {
  if (app.counts === null) {
    return (
      <section className="list-col">
        <div className="list-tabs">
          <span className="list-title">Loading mail…</span>
        </div>
        <div className="thread-list" />
      </section>
    );
  }

  const inboxView = app.view.kind === "inbox" ? app.view : null;
  const focusedAccount = app.scopeAccount
    ? app.accounts.find((a) => a.id === app.scopeAccount) ?? null
    : null;
  const syncIssue =
    focusedAccount &&
    (focusedAccount.sync.kind === "blocked" || focusedAccount.sync.kind === "backoff")
      ? focusedAccount.sync.kind === "blocked"
        ? focusedAccount.sync.reason
        : "Temporary connection error — retrying automatically."
      : null;

  return (
    <section className="list-col">
      {inboxView ? (
        <div className="list-tabs">
          {TABS.map((category) => (
            <button
              key={category}
              className={`list-tab${inboxView.category === category ? " active" : ""}`}
              onClick={() => app.selectCategory(category)}
            >
              {categoryLabel(category)}
              <span className="tab-badge">{tabCount(app, category)}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className="list-tabs">
          <span className="list-title">{viewTitle(app.view)}</span>
        </div>
      )}

      {focusedAccount && syncIssue && (
        <div className="sync-banner" role="status">
          <div className="sync-banner-head">
            <AlertTriangle />
            <span>{syncIssue}</span>
          </div>
          <div className="sync-banner-actions">
            {(focusedAccount.provider === "gmail" || focusedAccount.provider === "outlook") && (
              <button
                className="t-btn"
                onClick={() => void app.reauthAccount(focusedAccount.id)}
                disabled={app.syncing}
                title="Re-authenticate in browser"
              >
                <ExternalLink />
                Sign in again
              </button>
            )}
            <button
              className="t-btn"
              onClick={() => void app.syncNow()}
              disabled={app.syncing}
            >
              <RefreshCw />
              {app.syncing ? "Syncing…" : "Retry sync"}
            </button>
            <button
              className="t-btn"
              onClick={() => void app.removeAccount(focusedAccount.id)}
              title="Remove this account"
            >
              <Trash2 />
              Remove
            </button>
          </div>
        </div>
      )}

      <div className="thread-list">
        {app.threads.length === 0 ? (
          <div className="empty-state">
            <Inbox />
            <strong>{emptyTitle(app)}</strong>
            <span>{emptyHint(app)}</span>
          </div>
        ) : (
          app.threads.map((thread) => <ThreadRow key={thread.id} app={app} thread={thread} />)
        )}
      </div>
    </section>
  );
}
