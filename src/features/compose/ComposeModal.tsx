import { useEffect, useMemo, useRef, useState } from "react";
import {
  CalendarClock,
  ChevronDown,
  Clock,
  Lock,
  Paperclip,
  PenTool,
  X,
} from "lucide-react";
import type { CreateDraftInput, DraftAttachmentInput, SignatureView } from "../../types/mail";
import { AvailabilityPicker } from "../calendar/AvailabilityPicker";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { anchorBelow } from "../ui/anchor";
import { RichEditor, plainTextToHtml } from "./RichEditor";

const MAX_TOTAL_ATTACHMENT_BYTES = 25 * 1024 * 1024;

interface PendingAttachment extends DraftAttachmentInput {
  id: string;
  size: number;
}

function splitRecipients(raw: string): string[] {
  return raw
    .split(/[,\s;]+/)
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
}

function formatSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return `${bytes} B`;
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(new Error(`Could not read ${file.name}`));
    reader.readAsDataURL(file);
  });
}

export function ComposeModal({ app }: { app: MailApp }) {
  const firstAccount = app.accounts[0];
  const [accountId, setAccountId] = useState(() => app.scopeAccount ?? firstAccount?.id ?? "");
  const [to, setTo] = useState("");
  const [cc, setCc] = useState("");
  const [showCc, setShowCc] = useState(false);
  const [subject, setSubject] = useState("");
  const [editorMode, setEditorMode] = useState<"rich" | "plain">("rich");
  const [bodyText, setBodyText] = useState("");
  const [bodyHtml, setBodyHtml] = useState("");
  const [syncNonce, setSyncNonce] = useState(0);
  const [attachments, setAttachments] = useState<PendingAttachment[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const [selectedSigId, setSelectedSigId] = useState<string>("auto");
  const [error, setError] = useState<string | null>(null);
  const [snippetsOpen, setSnippetsOpen] = useState(false);
  const [availOpen, setAvailOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const account = app.accounts.find((a) => a.id === accountId) ?? firstAccount;
  const accountSignatures: SignatureView[] = useMemo(
    () => (account ? app.signatures.filter((s) => s.accountId === account.id) : []),
    [app.signatures, account],
  );
  const activeSignature: SignatureView | null = useMemo(() => {
    if (selectedSigId === "none") return null;
    if (selectedSigId === "auto") {
      return accountSignatures.find((s) => s.isDefault) ?? null;
    }
    return accountSignatures.find((s) => s.id === selectedSigId) ?? null;
  }, [accountSignatures, selectedSigId]);

  const fieldsRef = useRef({
    accountId,
    to,
    cc,
    showCc,
    subject,
    editorMode,
    bodyText,
    bodyHtml,
    attachments,
    activeSignature,
  });
  useEffect(() => {
    fieldsRef.current = {
      accountId,
      to,
      cc,
      showCc,
      subject,
      editorMode,
      bodyText,
      bodyHtml,
      attachments,
      activeSignature,
    };
  });

  function appendSnippet(text: string): void {
    setBodyText((prev) => (prev ? `${prev}\n\n${text}` : text));
    setBodyHtml((prev) => {
      const htmlSnippet = plainTextToHtml(text);
      return prev ? `${prev}<br><br>${htmlSnippet}` : htmlSnippet;
    });
    setSyncNonce((n) => n + 1);
  }

  // Snippet inserted from the command bar lands in the open compose body.
  const composeInsert = app.composeInsert;
  useEffect(() => {
    if (!composeInsert) return;
    appendSnippet(composeInsert.body);
    app.clearComposeInsert();
  }, [composeInsert, app]);

  async function addFiles(files: FileList | File[]): Promise<void> {
    const incoming = Array.from(files);
    if (incoming.length === 0) return;
    const currentTotal = attachments.reduce((sum, a) => sum + a.size, 0);
    let running = currentTotal;
    const added: PendingAttachment[] = [];
    for (const file of incoming) {
      if (running + file.size > MAX_TOTAL_ATTACHMENT_BYTES) {
        setError("Total attachment size cannot exceed 25 MB.");
        break;
      }
      try {
        const dataBase64 = await fileToBase64(file);
        added.push({
          id: `${file.name}-${file.size}-${Date.now()}-${Math.random()}`,
          filename: file.name,
          mime: file.type || "application/octet-stream",
          size: file.size,
          dataBase64,
        });
        running += file.size;
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to attach file.");
      }
    }
    if (added.length > 0) {
      setAttachments((prev) => [...prev, ...added]);
      setError(null);
    }
  }

  function removeAttachment(id: string): void {
    setAttachments((prev) => prev.filter((a) => a.id !== id));
  }

  function submit(mode: "now" | "later", sendAt?: number): void {
    const fields = fieldsRef.current;
    const target = app.accounts.find((a) => a.id === fields.accountId) ?? firstAccount;
    if (!target) {
      setError("Add an account before sending.");
      return;
    }
    const recipients = splitRecipients(fields.to);
    if (recipients.length === 0) {
      setError("Add at least one recipient.");
      return;
    }

    const sig = fields.activeSignature;
    const finalText = sig
      ? fields.bodyText.trim()
        ? `${fields.bodyText}\n\n-- \n${sig.bodyText}`
        : `-- \n${sig.bodyText}`
      : fields.bodyText;

    const finalHtml =
      fields.editorMode === "rich" || (sig && sig.bodyHtml.trim().length > 0)
        ? (() => {
            const baseHtml = fields.bodyHtml.trim() || plainTextToHtml(fields.bodyText);
            if (!sig) return baseHtml;
            const sigHtml = sig.bodyHtml.trim() || plainTextToHtml(sig.bodyText);
            return `${baseHtml}<br><br><div class="ferrule-signature">-- <br>${sigHtml}</div>`;
          })()
        : null;

    const input: CreateDraftInput = {
      accountId: target.id,
      replyToThread: null,
      to: recipients,
      cc: fields.showCc ? splitRecipients(fields.cc) : [],
      bcc: [],
      subject: fields.subject,
      bodyText: finalText,
      bodyHtml: finalHtml,
      attachments: fields.attachments.map(({ filename, mime, dataBase64 }) => ({
        filename,
        mime,
        dataBase64,
      })),
    };
    setError(null);
    if (mode === "now") {
      void app.submitDraft(input, { kind: "undoable", undoSecs: 7 }).then(() => app.closeCompose());
    } else if (sendAt !== undefined) {
      void app.submitDraft(input, { kind: "scheduled", sendAt }).then(() => app.closeCompose());
    }
  }

  if (!app.composeOpen) return null;

  const totalAttBytes = attachments.reduce((sum, a) => sum + a.size, 0);

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeCompose();
      }}
    >
      <div
        className={`modal wide${dragOver ? " compose-drop-active" : ""}`}
        role="dialog"
        aria-label="New message"
        onDragOver={(event) => {
          event.preventDefault();
          if (!dragOver) setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(event) => {
          event.preventDefault();
          setDragOver(false);
          if (event.dataTransfer.files?.length) {
            void addFiles(event.dataTransfer.files);
          }
        }}
      >
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
                onChange={(event) => {
                  setAccountId(event.target.value);
                  setSelectedSigId("auto");
                }}
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
              placeholder="alice@example.com"
              value={to}
              onChange={(event) => setTo(event.target.value)}
            />
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
            <button
              className="tpl-btn"
              style={{ alignSelf: "flex-start" }}
              onClick={() => setShowCc(true)}
            >
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
            <RichEditor
              mode={editorMode}
              onModeChange={setEditorMode}
              bodyText={bodyText}
              bodyHtml={bodyHtml}
              syncNonce={syncNonce}
              onChange={({ text, html }) => {
                setBodyText(text);
                setBodyHtml(html);
              }}
            />
          </div>

          {activeSignature && (
            <div className="compose-signature-preview">
              <span className="hint-text">Signature ({activeSignature.name}):</span>
              <div className="sig-preview-text">{activeSignature.bodyText}</div>
            </div>
          )}

          {attachments.length > 0 && (
            <div className="compose-attachments">
              {attachments.map((att) => (
                <span key={att.id} className="attachment-chip">
                  <Paperclip />
                  <span>{att.filename}</span>
                  <span className="size">{formatSize(att.size)}</span>
                  <button
                    type="button"
                    className="att-remove-btn"
                    title={`Remove ${att.filename}`}
                    onClick={() => removeAttachment(att.id)}
                  >
                    <X />
                  </button>
                </span>
              ))}
              <span className="hint-text" style={{ alignSelf: "center" }}>
                {formatSize(totalAttBytes)} / 25 MB
              </span>
            </div>
          )}

          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          <div className="tpl-wrap" style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <input
              ref={fileInputRef}
              type="file"
              multiple
              style={{ display: "none" }}
              onChange={(event) => {
                if (event.target.files?.length) {
                  void addFiles(event.target.files);
                  event.target.value = "";
                }
              }}
            />
            <button
              type="button"
              className="tpl-btn"
              onClick={() => fileInputRef.current?.click()}
              title="Attach files (or drag & drop)"
            >
              <Paperclip style={{ width: 12, height: 12 }} />
              Attach
            </button>

            {accountSignatures.length > 0 && (
              <label className="sig-picker-wrap" title="Signature">
                <PenTool style={{ width: 11, height: 11 }} />
                <select
                  className="sig-select"
                  value={selectedSigId}
                  onChange={(event) => setSelectedSigId(event.target.value)}
                >
                  <option value="auto">
                    Sig: {accountSignatures.find((s) => s.isDefault)?.name ?? "None"}
                  </option>
                  <option value="none">No signature</option>
                  {accountSignatures.map((sig) => (
                    <option key={sig.id} value={sig.id}>
                      {sig.name}
                    </option>
                  ))}
                </select>
              </label>
            )}

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
                {app.templates.length === 0 ? (
                  <span className="cmd-empty">No snippets yet.</span>
                ) : (
                  app.templates.map((template) => (
                    <button
                      key={template.id}
                      className="snippet-item"
                      onClick={() => {
                        appendSnippet(template.bodyText);
                        setSnippetsOpen(false);
                      }}
                    >
                      <span className="name">{template.name}</span>
                      {template.shortcut && <span className="kbd">{template.shortcut}</span>}
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
          <span style={{ flex: 1 }} />
          <button
            className="btn"
            onClick={(event) =>
              app.requestSendLater(anchorBelow(event.currentTarget), (until) =>
                submit("later", until),
              )
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
            appendSnippet(text);
          }}
          onClose={() => setAvailOpen(false)}
        />
      )}
    </div>
  );
}
