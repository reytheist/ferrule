import { useEffect, useMemo, useRef, useState } from "react";
import { CalendarClock, ChevronDown, Clock, Lock, X } from "lucide-react";
import type { CreateDraftInput } from "../../types/mail";
import { AvailabilityPicker } from "../calendar/AvailabilityPicker";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { anchorBelow } from "../ui/anchor";

function splitRecipients(raw: string): string[] {
  return raw
    .split(/[,\s;]+/)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
}

export function ComposeModal({ app }: { app: MailApp }) {
  const firstAccount = app.accounts[0];
  const [accountId, setAccountId] = useState(() => app.scopeAccount ?? firstAccount?.id ?? "");
  const [to, setTo] = useState("");
  const [cc, setCc] = useState("");
  const [showCc, setShowCc] = useState(false);
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [snippetsOpen, setSnippetsOpen] = useState(false);
  const [availOpen, setAvailOpen] = useState(false);
  const bodyRef = useRef<HTMLTextAreaElement>(null);

  const fieldsRef = useRef({ accountId, to, cc, showCc, subject, body });
  useEffect(() => {
    fieldsRef.current = { accountId, to, cc, showCc, subject, body };
  });

  // Snippet inserted from the command bar lands in the open compose body.
  const composeInsert = app.composeInsert;
  useEffect(() => {
    if (!composeInsert) return;
    const text = composeInsert.body;
    setBody((prev) => (prev ? `${prev}\n\n${text}` : text));
    app.clearComposeInsert();
    bodyRef.current?.focus();
  }, [composeInsert, app]);

  const account = app.accounts.find((a) => a.id === accountId) ?? firstAccount;

  const recipientSuggestions = useMemo(() => {
    const token = to.split(/[,\s;]+/).pop()?.trim().toLowerCase() ?? "";
    if (token.length < 1) return [];
    const pool: Array<{ label: string; email: string; badge: string }> = [];
    for (const m of app.staffDirectory) {
      pool.push({ label: `${m.name} (@${m.handle})`, email: m.email, badge: "Staff" });
    }
    for (const c of app.showContacts) {
      for (const email of c.emails) {
        pool.push({
          label: c.organization ? `${c.name} · ${c.organization}` : c.name,
          email,
          badge: c.category,
        });
      }
    }
    const existing = new Set(splitRecipients(to).map((e) => e.toLowerCase()));
    return pool
      .filter(
        (entry) =>
          !existing.has(entry.email.toLowerCase()) &&
          (entry.label.toLowerCase().includes(token) ||
            entry.email.toLowerCase().includes(token)),
      )
      .slice(0, 5);
  }, [to, app.staffDirectory, app.showContacts]);

  function applySuggestion(email: string): void {
    const parts = to.split(/([,\s;]+)/);
    if (parts.length > 0) parts[parts.length - 1] = email;
    const joined = parts.join("");
    setTo(joined.endsWith(", ") ? joined : `${joined}, `);
  }

  function submit(mode: "now" | "later", sendAt?: number): void {
    const fields = fieldsRef.current;
    const target = app.accounts.find((a) => a.id === fields.accountId);
    if (!target) {
      setError("Add an account before sending.");
      return;
    }
    const recipients = splitRecipients(fields.to);
    if (recipients.length === 0) {
      setError("Add at least one recipient.");
      return;
    }
    const input: CreateDraftInput = {
      accountId: target.id,
      replyToThread: null,
      to: recipients,
      cc: fields.showCc ? splitRecipients(fields.cc) : [],
      bcc: [],
      subject: fields.subject,
      bodyText: fields.body,
    };
    setError(null);
    if (mode === "now") {
      void app.submitDraft(input, { kind: "undoable", undoSecs: 7 }).then(() => app.closeCompose());
    } else if (sendAt !== undefined) {
      void app.submitDraft(input, { kind: "scheduled", sendAt }).then(() => app.closeCompose());
    }
  }

  if (!app.composeOpen) return null;

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeCompose();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="New message">
        <div className="modal-head">
          <h2>New message</h2>
          <button className="icon-btn" onClick={app.closeCompose} title="Close (Esc)">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="field">
            <label>From</label>
            <div className="compose-from">
              <span
                className={`from-pill${account ? ` ${spaceClass(account.color)}` : ""}`}
                title="Outgoing identity is locked to the selected space"
              >
                <Lock />
                <span className="fp-label">From:</span>
                {account?.identities.find((i) => i.isPrimary)?.address ?? account?.email ?? "—"}
              </span>
              <select
                className="select"
                value={accountId}
                onChange={(event) => setAccountId(event.target.value)}
              >
                {app.accounts.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.displayName} · {entry.email}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="field">
            <label>To</label>
            <input
              className="input"
              placeholder="alice@example.com, or type a staff name / show contact…"
              value={to}
              onChange={(event) => setTo(event.target.value)}
            />
            {recipientSuggestions.length > 0 && (
              <div className="recipient-suggestions">
                {recipientSuggestions.map((item) => (
                  <button
                    key={item.email}
                    type="button"
                    className="recipient-suggestion-item"
                    onClick={() => applySuggestion(item.email)}
                  >
                    <span>{item.label}</span>
                    <span className="hint-text">{item.email}</span>
                    <span className="kbd">{item.badge}</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          {showCc ? (
            <div className="field">
              <label>Cc</label>
              <input
                className="input"
                placeholder="cc@example.com"
                value={cc}
                onChange={(event) => setCc(event.target.value)}
              />
            </div>
          ) : (
            <button className="tpl-btn" style={{ alignSelf: "flex-start" }} onClick={() => setShowCc(true)}>
              + Cc
            </button>
          )}

          <div className="field">
            <label>Subject</label>
            <input
              className="input"
              placeholder="Subject"
              value={subject}
              onChange={(event) => setSubject(event.target.value)}
            />
          </div>

          <div className="field">
            <label>Message</label>
            <textarea
              ref={bodyRef}
              className="textarea"
              placeholder="Write your message…"
              value={body}
              onChange={(event) => setBody(event.target.value)}
            />
          </div>

          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          <div className="tpl-wrap" style={{ display: "flex", gap: 6 }}>
            <button
              type="button"
              className="tpl-btn"
              onClick={() => setAvailOpen(true)}
              title="Insert open calendar availability slots"
            >
              <CalendarClock style={{ width: 12, height: 12 }} />
              Availability
            </button>
            <button className="tpl-btn" onClick={() => setSnippetsOpen((v) => !v)}>
              Snippets
              <ChevronDown />
            </button>
            {snippetsOpen && (
              <div
                className="cmd-backdrop"
                style={{ background: "transparent", zIndex: 14 }}
                onClick={() => setSnippetsOpen(false)}
              />
            )}
            {snippetsOpen && (
              <div className="snippet-menu" style={{ bottom: "calc(100% + 6px)", top: "auto" }}>
                {app.templates.length === 0 && app.sharedTemplates.length === 0 ? (
                  <span className="cmd-empty">No snippets yet.</span>
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
                              const text = template.bodyText;
                              setBody((prev) => (prev ? `${prev}\n\n${text}` : text));
                              if (!subject && template.subject) setSubject(template.subject);
                              setSnippetsOpen(false);
                              bodyRef.current?.focus();
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
                              const text = template.bodyText;
                              setBody((prev) => (prev ? `${prev}\n\n${text}` : text));
                              setSnippetsOpen(false);
                              bodyRef.current?.focus();
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
          <span style={{ flex: 1 }} />
          <button
            className="btn"
            onClick={(event) =>
              app.requestSendLater(anchorBelow(event.currentTarget), (until) => submit("later", until))
            }
          >
            <Clock />
            Send Later
          </button>
          <button className="btn primary" onClick={() => submit("now")}>
            Send
          </button>
        </div>
      </div>
      {availOpen && (
        <AvailabilityPicker
          onInsert={(text) => {
            setBody((prev) => (prev ? `${prev}\n\n${text}` : text));
            bodyRef.current?.focus();
          }}
          onClose={() => setAvailOpen(false)}
        />
      )}
    </div>
  );
}
