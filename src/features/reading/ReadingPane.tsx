import { useEffect, useRef, useState } from "react";
import {
  Archive,
  CalendarClock,
  CalendarDays,
  CalendarPlus,
  CheckSquare,
  ChevronDown,
  ChevronLeft,
  Clock,
  Flag,
  Image,
  Lock,
  MapPin,
  Paperclip,
  Share2,
  ShieldCheck,
  Star,
  Users,
} from "lucide-react";
import type { CalendarInviteView, MessageView, RsvpStatus, ThreadView } from "../../types/mail";
import { AvailabilityPicker } from "../calendar/AvailabilityPicker";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { StaffCollabSidebar } from "../staff/StaffCollabSidebar";
import { anchorBelow } from "../ui/anchor";
import { formatFull, initialsOf } from "../ui/format";

function formatSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return `${bytes} B`;
}

function senderOf(message: MessageView): string {
  return message.fromName ?? message.fromEmail;
}

function PrivacyRibbon({ app, thread }: { app: MailApp; thread: ThreadView }) {
  const last = thread.messages[thread.messages.length - 1];
  if (!last) return null;
  const blocked = last.privacy.trackers.length;
  const unsubscribe = last.unsubscribe;

  return (
    <div className="privacy-ribbon">
      <span className="pr-left">
        <ShieldCheck />
        {blocked > 0
          ? `Blocked ${blocked} spy pixel${blocked > 1 ? "s" : ""} · DKIM verified`
          : "No trackers found · DKIM verified"}
      </span>
      {unsubscribe && (
        <button
          onClick={() =>
            app.showToast(
              unsubscribe.oneClick
                ? "Sent 1-click unsubscribe & muted sender"
                : "Unsubscribe request sent",
            )
          }
        >
          {unsubscribe.oneClick ? "1-Click Unsubscribe" : "Unsubscribe"}
        </button>
      )}
    </div>
  );
}

function AttachmentChip({ name, size }: { name: string; size: number }) {
  return (
    <span className="attachment-chip">
      <Paperclip />
      {name}
      <span className="size">{formatSize(size)}</span>
    </span>
  );
}

function CalendarInviteCard({
  app,
  invite,
}: {
  app: MailApp;
  invite: CalendarInviteView;
}) {
  const when = new Date(invite.startAt).toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
  const endTime = new Date(invite.endAt).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  const rsvpOptions: Array<{ status: RsvpStatus; label: string }> = [
    { status: "accepted", label: "Accept" },
    { status: "tentative", label: "Maybe" },
    { status: "declined", label: "Decline" },
  ];

  return (
    <div className="ics-invite-card">
      <div className="ics-invite-head">
        <CalendarDays style={{ width: 15, height: 15, color: "var(--app-accent)", flexShrink: 0 }} />
        <div className="ics-invite-main">
          <strong>{invite.summary}</strong>
          <div className="hint-text">
            {when} – {endTime}
            {invite.location && (
              <>
                {" · "}
                <MapPin style={{ width: 10, height: 10, display: "inline", verticalAlign: "-1px" }} />{" "}
                {invite.location}
              </>
            )}
          </div>
          {invite.organizerEmail && (
            <div className="hint-text">
              Organizer: {invite.organizerName ?? invite.organizerEmail}
            </div>
          )}
        </div>
        <span className="kbd">
          {invite.rsvpStatus === "needsAction" ? "Invite" : invite.rsvpStatus}
        </span>
      </div>

      <div className="ics-invite-actions">
        {rsvpOptions.map((opt) => (
          <button
            key={opt.status}
            type="button"
            className={`btn compact${invite.rsvpStatus === opt.status ? " primary" : ""}`}
            onClick={() =>
              void app.rsvpInvite({
                messageId: invite.messageId,
                uid: invite.uid,
                status: opt.status,
              })
            }
          >
            {opt.label}
          </button>
        ))}
        <span style={{ flex: 1 }} />
        <button
          type="button"
          className="btn compact"
          onClick={() =>
            void app.addInviteToCalendar({
              messageId: invite.messageId,
              uid: invite.uid,
              department: "operations",
            })
          }
        >
          <CalendarPlus />
          {invite.addedEventId ? "In Pigment Calendar" : "Add to Pigment Calendar"}
        </button>
      </div>
    </div>
  );
}

function MessageBubble({ app, message }: { app: MailApp; message: MessageView }) {
  const invites = message.invites ?? [];
  return (
    <div className="msg-bubble">
      <div className="msg-meta">
        <span className="avatar">{initialsOf(senderOf(message))}</span>
        <div>
          <div className="msg-from">{senderOf(message)}</div>
          <div className="msg-to">to {message.to.join(", ")}</div>
        </div>
        <span className="msg-time">{formatFull(message.date)}</span>
      </div>
      {invites.map((invite) => (
        <CalendarInviteCard key={`${invite.messageId}-${invite.uid}`} app={app} invite={invite} />
      ))}
      <iframe
        className="msg-frame"
        sandbox=""
        srcDoc={message.bodySrcdoc}
        title={`Message from ${senderOf(message)}`}
      />
      {message.attachments.length > 0 && (
        <div className="msg-attachments">
          {message.attachments.map((attachment) => (
            <AttachmentChip key={attachment.id} name={attachment.filename} size={attachment.size} />
          ))}
        </div>
      )}
    </div>
  );
}

function CollapsedMessage({ message, onToggle }: { message: MessageView; onToggle: () => void }) {
  return (
    <button className="collapsed-msg" onClick={onToggle}>
      <strong>{senderOf(message)}</strong>
      <span className="cm-snip">{message.snippet}</span>
      <span className="cm-time">{formatFull(message.date)}</span>
      <ChevronDown style={{ width: 11, height: 11 }} />
    </button>
  );
}

function ReplyDock({ app, thread }: { app: MailApp; thread: ThreadView }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [availOpen, setAvailOpen] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const account = app.accounts.find((a) => a.id === thread.accountId);
  const identity = account?.identities.find((i) => i.isPrimary) ?? account?.identities[0];
  const fromAddress = identity?.address ?? thread.accountEmail;
  const last = thread.messages[thread.messages.length - 1];
  const replyTarget = last ? senderOf(last) : "this thread";

  useEffect(() => {
    if (app.replyNonce > 0) inputRef.current?.focus();
  }, [app.replyNonce]);

  return (
    <div className={`reply-dock ${spaceClass(thread.accountColor)}`}>
      <div className="from-guard">
        <span className="from-pill" title="Replies are locked to this identity">
          <Lock />
          <span className="fp-label">From:</span>
          {fromAddress}
        </span>
        <div className="tpl-wrap" style={{ display: "flex", gap: 6 }}>
          <button className="tpl-btn" onClick={() => setAvailOpen(true)} title="Insert open calendar availability slots">
            <CalendarClock style={{ width: 12, height: 12 }} />
            Availability
          </button>
          <button className="tpl-btn" onClick={() => setMenuOpen((v) => !v)}>
            Snippets
            <ChevronDown />
          </button>
          {menuOpen && (
            <div
              className="cmd-backdrop"
              style={{ background: "transparent", zIndex: 14 }}
              onClick={() => setMenuOpen(false)}
            />
          )}
          {menuOpen && (
            <div className="snippet-menu">
              {app.templates.length === 0 && app.sharedTemplates.length === 0 ? (
                <span className="cmd-empty">No snippets yet — create one in Templates.</span>
              ) : (
                <>
                  {app.sharedTemplates.length > 0 && (
                    <>
                      <span className="snippet-group-label">Shared Staff Templates</span>
                      {app.sharedTemplates.map((template) => (
                        <button
                          key={template.id}
                          className="snippet-item shared"
                          onClick={() => {
                            app.insertTextSnippet(template.bodyText);
                            setMenuOpen(false);
                          }}
                        >
                          <span className="name">
                            <span className="staff-tpl-dot" />
                            {template.name}
                          </span>
                          <span className="kbd">{template.shortcut ?? template.category}</span>
                        </button>
                      ))}
                    </>
                  )}
                  {app.templates.length > 0 && (
                    <>
                      {app.sharedTemplates.length > 0 && (
                        <span className="snippet-group-label">Personal Snippets</span>
                      )}
                      {app.templates.map((template) => (
                        <button
                          key={template.id}
                          className="snippet-item"
                          onClick={() => {
                            app.insertTemplate(template);
                            setMenuOpen(false);
                          }}
                        >
                          <span className="name">{template.name}</span>
                          {template.shortcut && <span className="kbd">{template.shortcut}</span>}
                        </button>
                      ))}
                    </>
                  )}
                </>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="reply-row">
        <textarea
          ref={inputRef}
          className="reply-input"
          rows={2}
          placeholder={`Reply to ${replyTarget}…`}
          value={app.replyText}
          onChange={(event) => app.setReplyText(event.target.value)}
        />
        <div className="send-split">
          <button
            className="send-main"
            disabled={app.replyText.trim() === ""}
            onClick={() => void app.sendReply({ kind: "undoable", undoSecs: 7 })}
          >
            Send
          </button>
          <button
            className="send-later-btn"
            title="Send Later"
            onClick={(event) =>
              app.requestSendLater(anchorBelow(event.currentTarget), (until) =>
                app.sendReply({ kind: "scheduled", sendAt: until }),
              )
            }
          >
            <Clock />
          </button>
        </div>
      </div>
      {availOpen && (
        <AvailabilityPicker
          onInsert={(text) => {
            app.setReplyText(app.replyText ? `${app.replyText}\n\n${text}` : text);
            inputRef.current?.focus();
          }}
          onClose={() => setAvailOpen(false)}
        />
      )}
    </div>
  );
}

export function ReadingPane({ app }: { app: MailApp }) {
  const thread = app.thread;

  if (!thread) {
    return (
      <main className="read-col">
        <div className="read-card">
          <div className="read-placeholder">
            <ShieldCheck />
            <strong>No thread open</strong>
            <span>Pick a conversation on the left, or press</span>
            <span className="hint">j / k to move · / to search</span>
          </div>
        </div>
      </main>
    );
  }

  const earlier = thread.messages.slice(0, -1);
  const current = thread.messages[thread.messages.length - 1];
  const imagesAllowed = app.remoteContent !== "blockAll";
  const account = app.accounts.find((a) => a.id === thread.accountId);
  const isStaffSpace = account?.spaceKind === "pigmentStaff";
  const shared =
    app.activeSharedThread && app.activeSharedThread.sourceThreadId === thread.id
      ? app.activeSharedThread
      : null;
  const participantEmails = Array.from(
    new Set(
      thread.messages.flatMap((m) => [m.fromEmail, ...m.to, ...m.cc]).filter(Boolean),
    ),
  );

  return (
    <main className="read-col">
      <div className="read-card">
        <div className="read-toolbar">
          {app.layout === "classic" && (
            <button className="t-btn" onClick={app.backToList}>
              <ChevronLeft />
              Inbox
            </button>
          )}
          <button className="t-btn" onClick={() => void app.archiveThread(thread.id)}>
            <Archive />
            Archive <span className="k">E</span>
          </button>
          <button
            className="t-btn"
            onClick={(event) => app.requestSnooze(thread.id, anchorBelow(event.currentTarget))}
          >
            <Clock />
            Snooze <span className="k">H</span>
          </button>
          <button
            className="t-btn"
            onClick={(event) => app.requestFollowUp(thread.id, anchorBelow(event.currentTarget))}
          >
            <Flag />
            Follow-up <span className="k">F</span>
          </button>
          <button
            className={`t-btn${thread.starred ? " on" : ""}`}
            onClick={() => void app.starThread(thread.id)}
          >
            <Star />
            Star <span className="k">S</span>
          </button>
          <button
            className={`t-btn${imagesAllowed ? " on" : ""}`}
            onClick={app.toggleRemote}
            title={
              imagesAllowed
                ? "Remote images load (trackers stay stripped)"
                : "Blocked: remote images never load"
            }
          >
            <Image />
            {imagesAllowed ? "Images on" : "Images blocked"}
          </button>
          <button
            className="t-btn"
            onClick={() =>
              app.openNewActionItemModal({
                title: thread.subject,
                sourceThreadId: thread.id,
              })
            }
            title="Create a Pigment Staff Action Item linked to this email thread"
          >
            <CheckSquare />
            + Action Item
          </button>
          <button
            className={`t-btn${shared ? " on" : ""}`}
            disabled={!isStaffSpace}
            onClick={app.openShareModal}
            title={
              isStaffSpace
                ? shared
                  ? "Thread shared with Pigment Staff — click to update sharing"
                  : "Share sanitized snapshot with Pigment Staff"
                : "Staff Sharing is disabled for Personal spaces"
            }
          >
            {shared ? <Users /> : <Share2 />}
            {shared ? "Shared" : "Share with Staff"}
          </button>
        </div>

        <div className="read-split-body">
          <div className="read-scroll">
            <div className="read-header">
              <div className="read-subject">{thread.subject}</div>
              <div className="thread-meta-row">
                <span className={`badge badge-acct ${spaceClass(thread.accountColor)}`}>
                  <span className="acct-dot" />
                  {thread.accountLabel} · {thread.accountEmail}
                </span>
                {shared && (
                  <span className="badge badge-staff">
                    <Users style={{ width: 9, height: 9 }} />
                    Shared · {shared.status}
                  </span>
                )}
                {thread.snoozedUntil !== null && (
                  <span className="badge badge-snooze">Snoozed</span>
                )}
                {thread.followUpDue !== null && (
                  <span className="badge badge-followup">Follow-up due</span>
                )}
              </div>
              <PrivacyRibbon app={app} thread={thread} />
            </div>

            {earlier.map((message) =>
              app.expanded.has(message.id) ? (
                <MessageBubble key={message.id} app={app} message={message} />
              ) : (
                <CollapsedMessage
                  key={message.id}
                  message={message}
                  onToggle={() => app.toggleExpand(message.id)}
                />
              ),
            )}

            {current && <MessageBubble app={app} message={current} />}
          </div>

          <StaffCollabSidebar
            app={app}
            shared={shared}
            participantEmails={participantEmails}
            senderName={current?.fromName ?? null}
            senderEmail={current?.fromEmail ?? null}
            canLoadApprovedToReply={true}
          />
        </div>

        <ReplyDock app={app} thread={thread} />
      </div>
    </main>
  );
}
