import { useState } from "react";
import {
  Code2,
  Moon,
  PenTool,
  Plus,
  SlidersHorizontal,
  Sparkles,
  Sun,
  Terminal,
  Trash2,
  X,
} from "lucide-react";
import type { AccountId, SignatureId } from "../../types/mail";
import { plainTextToHtml } from "../compose/RichEditor";
import type { MailApp, TextSize } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";

export function SettingsModal({ app }: { app: MailApp }) {
  const [busy, setBusy] = useState(false);
  const [sigAccountId, setSigAccountId] = useState<string>("");
  const [editingSigId, setEditingSigId] = useState<SignatureId | null>(null);
  const [sigName, setSigName] = useState("");
  const [sigBody, setSigBody] = useState("");
  const [sigDefault, setSigDefault] = useState(true);

  if (!app.settingsOpen) return null;

  const devTools = app.settings.developerToolsEnabled;
  const activeSigAccount =
    (app.accounts.find((a) => a.id === sigAccountId) ?? app.accounts[0])?.id ?? null;
  const accountSigs = activeSigAccount
    ? app.signatures.filter((s) => s.accountId === activeSigAccount)
    : [];

  async function saveSig(): Promise<void> {
    if (!activeSigAccount || !sigName.trim()) return;
    setBusy(true);
    try {
      await app.persistSignature({
        id: editingSigId,
        accountId: activeSigAccount as AccountId,
        name: sigName.trim(),
        bodyText: sigBody,
        bodyHtml: plainTextToHtml(sigBody),
        isDefault: sigDefault,
      });
      setEditingSigId(null);
      setSigName("");
      setSigBody("");
      setSigDefault(false);
    } finally {
      setBusy(false);
    }
  }

  async function toggleDeveloperTools(): Promise<void> {
    if (busy) return;
    setBusy(true);
    try {
      await app.setDeveloperToolsEnabled(!devTools);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) app.closeSettings();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Settings">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <SlidersHorizontal style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Settings</h2>
          </div>
          <button
            className="icon-btn"
            disabled={busy}
            onClick={app.closeSettings}
            title="Close (Esc)"
          >
            <X />
          </button>
        </div>

        <div className="modal-body">
          <section className="settings-section">
            <div className="settings-section-title">Appearance &amp; Layout</div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Theme</span>
                <span className="hint-text">Switch between dark and light Mosaic surfaces.</span>
              </div>
              <button type="button" className="btn compact" onClick={app.toggleTheme}>
                {app.theme === "dark" ? <Sun /> : <Moon />}
                {app.theme === "dark" ? "Dark" : "Light"}
              </button>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Reading pane</span>
                <span className="hint-text">Side-by-side split view or classic single-pane list.</span>
              </div>
              <button type="button" className="btn compact" onClick={app.toggleLayout}>
                {app.layout === "split" ? "Split view" : "Classic view"}
              </button>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Interface text size</span>
                <span className="hint-text">
                  Scales sidebar, thread list, toolbar, and dialog labels independently of email text.
                </span>
              </div>
              <div className="segmented-control" role="group" aria-label="Interface text size">
                {(["small", "medium", "large"] as TextSize[]).map((size) => (
                  <button
                    key={size}
                    type="button"
                    className={`seg-btn${app.chromeTextSize === size ? " active" : ""}`}
                    onClick={() => app.setChromeTextSize(size)}
                  >
                    {size === "small" ? "Small" : size === "medium" ? "Medium" : "Large"}
                  </button>
                ))}
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Reader text size</span>
                <span className="hint-text">
                  Scales message body, subject, headers, and reply box independently of the interface.
                </span>
              </div>
              <div className="segmented-control" role="group" aria-label="Reader text size">
                {(["small", "medium", "large"] as TextSize[]).map((size) => (
                  <button
                    key={size}
                    type="button"
                    className={`seg-btn${app.readerTextSize === size ? " active" : ""}`}
                    onClick={() => app.setReaderTextSize(size)}
                  >
                    {size === "small" ? "Small" : size === "medium" ? "Medium" : "Large"}
                  </button>
                ))}
              </div>
            </div>
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Account Spaces</div>
            {app.accounts.length === 0 ? (
              <span className="hint-text">No accounts connected yet.</span>
            ) : (
              <div className="space-list-mini">
                {app.accounts.map((account) => (
                  <div key={account.id} className="space-list-row">
                    <span className={`acct-dot ${spaceClass(account.color)}`} />
                    <div className="space-list-main">
                      <span className="space-list-name">{account.displayName}</span>
                      <span className="space-list-email">{account.email}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
            <span className="hint-text">
              All mail stays 100% local in SQLite, with outgoing identities locked per space.
            </span>
          </section>

          <section className="settings-section">
            <div className="settings-section-title">
              <PenTool style={{ width: 12, height: 12, marginRight: 5, verticalAlign: "-1px" }} />
              Named Signatures
            </div>
            {app.accounts.length === 0 ? (
              <span className="hint-text">Connect an account to configure signatures.</span>
            ) : (
              <div className="signature-manager">
                <select
                  className="select"
                  value={activeSigAccount ?? ""}
                  onChange={(event) => {
                    setSigAccountId(event.target.value);
                    setEditingSigId(null);
                    setSigName("");
                    setSigBody("");
                  }}
                >
                  {app.accounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.displayName} · {account.email}
                    </option>
                  ))}
                </select>

                {accountSigs.length > 0 && (
                  <div className="signature-list">
                    {accountSigs.map((sig) => (
                      <div key={sig.id} className="signature-item">
                        <div className="signature-item-main">
                          <strong>{sig.name}</strong>
                          {sig.isDefault && <span className="kbd">Default</span>}
                          <span className="hint-text">{sig.bodyText.slice(0, 60)}</span>
                        </div>
                        <button
                          type="button"
                          className="btn compact"
                          onClick={() => {
                            setEditingSigId(sig.id);
                            setSigName(sig.name);
                            setSigBody(sig.bodyText);
                            setSigDefault(sig.isDefault);
                          }}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          className="icon-btn"
                          title="Delete signature"
                          onClick={() => void app.removeSignature(sig.id)}
                        >
                          <Trash2 />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                <div className="signature-editor-box">
                  <input
                    className="input"
                    placeholder="Signature name (e.g. Formal, Casual)"
                    value={sigName}
                    onChange={(event) => setSigName(event.target.value)}
                  />
                  <textarea
                    className="textarea"
                    rows={3}
                    placeholder="Signature text…"
                    value={sigBody}
                    onChange={(event) => setSigBody(event.target.value)}
                  />
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <label className="hint-text" style={{ display: "flex", alignItems: "center", gap: 5 }}>
                      <input
                        type="checkbox"
                        checked={sigDefault}
                        onChange={(event) => setSigDefault(event.target.checked)}
                      />
                      Default for this account
                    </label>
                    <span style={{ flex: 1 }} />
                    {editingSigId && (
                      <button
                        type="button"
                        className="btn compact"
                        onClick={() => {
                          setEditingSigId(null);
                          setSigName("");
                          setSigBody("");
                        }}
                      >
                        Cancel
                      </button>
                    )}
                    <button
                      type="button"
                      className="btn compact primary"
                      disabled={busy || !sigName.trim()}
                      onClick={() => void saveSig()}
                    >
                      <Plus />
                      {editingSigId ? "Update" : "Add Signature"}
                    </button>
                  </div>
                </div>
              </div>
            )}
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Setup &amp; Guidance</div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Feature Tour</span>
                <span className="hint-text">
                  Walk through Account Spaces, Command Bar, scheduling, and Calendar.
                </span>
              </div>
              <button type="button" className="btn compact" onClick={app.startTour}>
                <Sparkles />
                Replay Feature Tour
              </button>
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Onboarding</span>
                <span className="hint-text">
                  Re-open the guided setup flow for appearance and email accounts.
                </span>
              </div>
              <button type="button" className="btn compact" onClick={app.openOnboarding}>
                <SlidersHorizontal />
                Run Onboarding
              </button>
            </div>
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Developer Tools &amp; Agent Bridge</div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">
                  <Code2 style={{ width: 13, height: 13 }} />
                  Enable Developer Tools
                </span>
                <span className="hint-text">
                  Unlocks local MCP server controls, AgentPort configuration, and the Agent Draft
                  Approval Queue.
                </span>
              </div>
              <button
                type="button"
                role="switch"
                aria-checked={devTools}
                aria-label="Enable Developer Tools"
                disabled={busy}
                className={`toggle-switch${devTools ? " on" : ""}`}
                onClick={() => void toggleDeveloperTools()}
              >
                <span className="toggle-thumb" />
              </button>
            </div>

            {devTools && (
              <div className="dev-status-card">
                <div className="dev-status-head">
                  <Terminal style={{ width: 13, height: 13 }} />
                  <span>AgentPort &amp; Local MCP</span>
                  <span className="space-kind-badge staff">Enabled</span>
                </div>
                <span className="hint-text">
                  <code>mail_core::agent::AgentPort</code> exposes read, search, and{" "}
                  <code>stage_draft</code> only. Agent drafts always land in{" "}
                  <code>PendingApproval</code> and cannot be sent without explicit human approval.
                </span>
              </div>
            )}
          </section>
        </div>

        <div className="modal-foot">
          <span className="hint-text" style={{ flex: 1 }}>
            Credentials stay strictly in your OS keychain.
          </span>
          <button type="button" className="btn primary" disabled={busy} onClick={app.closeSettings}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
