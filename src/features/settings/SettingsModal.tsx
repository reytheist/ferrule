import { useEffect, useState } from "react";
import {
  Bot,
  Code2,
  KeyRound,
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
import type { AccountId, AgentAccountPermissionView, SignatureId } from "../../types/mail";
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

  // In-Pane AI settings state
  const [aiEnabled, setAiEnabled] = useState(app.aiConfig.enabled);
  const [aiBaseUrl, setAiBaseUrl] = useState(app.aiConfig.baseUrl);
  const [aiModel, setAiModel] = useState(app.aiConfig.model);
  const [aiApiKey, setAiApiKey] = useState("");

  // Hermes MCP Bridge & Push state
  const [mcpEnabled, setMcpEnabled] = useState(app.agentBridgeConfig.mcpEnabled);
  const [ssePort, setSsePort] = useState(String(app.agentBridgeConfig.ssePort));
  const [webhookEnabled, setWebhookEnabled] = useState(app.agentBridgeConfig.webhookEnabled);
  const [webhookUrl, setWebhookUrl] = useState(app.agentBridgeConfig.webhookUrl);
  const [webhookSecret, setWebhookSecret] = useState("");

  useEffect(() => {
    setAiEnabled(app.aiConfig.enabled);
    setAiBaseUrl(app.aiConfig.baseUrl);
    setAiModel(app.aiConfig.model);
  }, [app.aiConfig]);

  useEffect(() => {
    setMcpEnabled(app.agentBridgeConfig.mcpEnabled);
    setSsePort(String(app.agentBridgeConfig.ssePort));
    setWebhookEnabled(app.agentBridgeConfig.webhookEnabled);
    setWebhookUrl(app.agentBridgeConfig.webhookUrl);
  }, [app.agentBridgeConfig]);

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

  async function saveAiSettings(clearKey = false): Promise<void> {
    setBusy(true);
    try {
      await app.saveAiConfig({
        enabled: aiEnabled,
        baseUrl: aiBaseUrl.trim(),
        model: aiModel.trim(),
        apiKey: clearKey ? null : aiApiKey.trim() || null,
        clearApiKey: clearKey,
      });
      setAiApiKey("");
    } catch {
      // Toast already shown by useMailApp
    } finally {
      setBusy(false);
    }
  }

  async function saveBridgeSettings(clearSecret = false): Promise<void> {
    setBusy(true);
    try {
      const parsedPort = Number.parseInt(ssePort, 10);
      await app.saveAgentBridgeConfig({
        mcpEnabled,
        ssePort: Number.isFinite(parsedPort) && parsedPort > 0 && parsedPort <= 65535 ? parsedPort : 48173,
        webhookEnabled,
        webhookUrl: webhookUrl.trim(),
        webhookSecret: clearSecret ? null : webhookSecret.trim() || null,
        clearWebhookSecret: clearSecret,
      });
      setWebhookSecret("");
    } catch {
      // Toast already shown by useMailApp
    } finally {
      setBusy(false);
    }
  }

  async function updatePerm(
    perm: AgentAccountPermissionView,
    patch: Partial<Pick<AgentAccountPermissionView, "enabled" | "allowRead" | "allowTriage" | "allowDrafts" | "blockedFolders">>,
  ): Promise<void> {
    setBusy(true);
    try {
      await app.saveAgentPermission({
        accountId: perm.accountId,
        enabled: patch.enabled ?? perm.enabled,
        allowRead: patch.allowRead ?? perm.allowRead,
        allowTriage: patch.allowTriage ?? perm.allowTriage,
        allowDrafts: patch.allowDrafts ?? perm.allowDrafts,
        blockedFolders: patch.blockedFolders ?? perm.blockedFolders,
      });
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
            <div className="settings-section-title">
              <Sparkles style={{ width: 12, height: 12, marginRight: 5, verticalAlign: "-1px" }} />
              In-Pane AI (OpenAI-Compatible)
            </div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">Enable In-Pane AI</span>
                <span className="hint-text">
                  Thread summaries, selection Q&amp;A, and 1-click reply draft staging. Points at
                  Hermes, Ollama, or any OpenAI-compatible API.
                </span>
              </div>
              <button
                type="button"
                role="switch"
                aria-checked={aiEnabled}
                aria-label="Enable In-Pane AI"
                disabled={busy}
                className={`toggle-switch${aiEnabled ? " on" : ""}`}
                onClick={() => setAiEnabled((v) => !v)}
              >
                <span className="toggle-thumb" />
              </button>
            </div>
            <div className="ai-settings-grid">
              <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                <button
                  type="button"
                  className="btn compact"
                  onClick={() => {
                    setAiBaseUrl("http://127.0.0.1:11434/v1");
                    setAiModel("llama3.2");
                  }}
                >
                  Ollama Local
                </button>
                <button
                  type="button"
                  className="btn compact"
                  onClick={() => {
                    setAiBaseUrl("http://127.0.0.1:8080/v1");
                    setAiModel("hermes");
                  }}
                >
                  Hermes Local
                </button>
                <button
                  type="button"
                  className="btn compact"
                  onClick={() => {
                    setAiBaseUrl("https://api.openai.com/v1");
                    setAiModel("gpt-4o-mini");
                  }}
                >
                  OpenAI
                </button>
              </div>
              <input
                className="input"
                placeholder="Base URL (e.g. http://127.0.0.1:11434/v1)"
                value={aiBaseUrl}
                onChange={(e) => setAiBaseUrl(e.target.value)}
              />
              <input
                className="input"
                placeholder="Model (e.g. llama3.2, gpt-4o-mini)"
                value={aiModel}
                onChange={(e) => setAiModel(e.target.value)}
              />
              <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                <input
                  className="input"
                  type="password"
                  placeholder={
                    app.aiConfig.hasApiKey
                      ? "API key saved in OS keychain (enter to replace)"
                      : "Optional API key (stored strictly in OS keychain)"
                  }
                  value={aiApiKey}
                  onChange={(e) => setAiApiKey(e.target.value)}
                />
                {app.aiConfig.hasApiKey && (
                  <button
                    type="button"
                    className="btn compact"
                    disabled={busy}
                    onClick={() => void saveAiSettings(true)}
                  >
                    Clear Key
                  </button>
                )}
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span className="hint-text" style={{ flex: 1, display: "flex", alignItems: "center", gap: 4 }}>
                  <KeyRound style={{ width: 11, height: 11 }} />
                  {app.aiConfig.hasApiKey ? "API key stored in OS keychain" : "No API key stored"}
                </span>
                <button
                  type="button"
                  className="btn compact primary"
                  disabled={busy}
                  onClick={() => void saveAiSettings(false)}
                >
                  Save AI Settings
                </button>
              </div>
            </div>
          </section>

          <section className="settings-section">
            <div className="settings-section-title">Developer Tools &amp; Hermes MCP Bridge</div>
            <div className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">
                  <Code2 style={{ width: 13, height: 13 }} />
                  Enable Developer Tools
                </span>
                <span className="hint-text">
                  Unlocks local MCP server controls, per-account/folder permissions, SSE/webhook
                  event push, and the Agent Activity &amp; Approval Queue.
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
                  <span>Single-Writer MCP Bridge (ferrule mcp)</span>
                  <button
                    type="button"
                    className="btn compact"
                    onClick={() => {
                      app.closeSettings();
                      app.openAgentModal();
                    }}
                  >
                    <Bot />
                    Activity &amp; Approvals ({app.pendingAgentDrafts.length})
                  </button>
                </div>
                <span className="hint-text">
                  Local IPC: <code>{app.agentBridgeConfig.ipcEndpoint || "auto"}</code> · SSE Push:{" "}
                  <code>
                    http://127.0.0.1:{app.agentBridgeConfig.ssePort}/events
                    {app.agentBridgeConfig.sseToken ? `?token=${app.agentBridgeConfig.sseToken}` : ""}
                  </code>
                </span>
                <span className="hint-text">
                  Point Hermes at <code>ferrule mcp</code> (or <code>ferrule-mcp</code>). Agents
                  can only read plain text (prompt-injection framed), log undoable triage actions,
                  and stage drafts in <code>PendingApproval</code>.
                </span>

                <div className="settings-row">
                  <span className="settings-row-label">Accept MCP Bridge Connections</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={mcpEnabled}
                    aria-label="Accept MCP Bridge Connections"
                    disabled={busy}
                    className={`toggle-switch${mcpEnabled ? " on" : ""}`}
                    onClick={() => setMcpEnabled((v) => !v)}
                  >
                    <span className="toggle-thumb" />
                  </button>
                </div>

                <div className="ai-settings-grid">
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    <label className="hint-text" style={{ minWidth: 90 }}>
                      SSE Port
                    </label>
                    <input
                      className="input"
                      value={ssePort}
                      onChange={(e) => setSsePort(e.target.value)}
                      placeholder="48173"
                    />
                  </div>
                  <div className="settings-row">
                    <span className="settings-row-label">Localhost Webhook Push</span>
                    <button
                      type="button"
                      role="switch"
                      aria-checked={webhookEnabled}
                      aria-label="Localhost Webhook Push"
                      disabled={busy}
                      className={`toggle-switch${webhookEnabled ? " on" : ""}`}
                      onClick={() => setWebhookEnabled((v) => !v)}
                    >
                      <span className="toggle-thumb" />
                    </button>
                  </div>
                  <input
                    className="input"
                    placeholder="Localhost webhook URL (e.g. http://127.0.0.1:8787/hooks/ferrule)"
                    value={webhookUrl}
                    onChange={(e) => setWebhookUrl(e.target.value)}
                  />
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    <input
                      className="input"
                      type="password"
                      placeholder={
                        app.agentBridgeConfig.hasWebhookSecret
                          ? "Webhook secret saved in OS keychain (enter to replace)"
                          : "Optional HMAC webhook secret (OS keychain only)"
                      }
                      value={webhookSecret}
                      onChange={(e) => setWebhookSecret(e.target.value)}
                    />
                    {app.agentBridgeConfig.hasWebhookSecret && (
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busy}
                        onClick={() => void saveBridgeSettings(true)}
                      >
                        Clear Secret
                      </button>
                    )}
                  </div>
                  <div style={{ display: "flex", justifyContent: "flex-end" }}>
                    <button
                      type="button"
                      className="btn compact primary"
                      disabled={busy}
                      onClick={() => void saveBridgeSettings(false)}
                    >
                      Save Bridge Settings
                    </button>
                  </div>
                </div>

                {app.agentPermissions.length > 0 && (
                  <div className="agent-perm-list">
                    <div className="settings-section-title" style={{ marginTop: 6 }}>
                      Per-Account &amp; Per-Folder Agent Permissions
                    </div>
                    {app.agentPermissions.map((perm) => (
                      <div key={perm.accountId} className="agent-perm-card">
                        <div className="agent-perm-head">
                          <strong>{perm.accountLabel}</strong>
                          <span className="hint-text">{perm.accountEmail}</span>
                          <span style={{ flex: 1 }} />
                          <label className="perm-check">
                            <input
                              type="checkbox"
                              checked={perm.enabled}
                              onChange={(e) => void updatePerm(perm, { enabled: e.target.checked })}
                            />
                            Enabled
                          </label>
                          <label className="perm-check">
                            <input
                              type="checkbox"
                              checked={perm.allowRead}
                              disabled={!perm.enabled}
                              onChange={(e) => void updatePerm(perm, { allowRead: e.target.checked })}
                            />
                            Read
                          </label>
                          <label className="perm-check">
                            <input
                              type="checkbox"
                              checked={perm.allowTriage}
                              disabled={!perm.enabled}
                              onChange={(e) =>
                                void updatePerm(perm, { allowTriage: e.target.checked })
                              }
                            />
                            Triage
                          </label>
                          <label className="perm-check">
                            <input
                              type="checkbox"
                              checked={perm.allowDrafts}
                              disabled={!perm.enabled}
                              onChange={(e) =>
                                void updatePerm(perm, { allowDrafts: e.target.checked })
                              }
                            />
                            Drafts
                          </label>
                        </div>
                        <div className="folder-pill-row">
                          <span className="hint-text">Folders:</span>
                          {perm.knownFolders.map((folder) => {
                            const blocked = perm.blockedFolders.some(
                              (b) => b.toLowerCase() === folder.toLowerCase(),
                            );
                            return (
                              <button
                                key={folder}
                                type="button"
                                disabled={!perm.enabled || busy}
                                className={`folder-pill${blocked ? " blocked" : ""}`}
                                title={blocked ? "Blocked from agent (click to allow)" : "Allowed for agent (click to block)"}
                                onClick={() => {
                                  const next = blocked
                                    ? perm.blockedFolders.filter(
                                        (b) => b.toLowerCase() !== folder.toLowerCase(),
                                      )
                                    : [...perm.blockedFolders, folder];
                                  void updatePerm(perm, { blockedFolders: next });
                                }}
                              >
                                {folder} {blocked ? "×" : "✓"}
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
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
