import { useEffect, useMemo, useState } from "react";
import {
  CheckCircle2,
  Contact,
  Edit3,
  Lock,
  MessageSquare,
  Send,
  ShieldAlert,
  UserPlus,
  Users,
} from "lucide-react";
import type {
  HandoffStatus,
  SharedThreadView,
  ShowContactView,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { formatFull } from "../ui/format";

const CATEGORY_LABEL: Record<ShowContactView["category"], string> = {
  sponsor: "Sponsor",
  vendor: "Vendor",
  instructor: "Instructor",
  vip: "VIP",
  volunteer: "Volunteer",
  venue: "Venue",
  other: "Contact",
};

function renderMentions(text: string): Array<string | { mention: string; key: number }> {
  const parts: Array<string | { mention: string; key: number }> = [];
  const re = /(@[a-zA-Z0-9_-]+)/g;
  let last = 0;
  let match = re.exec(text);
  let key = 0;
  while (match !== null) {
    if (match.index > last) parts.push(text.slice(last, match.index));
    parts.push({ mention: match[0], key: key++ });
    last = match.index + match[0].length;
    match = re.exec(text);
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

interface StaffCollabSidebarProps {
  app: MailApp;
  shared: SharedThreadView | null;
  participantEmails: string[];
  senderName: string | null;
  senderEmail: string | null;
  canLoadApprovedToReply: boolean;
}

export function StaffCollabSidebar({
  app,
  shared,
  participantEmails,
  senderName,
  senderEmail,
  canLoadApprovedToReply,
}: StaffCollabSidebarProps) {
  const [commentText, setCommentText] = useState("");
  const [draftBody, setDraftBody] = useState("");
  const [takeOverConfirm, setTakeOverConfirm] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewHandle, setReviewHandle] = useState("");
  const [reviewNote, setReviewNote] = useState("");
  const [busy, setBusy] = useState(false);

  const matchedContact = useMemo(() => {
    const lower = new Set(participantEmails.map((e) => e.trim().toLowerCase()));
    return (
      app.showContacts.find((c) => c.emails.some((e) => lower.has(e.toLowerCase()))) ?? null
    );
  }, [app.showContacts, participantEmails]);

  const draft = shared?.draft ?? null;
  const myHandle = app.staffWorkspace.staffHandle;
  const isEditingMine =
    draft?.lockState === "editing" &&
    draft.editorHandle !== null &&
    draft.editorHandle === myHandle;
  const isLockedByOther =
    draft?.lockState === "editing" &&
    draft.editorHandle !== null &&
    draft.editorHandle !== myHandle;

  useEffect(() => {
    setDraftBody(draft?.bodyText ?? "");
    setTakeOverConfirm(false);
    setReviewOpen(false);
  }, [shared?.id, draft?.rev, draft?.bodyText]);

  if (!shared && !matchedContact) return null;

  async function postComment(): Promise<void> {
    if (!shared || !commentText.trim() || busy) return;
    setBusy(true);
    try {
      await app.postSharedComment({
        sharedThreadId: shared.id,
        bodyText: commentText.trim(),
      });
      setCommentText("");
    } finally {
      setBusy(false);
    }
  }

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

  async function quickAddContact(): Promise<void> {
    if (!senderEmail || busy) return;
    setBusy(true);
    try {
      await app.persistShowContact({
        id: null,
        name: senderName || senderEmail.split("@")[0] || senderEmail,
        emails: [senderEmail],
        category: "other",
        organization: null,
        tier: null,
        notes: "",
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <aside className="collab-sidebar" aria-label="Staff collaboration sidebar">
      {matchedContact ? (
        <section className="collab-card">
          <div className="collab-card-head">
            <Contact />
            <span>Show Contact</span>
            <span className={`contact-cat-pill cat-${matchedContact.category}`}>
              {CATEGORY_LABEL[matchedContact.category]}
            </span>
          </div>
          <div className="contact-card-name">{matchedContact.name}</div>
          {matchedContact.organization && (
            <div className="contact-card-sub">
              {matchedContact.organization}
              {matchedContact.tier ? ` · ${matchedContact.tier}` : ""}
            </div>
          )}
          {!matchedContact.organization && matchedContact.tier && (
            <div className="contact-card-sub">{matchedContact.tier}</div>
          )}
          <div className="contact-card-emails">{matchedContact.emails.join(", ")}</div>
          {matchedContact.notes && <p className="contact-card-notes">{matchedContact.notes}</p>}
          <button
            type="button"
            className="t-btn"
            style={{ alignSelf: "flex-start" }}
            onClick={app.openContacts}
          >
            Edit in Directory
          </button>
        </section>
      ) : (
        shared &&
        senderEmail && (
          <section className="collab-card compact-row">
            <span className="hint-text" style={{ flex: 1 }}>
              {senderName ?? senderEmail} is not in Show Contacts.
            </span>
            <button
              type="button"
              className="t-btn"
              disabled={busy}
              onClick={() => void quickAddContact()}
            >
              <UserPlus />
              Save Contact
            </button>
          </section>
        )
      )}

      {shared && (
        <>
          <section className="collab-card">
            <div className="collab-card-head">
              <Users />
              <span>Shared Handoff</span>
              <span className={`handoff-pill status-${shared.status}`}>{shared.status}</span>
            </div>
            <div className="hint-text">
              Shared by <strong>{shared.ownerName}</strong> ({shared.ownerEmail}) ·{" "}
              {shared.visibility === "allStaff"
                ? "All Staff"
                : shared.invitedHandles.map((h) => `@${h}`).join(", ")}
            </div>
            <div className="handoff-controls">
              <label className="handoff-field">
                <span>Owner</span>
                <select
                  className="select compact-select"
                  value={shared.assigneeHandle ?? ""}
                  onChange={(event) =>
                    void app.updateHandoff({
                      sharedThreadId: shared.id,
                      assigneeHandle: event.target.value || null,
                      status: shared.status,
                    })
                  }
                >
                  <option value="">Unassigned</option>
                  {app.staffDirectory.map((m) => (
                    <option key={m.id} value={m.handle}>
                      {m.name} (@{m.handle})
                    </option>
                  ))}
                </select>
              </label>
              <label className="handoff-field">
                <span>Status</span>
                <select
                  className="select compact-select"
                  value={shared.status}
                  onChange={(event) =>
                    void app.updateHandoff({
                      sharedThreadId: shared.id,
                      assigneeHandle: shared.assigneeHandle,
                      status: event.target.value as HandoffStatus,
                    })
                  }
                >
                  <option value="open">Open</option>
                  <option value="waiting">Waiting</option>
                  <option value="done">Done</option>
                </select>
              </label>
            </div>
          </section>

          <section className="collab-card">
            <div className="collab-card-head">
              <Edit3 />
              <span>Shared Reply Draft</span>
              {draft?.lockState === "editing" && (
                <span className="draft-lock-pill editing">
                  <Lock />
                  Editing: {draft.editorName ?? `@${draft.editorHandle}`}
                </span>
              )}
              {draft?.lockState === "reviewRequested" && (
                <span className="draft-lock-pill review">Review requested</span>
              )}
              {draft?.lockState === "approved" && (
                <span className="draft-lock-pill approved">
                  <CheckCircle2 />
                  Approved
                </span>
              )}
            </div>

            {isEditingMine ? (
              <>
                <textarea
                  className="textarea collab-draft-area"
                  rows={4}
                  placeholder="Draft the shared reply here…"
                  value={draftBody}
                  onChange={(event) => setDraftBody(event.target.value)}
                />
                <div className="collab-actions">
                  <button
                    type="button"
                    className="t-btn on"
                    disabled={busy}
                    onClick={() =>
                      void handleDraftAction({
                        kind: "save",
                        subject:
                          draft?.subject ??
                          (/^re:/i.test(shared.subject)
                            ? shared.subject
                            : `Re: ${shared.subject}`),
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
                    className="t-btn"
                    disabled={busy}
                    onClick={() => setReviewOpen((v) => !v)}
                  >
                    Request Review
                  </button>
                  <button
                    type="button"
                    className="t-btn"
                    disabled={busy}
                    onClick={() => void handleDraftAction({ kind: "approve" })}
                  >
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
                          subject:
                            draft?.subject ??
                            (/^re:/i.test(shared.subject)
                              ? shared.subject
                              : `Re: ${shared.subject}`),
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
                  <pre className="collab-draft-preview">{draft.bodyText}</pre>
                ) : (
                  <span className="hint-text">No shared reply draft yet.</span>
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
                          className="t-btn on"
                          disabled={busy}
                          onClick={() => void handleDraftAction({ kind: "takeOver" })}
                        >
                          Confirm Take Over
                        </button>
                        <button
                          type="button"
                          className="t-btn"
                          onClick={() => setTakeOverConfirm(false)}
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <button
                        type="button"
                        className="t-btn"
                        disabled={busy}
                        onClick={() => setTakeOverConfirm(true)}
                      >
                        Take Over
                      </button>
                    )
                  ) : (
                    <button
                      type="button"
                      className="t-btn"
                      disabled={busy}
                      onClick={() => void handleDraftAction({ kind: "claim" })}
                    >
                      <Edit3 />
                      {draft ? "Edit Draft" : "Start Reply Draft"}
                    </button>
                  )}

                  {draft && draft.lockState !== "approved" && !isLockedByOther && (
                    <button
                      type="button"
                      className="t-btn"
                      disabled={busy}
                      onClick={() => void handleDraftAction({ kind: "approve" })}
                    >
                      <CheckCircle2 />
                      Approve
                    </button>
                  )}

                  {draft?.lockState === "approved" &&
                    canLoadApprovedToReply &&
                    draft.bodyText.trim() !== "" && (
                      <button
                        type="button"
                        className="t-btn on"
                        onClick={() => app.loadApprovedDraftToReply(draft.bodyText)}
                      >
                        <Send />
                        Load to Reply &amp; Send
                      </button>
                    )}
                </div>
              </>
            )}
          </section>

          <section className="collab-card collab-comments-card">
            <div className="collab-card-head">
              <MessageSquare />
              <span>Internal Comments</span>
              <span className="nav-count">{shared.comments.length}</span>
            </div>

            <div className="collab-comment-list">
              {shared.comments.length === 0 ? (
                <span className="hint-text">
                  Private staff discussion — external recipients never see these notes.
                </span>
              ) : (
                shared.comments.map((comment) => (
                  <div key={comment.id} className="collab-comment">
                    <div className="collab-comment-meta">
                      <strong>{comment.authorName}</strong>
                      <span className="handle">@{comment.authorHandle}</span>
                      <span className="time">{formatFull(comment.createdAt)}</span>
                    </div>
                    <div className="collab-comment-body">
                      {renderMentions(comment.bodyText).map((part, idx) =>
                        typeof part === "string" ? (
                          <span key={idx}>{part}</span>
                        ) : (
                          <span key={`m-${part.key}`} className="mention-tag">
                            {part.mention}
                          </span>
                        ),
                      )}
                    </div>
                  </div>
                ))
              )}
            </div>

            {app.staffDirectory.length > 0 && (
              <div className="mention-bar">
                {app.staffDirectory.slice(0, 5).map((member) => (
                  <button
                    key={member.id}
                    type="button"
                    className="mention-chip"
                    onClick={() =>
                      setCommentText((prev) =>
                        prev && !prev.endsWith(" ")
                          ? `${prev} @${member.handle} `
                          : `${prev}@${member.handle} `,
                      )
                    }
                  >
                    @{member.handle}
                  </button>
                ))}
              </div>
            )}

            <div className="collab-comment-input-row">
              <input
                className="input compact-input"
                placeholder="Add internal note (@mention)…"
                value={commentText}
                onChange={(event) => setCommentText(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    void postComment();
                  }
                }}
              />
              <button
                type="button"
                className="btn compact primary"
                disabled={busy || !commentText.trim()}
                onClick={() => void postComment()}
              >
                Post
              </button>
            </div>
          </section>
        </>
      )}
    </aside>
  );
}
