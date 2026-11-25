import { useState } from "react";
import { Code2, Moon, Shield, SlidersHorizontal, Sun, Terminal, Users, X } from "lucide-react";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";

export function SettingsModal({ app }: { app: MailApp }) {
  const [busy, setBusy] = useState(false);

  if (!app.settingsOpen) return null;

  const devTools = app.settings.developerToolsEnabled;

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
      <div className="modal" role="dialog" aria-label="Settings">
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
