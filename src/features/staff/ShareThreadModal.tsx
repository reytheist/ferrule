import { useState } from "react";
import { ShieldAlert, Users, X } from "lucide-react";
import type { ShareVisibility } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export function ShareThreadModal({ app }: { app: MailApp }) {
  const [visibility, setVisibility] = useState<ShareVisibility>("allStaff");
  const [selectedHandles, setSelectedHandles] = useState<string[]>([]);
  const [assigneeHandle, setAssigneeHandle] = useState<string>("");
  const [initialNote, setInitialNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!app.shareModalOpen || !app.thread) return null;

  const thread = app.thread;
  const account = app.accounts.find((a) => a.id === thread.accountId);
  const isStaffSpace = account?.spaceKind === "pigmentStaff";

  function toggleHandle(handle: string): void {
    setSelectedHandles((prev) =>
      prev.includes(handle) ? prev.filter((h) => h !== handle) : [...prev, handle],
    );
  }

  async function submit(): Promise<void> {
    if (!isStaffSpace) {
      setError("Personal spaces have Staff Sharing disabled.");
      return;
    }
    if (visibility === "specific" && selectedHandles.length === 0) {
      setError("Pick at least one teammate or choose All Staff.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await app.shareThread({
        threadId: thread.id,
        visibility,
        invitedHandles: visibility === "specific" ? selectedHandles : [],
        assigneeHandle: assigneeHandle || null,
        initialNote: initialNote.trim() || null,
      });
      app.closeShareModal();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) app.closeShareModal();
      }}
    >
      <div className="modal" role="dialog" aria-label="Share with Pigment Staff">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Users style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Share with Staff</h2>
          </div>
          <button
            className="icon-btn"
            disabled={busy}
            onClick={app.closeShareModal}
            title="Close (Esc)"
          >
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="share-thread-preview">
            <strong>{thread.subject}</strong>
            <span className="hint-text">
              Uploads a sanitized snapshot ({thread.messages.length} message
              {thread.messages.length === 1 ? "" : "s"}, trackers stripped) to Staff Sync. Teammates
              never get IMAP access to {thread.accountEmail}.
            </span>
          </div>

          {!isStaffSpace ? (
            <div className="staff-guardrail-box">
              <ShieldAlert />
              <div>
                <strong>Personal Space Guardrail</strong>
                <span>
                  This thread belongs to a Personal space. Switch the account to{" "}
                  <strong>Pigment Staff</strong> in the Space menu to enable Staff Sharing.
                </span>
              </div>
            </div>
          ) : !app.staffWorkspace.connected ? (
            <div className="staff-guardrail-box">
              <Users />
              <div>
                <strong>Connect Staff Sync first</strong>
                <span>Open Settings → Pigment Staff Sync to connect your workspace.</span>
              </div>
            </div>
          ) : (
            <>
              <div className="field">
                <label>Share with</label>
                <div className="segmented-row">
                  <button
                    type="button"
                    className={`btn compact${visibility === "allStaff" ? " primary" : ""}`}
                    onClick={() => setVisibility("allStaff")}
                  >
                    All Staff
                  </button>
                  <button
                    type="button"
                    className={`btn compact${visibility === "specific" ? " primary" : ""}`}
                    onClick={() => setVisibility("specific")}
                  >
                    Specific teammates
                  </button>
                </div>
              </div>

              {visibility === "specific" && (
                <div className="field">
                  <label>Invited teammates</label>
                  <div className="staff-chip-grid">
                    {app.staffDirectory.map((member) => {
                      const active = selectedHandles.includes(member.handle);
                      return (
                        <button
                          key={member.id}
                          type="button"
                          className={`staff-pick-chip${active ? " on" : ""}`}
                          onClick={() => toggleHandle(member.handle)}
                        >
                          <span>{member.name}</span>
                          <span className="handle">@{member.handle}</span>
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}

              <div className="field">
                <label>Hand off owner (optional)</label>
                <select
                  className="select"
                  value={assigneeHandle}
                  onChange={(event) => setAssigneeHandle(event.target.value)}
                >
                  <option value="">Unassigned</option>
                  {app.staffDirectory.map((member) => (
                    <option key={member.id} value={member.handle}>
                      {member.name} (@{member.handle}) — {member.role}
                    </option>
                  ))}
                </select>
              </div>

              <div className="field">
                <label>Internal note (optional — supports @mentions)</label>
                <textarea
                  className="textarea"
                  rows={3}
                  placeholder="e.g. @maya can you confirm booth A1 availability before we reply?"
                  value={initialNote}
                  onChange={(event) => setInitialNote(event.target.value)}
                />
              </div>
            </>
          )}

          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          <button type="button" className="btn" disabled={busy} onClick={app.closeShareModal}>
            Cancel
          </button>
          <span style={{ flex: 1 }} />
          {!app.staffWorkspace.connected ? (
            <button
              type="button"
              className="btn primary"
              onClick={() => {
                app.closeShareModal();
                app.openSettings();
              }}
            >
              Open Staff Sync Settings
            </button>
          ) : (
            <button
              type="button"
              className="btn primary"
              disabled={busy || !isStaffSpace}
              onClick={() => void submit()}
            >
              {busy ? "Sharing…" : "Share Snapshot"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
