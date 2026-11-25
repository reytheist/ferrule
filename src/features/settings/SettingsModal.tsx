import { useState } from "react";
import {
  Code2,
  Moon,
  RefreshCw,
  Shield,
  SlidersHorizontal,
  Sparkles,
  Sun,
  Terminal,
  Unplug,
  Users,
  X,
} from "lucide-react";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";

export function SettingsModal({ app }: { app: MailApp }) {
  const [busy, setBusy] = useState(false);
  const [serverUrl, setServerUrl] = useState(() => app.staffWorkspace.serverUrl || "");
  const [token, setToken] = useState("");
  const [staffName, setStaffName] = useState(() => app.staffWorkspace.staffName || "");
  const [staffHandle, setStaffHandle] = useState(() => app.staffWorkspace.staffHandle || "");
  const [staffEmail, setStaffEmail] = useState(() => app.staffWorkspace.staffEmail || "");
  const [staffRole, setStaffRole] = useState(() => app.staffWorkspace.staffRole || "");
  const [staffError, setStaffError] = useState<string | null>(null);

  if (!app.settingsOpen) return null;

  const devTools = app.settings.developerToolsEnabled;
  const ws = app.staffWorkspace;
  const syncStatusLabel =
    ws.status === "offline"
      ? ws.pendingOutboxCount > 0
        ? `Offline (${ws.pendingOutboxCount} pending)`
        : "Offline"
      : ws.status === "error"
        ? ws.pendingOutboxCount > 0
          ? `Error (${ws.pendingOutboxCount} pending)`
          : "Error"
        : ws.pendingOutboxCount > 0
          ? `Syncing (${ws.pendingOutboxCount} pending)`
          : "Connected";

  async function toggleDeveloperTools(): Promise<void> {
    if (busy) return;
    setBusy(true);
    try {
      await app.setDeveloperToolsEnabled(!devTools);
    } finally {
      setBusy(false);
    }
  }

  async function connectWorkspace(): Promise<void> {
    if (busy) return;
    setBusy(true);
    setStaffError(null);
    try {
      await app.connectStaffWorkspace({
        serverUrl: serverUrl.trim(),
        token: token.trim(),
        staffName: staffName.trim(),
        staffHandle: staffHandle.trim().replace(/^@/, ""),
        staffEmail: staffEmail.trim(),
        staffRole: staffRole.trim(),
      });
      setToken("");
    } catch (err) {
      setStaffError(err instanceof Error ? err.message : String(err));
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
            <div className="settings-section-title">Pigment Staff Sync</div>
            {ws.connected ? (
              <div className="staff-ws-card">
                <div className="staff-ws-head">
                  <Users style={{ width: 14, height: 14, color: "var(--app-shield)" }} />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <strong>
                      {ws.staffName} (@{ws.staffHandle})
                    </strong>
                    <div className="hint-text">
                      {ws.staffRole} · {ws.staffEmail} · {ws.serverUrl}
                    </div>
                  </div>
                  <span className={`space-kind-badge${ws.status === "connected" ? " staff" : ""}`}>
                    {syncStatusLabel}
                  </span>
                </div>
                {ws.lastError && <span className="error-text">{ws.lastError}</span>}
                {app.staffDirectory.length > 0 && (
                  <div className="staff-dir-strip">
                    {app.staffDirectory.map((member) => (
                      <span
                        key={member.id}
                        className="staff-dir-chip"
                        title={`${member.name} - ${member.role} (${member.email})`}
                      >
                        @{member.handle}
                      </span>
                    ))}
                  </div>
                )}
                <div className="collab-actions">
                  <button
                    type="button"
                    className="btn compact"
                    disabled={busy}
                    onClick={() => void app.syncStaffWorkspace()}
                  >
                    <RefreshCw />
                    Sync Now
                  </button>
                  <button
                    type="button"
                    className="btn compact"
                    disabled={busy}
                    onClick={() => void app.disconnectStaffWorkspace()}
                  >
                    <Unplug />
                    Disconnect
                  </button>
                </div>
              </div>
            ) : (
              <div className="staff-connect-form">
                <div className="field-row">
                  <div className="field">
                    <label>Staff Sync Server URL</label>
                    <input
                      className="input"
                      placeholder="https://pigmentshow.us/api/ferrule"
                      value={serverUrl}
                      onChange={(event) => setServerUrl(event.target.value)}
                    />
                  </div>
                  <div className="field">
                    <label>Invite / Staff Token (OS Keychain)</label>
                    <input
                      className="input"
                      type="password"
                      placeholder="Paste invite token"
                      value={token}
                      onChange={(event) => setToken(event.target.value)}
                    />
                  </div>
                </div>
                <div className="field-row">
                  <div className="field">
                    <label>Your Name</label>
                    <input
                      className="input"
                      value={staffName}
                      onChange={(event) => setStaffName(event.target.value)}
                    />
                  </div>
                  <div className="field" style={{ maxWidth: 140 }}>
                    <label>@handle</label>
                    <input
                      className="input"
                      value={staffHandle}
                      onChange={(event) => setStaffHandle(event.target.value)}
                    />
                  </div>
                </div>
                <div className="field-row">
                  <div className="field">
                    <label>Pigment Email</label>
                    <input
                      className="input"
                      placeholder="e.g. info@pigmentshow.us"
                      value={staffEmail}
                      onChange={(event) => setStaffEmail(event.target.value)}
                    />
                  </div>
                  <div className="field">
                    <label>Role</label>
                    <input
                      className="input"
                      value={staffRole}
                      onChange={(event) => setStaffRole(event.target.value)}
                    />
                  </div>
                </div>
                {staffError && <span className="error-text">{staffError}</span>}
                <div className="collab-actions">
                  <button
                    type="button"
                    className="btn compact primary"
                    disabled={busy}
                    onClick={() => void connectWorkspace()}
                  >
                    Connect Staff Sync
                  </button>
                  <span className="hint-text">
                    Token is stored strictly in your OS keychain, never in SQLite or localStorage.
                  </span>
                </div>
              </div>
            )}
          </section>

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
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Account Spaces</div>
            {app.accounts.length === 0 ? (
              <span className="hint-text">No accounts connected yet.</span>
            ) : (
              <div className="space-list-mini">
                {app.accounts.map((account) => {
                  const isStaff = account.spaceKind === "pigmentStaff";
                  return (
                    <div key={account.id} className="space-list-row">
                      <span className={`acct-dot ${spaceClass(account.color)}`} />
                      <div className="space-list-main">
                        <span className="space-list-name">{account.displayName}</span>
                        <span className="space-list-email">{account.email}</span>
                      </div>
                      <span
                        className={`space-kind-badge${isStaff ? " staff" : ""}`}
                        title={
                          isStaff
                            ? "Eligible for Staff Sharing"
                            : "Personal space — Staff Sharing disabled by default"
                        }
                      >
                        {isStaff ? (
                          <>
                            <Users />
                            Pigment Staff
                          </>
                        ) : (
                          <>
                            <Shield />
                            Personal
                          </>
                        )}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
            <span className="hint-text">
              All mail stays 100% local by default. Personal spaces keep Staff Sharing disabled so
              private threads are never accidentally shared.
            </span>
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Setup &amp; Guidance</div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Feature Tour</span>
                <span className="hint-text">
                  Walk through Account Spaces, Command Bar, scheduling, and Pigment Staff tools.
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
                  Re-open the guided setup flow for appearance, accounts, and Pigment Staff Sync.
                </span>
              </div>
              <button type="button" className="btn compact" onClick={app.openOnboarding}>
                <SlidersHorizontal />
                Run Onboarding
              </button>
            </div>
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Developer Tools</div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">
                  <Code2 style={{ width: 13, height: 13 }} />
                  Enable Developer Tools
                </span>
                <span className="hint-text">
                  Unlocks local MCP server controls, AgentPort configuration, and the Agent Draft
                  Approval Queue. Off by default for standard staff use.
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
