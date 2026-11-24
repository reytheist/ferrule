import { useEffect, useRef, useState } from "react";
import { ArrowLeft, Check, ExternalLink, Plus, X } from "lucide-react";
import type {
  AccountView,
  NewAccountInput,
  ProviderAuth,
  SpaceColor,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { SPACE_COLORS, spaceClass } from "../spaces/SpaceSwitcher";

type ProviderStep =
  | "gmailOAuth"
  | "outlookOAuth"
  | "gmailAppPassword"
  | "imapPassword";

type WizardStep = "provider" | "oauthSetup" | "oauthWaiting" | "customize" | "form";

interface ProviderOption {
  kind: ProviderStep;
  name: string;
  description: string;
}

const GOOGLE_CLIENT_KEY = "galapaghost.googleClientId";
const OUTLOOK_CLIENT_KEY = "galapaghost.outlookClientId";
const OUTLOOK_TENANT_KEY = "galapaghost.outlookTenant";

function readPref(key: string, fallback = ""): string {
  try {
    return window.localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function savePref(key: string, value: string): void {
  try {
    if (value.trim()) window.localStorage.setItem(key, value.trim());
  } catch {
    // Ignore storage failures in restricted webviews.
  }
}

function validPort(raw: string): boolean {
  const value = Number(raw);
  return Number.isInteger(value) && value > 0 && value <= 65535;
}

function pickNextColor(accounts: AccountView[]): SpaceColor {
  let best: SpaceColor = SPACE_COLORS[0] ?? "azure";
  let bestCount = Number.POSITIVE_INFINITY;
  for (const candidate of SPACE_COLORS) {
    const count = accounts.filter((a) => a.color === candidate).length;
    if (count === 0) return candidate;
    if (count < bestCount) {
      best = candidate;
      bestCount = count;
    }
  }
  return best;
}

export function AddAccountModal({ app }: { app: MailApp }) {
  const [step, setStep] = useState<WizardStep>("provider");
  const [provider, setProvider] = useState<ProviderStep>("gmailOAuth");
  const [connected, setConnected] = useState<AccountView | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [color, setColor] = useState<SpaceColor>(() => pickNextColor(app.accounts));
  const [address, setAddress] = useState("");
  const [secret, setSecret] = useState("");
  const [googleClientId, setGoogleClientId] = useState(
    () => readPref(GOOGLE_CLIENT_KEY) || app.savedOAuthClients.gmailClientId || "",
  );
  const [googleClientSecret, setGoogleClientSecret] = useState("");
  const [outlookClientId, setOutlookClientId] = useState(
    () => readPref(OUTLOOK_CLIENT_KEY) || app.savedOAuthClients.outlookClientId || "",
  );
  const [tenant, setTenant] = useState(
    () => readPref(OUTLOOK_TENANT_KEY) || app.savedOAuthClients.outlookTenant || "common",
  );
  const [username, setUsername] = useState("");
  const [imapHost, setImapHost] = useState("");
  const [imapPort, setImapPort] = useState("993");
  const [imapTls, setImapTls] = useState(true);
  const [smtpHost, setSmtpHost] = useState("");
  const [smtpPort, setSmtpPort] = useState("587");
  const [smtpTls, setSmtpTls] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  if (!app.addAccountOpen) return null;

  const effectiveGoogleClientId =
    googleClientId.trim() || app.savedOAuthClients.gmailClientId || "";
  const effectiveOutlookClientId =
    outlookClientId.trim() || app.savedOAuthClients.outlookClientId || "";
  const isOAuth = provider === "gmailOAuth" || provider === "outlookOAuth";
  const oauthProviderName = provider === "outlookOAuth" ? "Microsoft" : "Google";

  const providers: ProviderOption[] = [
    {
      kind: "gmailOAuth",
      name: "Gmail · Sign in with Google",
      description: effectiveGoogleClientId
        ? "Opens your browser to choose a Google account and sets up your space automatically."
        : "One-time Google Desktop Client ID setup, then one-click browser sign-in.",
    },
    {
      kind: "outlookOAuth",
      name: "Outlook · Sign in with Microsoft",
      description: effectiveOutlookClientId
        ? "Opens your browser to choose a Microsoft account and sets up your space automatically."
        : "One-time Microsoft Entra Public Client ID setup, then one-click browser sign-in.",
    },
    {
      kind: "gmailAppPassword",
      name: "Gmail · App Password",
      description: "No cloud console needed — paste a 16-letter Google App Password.",
    },
    {
      kind: "imapPassword",
      name: "Custom IMAP / SMTP",
      description: "Any standards-compliant server: host, port, and password.",
    },
  ];

  async function launchOAuth(
    kind: "gmailOAuth" | "outlookOAuth",
    explicitClientId?: string,
    explicitClientSecret?: string,
  ): Promise<void> {
    const isGoogle = kind === "gmailOAuth";
    const clientId = (
      explicitClientId ?? (isGoogle ? effectiveGoogleClientId : effectiveOutlookClientId)
    ).trim();
    if (!clientId) {
      setProvider(kind);
      setStep("oauthSetup");
      setError(
        isGoogle
          ? "Enter your Google Desktop OAuth Client ID once."
          : "Enter your Microsoft Entra Public Client ID once.",
      );
      return;
    }
    if (isGoogle) {
      savePref(GOOGLE_CLIENT_KEY, clientId);
      setGoogleClientId(clientId);
    } else {
      savePref(OUTLOOK_CLIENT_KEY, clientId);
      savePref(OUTLOOK_TENANT_KEY, tenant.trim() || "common");
      setOutlookClientId(clientId);
    }

    setProvider(kind);
    setConnected(null);
    setError(null);
    setStep("oauthWaiting");
    setBusy(true);
    try {
      const created = await app.oauthSignInAccount({
        provider: isGoogle ? "gmail" : "outlook",
        clientId,
        clientSecret: isGoogle ? (explicitClientSecret ?? googleClientSecret).trim() || null : null,
        tenant: isGoogle ? null : tenant.trim() || "common",
        displayName: null,
        color: null,
      });
      if (!mountedRef.current) return;
      setGoogleClientSecret("");
      setConnected(created);
      setDisplayName(created.displayName);
      setColor(created.color);
      setStep("customize");
    } catch (err) {
      if (!mountedRef.current) return;
      setError(
        err instanceof Error ? err.message : "Could not complete sign-in — try again.",
      );
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }

  function pick(next: ProviderStep): void {
    setProvider(next);
    setError(null);
    if (next === "gmailOAuth") {
      if (effectiveGoogleClientId) {
        void launchOAuth("gmailOAuth", effectiveGoogleClientId);
      } else {
        setStep("oauthSetup");
      }
      return;
    }
    if (next === "outlookOAuth") {
      if (effectiveOutlookClientId) {
        void launchOAuth("outlookOAuth", effectiveOutlookClientId);
      } else {
        setStep("oauthSetup");
      }
      return;
    }
    setColor(pickNextColor(app.accounts));
    setStep("form");
  }

  async function saveSpaceEditsIfChanged(): Promise<boolean> {
    if (!connected) return true;
    const trimmed = displayName.trim();
    if (!trimmed) {
      setError("Enter a name for this space.");
      return false;
    }
    if (trimmed === connected.displayName && color === connected.color) {
      return true;
    }
    setBusy(true);
    setError(null);
    try {
      const updated = await app.updateAccount({
        accountId: connected.id,
        displayName: trimmed,
        color,
      });
      if (mountedRef.current) setConnected(updated);
      return true;
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : "Could not update space.");
      }
      return false;
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }

  async function finishCustomize(): Promise<void> {
    const ok = await saveSpaceEditsIfChanged();
    if (ok && mountedRef.current) app.closeAddAccount();
  }

  async function addAnotherOAuth(): Promise<void> {
    const ok = await saveSpaceEditsIfChanged();
    if (!ok || !mountedRef.current) return;
    if (provider === "gmailOAuth" || provider === "outlookOAuth") {
      void launchOAuth(provider);
    } else {
      setStep("provider");
    }
  }

  function buildPasswordAuth(): ProviderAuth | null {
    const mail = address.trim();
    if (!mail.includes("@")) {
      setError("Enter a valid email address.");
      return null;
    }
    if (provider === "gmailAppPassword") {
      if (secret.replace(/\s+/g, "").length < 8) {
        setError("Enter the 16-character Google App Password.");
        return null;
      }
      return { kind: "gmailAppPassword", address: mail };
    }
    if (imapHost.trim().length === 0 || !validPort(imapPort)) {
      setError("Enter a valid IMAP host and port.");
      return null;
    }
    if (smtpHost.trim().length === 0 || !validPort(smtpPort)) {
      setError("Enter a valid SMTP host and port.");
      return null;
    }
    if (secret.length === 0) {
      setError("Enter the account password.");
      return null;
    }
    return {
      kind: "imapPassword",
      address: mail,
      username: username.trim() || mail,
      imap: { host: imapHost.trim(), port: Number(imapPort), tls: imapTls },
      smtp: { host: smtpHost.trim(), port: Number(smtpPort), tls: smtpTls },
    };
  }

  async function createPasswordAccount(): Promise<void> {
    const auth = buildPasswordAuth();
    if (!auth) return;
    setError(null);
    const input: NewAccountInput = {
      displayName: displayName.trim() || auth.address.split("@")[0] || "New account",
      color,
      auth,
      secret,
    };
    setBusy(true);
    try {
      await app.addAccount(input);
      if (mountedRef.current) app.closeAddAccount();
    } catch (err) {
      if (mountedRef.current) {
        setError(
          err instanceof Error ? err.message : "Could not connect — check details and retry.",
        );
      }
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }

  const canGoBack =
    !busy && (step === "form" || step === "oauthSetup" || (step === "oauthWaiting" && error !== null));

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) {
          if (step === "customize") {
            void finishCustomize();
          } else {
            app.closeAddAccount();
          }
        }
      }}
    >
      <div className="modal" role="dialog" aria-label="Add account">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {canGoBack && (
              <button
                className="icon-btn"
                onClick={() => {
                  setError(null);
                  setStep("provider");
                }}
                title="Back"
              >
                <ArrowLeft />
              </button>
            )}
            <h2>{step === "customize" ? "Customize space" : "Add account"}</h2>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className="wizard-steps">
              <span className={step === "provider" ? "step-on" : ""}>1 · provider</span>
              <span>→</span>
              <span
                className={
                  step === "oauthSetup" || step === "oauthWaiting" || step === "form"
                    ? "step-on"
                    : ""
                }
              >
                2 · sign in
              </span>
              {isOAuth && (
                <>
                  <span>→</span>
                  <span className={step === "customize" ? "step-on" : ""}>3 · space</span>
                </>
              )}
            </span>
            <button
              className="icon-btn"
              disabled={busy}
              onClick={() => {
                if (step === "customize") {
                  void finishCustomize();
                } else {
                  app.closeAddAccount();
                }
              }}
              title="Close (Esc)"
            >
              <X />
            </button>
          </div>
        </div>

        {step === "provider" && (
          <div className="modal-body">
            <span className="hint-text">Pick how this mailbox signs in.</span>
            <div className="provider-grid">
              {providers.map((option) => (
                <button
                  key={option.kind}
                  className={`provider-card${provider === option.kind ? " selected" : ""}`}
                  onClick={() => pick(option.kind)}
                >
                  <span className="pc-name">{option.name}</span>
                  <span className="pc-desc">{option.description}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {step === "oauthSetup" && (
          <div className="modal-body">
            <span className="hint-text">
              {provider === "gmailOAuth"
                ? "Paste your Google Desktop OAuth credentials once. After this, clicking Gmail opens Google directly."
                : "Paste your Microsoft Entra Public Client ID once. After this, clicking Outlook opens Microsoft directly."}
            </span>

            {provider === "gmailOAuth" ? (
              <>
                <div className="field">
                  <label>Google Desktop OAuth Client ID</label>
                  <input
                    className="input"
                    autoFocus
                    placeholder="123456789-xxxx.apps.googleusercontent.com"
                    value={googleClientId}
                    onChange={(event) => setGoogleClientId(event.target.value)}
                  />
                </div>
                <div className="field">
                  <label>Google Client Secret (stored in OS keychain, reused for all Gmail accounts)</label>
                  <input
                    className="input"
                    type="password"
                    placeholder="GOCSPX-… (or leave blank to reuse saved secret)"
                    value={googleClientSecret}
                    onChange={(event) => setGoogleClientSecret(event.target.value)}
                  />
                </div>
              </>
            ) : (
              <>
                <div className="field">
                  <label>Microsoft Entra Public Client ID</label>
                  <input
                    className="input"
                    autoFocus
                    placeholder="00000000-0000-0000-0000-000000000000"
                    value={outlookClientId}
                    onChange={(event) => setOutlookClientId(event.target.value)}
                  />
                </div>
                <div className="field">
                  <label>Tenant</label>
                  <input
                    className="input"
                    placeholder="common"
                    value={tenant}
                    onChange={(event) => setTenant(event.target.value)}
                  />
                </div>
              </>
            )}

            {error && <span className="error-text">{error}</span>}
          </div>
        )}

        {step === "oauthWaiting" && (
          <div className="modal-body">
            <div className="oauth-status-card">
              <div className="oauth-status-title">
                <ExternalLink style={{ width: 15, height: 15 }} />
                <span>
                  {error
                    ? `${oauthProviderName} sign-in did not finish`
                    : `Select your ${oauthProviderName} account in your browser`}
                </span>
              </div>
              <span className="hint-text">
                {error
                  ? "Fix the issue below or try choosing a different account."
                  : "Approve access in your browser. Galapaghost will automatically fill in your space name and assign a unique color."}
              </span>
              {error && <span className="error-text">{error}</span>}
              <div style={{ display: "flex", gap: 8, marginTop: 4 }}>
                {error && (
                  <button
                    type="button"
                    className="btn primary"
                    disabled={busy}
                    onClick={() => {
                      if (provider === "gmailOAuth" || provider === "outlookOAuth") {
                        void launchOAuth(provider);
                      }
                    }}
                  >
                    Try again
                  </button>
                )}
                <button
                  type="button"
                  className="btn"
                  disabled={busy}
                  onClick={() => {
                    setError(null);
                    setStep("oauthSetup");
                  }}
                >
                  Change OAuth client
                </button>
              </div>
            </div>
          </div>
        )}

        {step === "customize" && connected && (
          <div className="modal-body">
            <div className={`connected-pill ${spaceClass(color)}`}>
              <span className="acct-dot" />
              <span className="cp-email">{connected.email}</span>
              <span className="cp-state">
                <Check style={{ width: 12, height: 12 }} />
                Connected · syncing
              </span>
            </div>

            <div className="field">
              <label>Space name</label>
              <input
                className="input"
                autoFocus
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !busy) {
                    event.preventDefault();
                    void finishCustomize();
                  }
                }}
              />
              <span className="hint-text">
                Pre-filled from your account. Edit anytime from the top space bar.
              </span>
            </div>

            <div className="field">
              <label>Space color</label>
              <div className="color-picker">
                {SPACE_COLORS.map((entry) => (
                  <button
                    key={entry}
                    type="button"
                    className={`color-swatch ${spaceClass(entry)}${color === entry ? " selected" : ""}`}
                    title={entry}
                    onClick={() => setColor(entry)}
                  />
                ))}
              </div>
            </div>

            {error && <span className="error-text">{error}</span>}
          </div>
        )}

        {step === "form" && (
          <div className="modal-body">
            <div className="field-row">
              <div className="field">
                <label>Space name</label>
                <input
                  className="input"
                  placeholder="e.g. Domain Mail"
                  value={displayName}
                  onChange={(event) => setDisplayName(event.target.value)}
                />
              </div>
              <div className="field">
                <label>Email address</label>
                <input
                  className="input"
                  placeholder="you@example.com"
                  value={address}
                  onChange={(event) => setAddress(event.target.value)}
                />
              </div>
            </div>

            <div className="field">
              <label>Space color</label>
              <div className="color-picker">
                {SPACE_COLORS.map((entry) => (
                  <button
                    key={entry}
                    type="button"
                    className={`color-swatch ${spaceClass(entry)}${color === entry ? " selected" : ""}`}
                    title={entry}
                    onClick={() => setColor(entry)}
                  />
                ))}
              </div>
            </div>

            {provider === "gmailAppPassword" && (
              <div className="field">
                <label>Google App Password</label>
                <input
                  className="input"
                  type="password"
                  placeholder="abcd efgh ijkl mnop"
                  value={secret}
                  onChange={(event) => setSecret(event.target.value)}
                />
                <span className="hint-text">
                  Generate at myaccount.google.com/apppasswords (requires 2-Step Verification).
                  Spaces are stripped automatically.
                </span>
              </div>
            )}

            {provider === "imapPassword" && (
              <>
                <div className="field-row">
                  <div className="field">
                    <label>IMAP host</label>
                    <input
                      className="input"
                      placeholder="imap.example.com"
                      value={imapHost}
                      onChange={(event) => setImapHost(event.target.value)}
                    />
                  </div>
                  <div className="field" style={{ maxWidth: 96 }}>
                    <label>Port</label>
                    <input
                      className="input"
                      inputMode="numeric"
                      value={imapPort}
                      onChange={(event) => setImapPort(event.target.value)}
                    />
                  </div>
                  <div className="field" style={{ maxWidth: 74 }}>
                    <label>TLS</label>
                    <select
                      className="select"
                      value={imapTls ? "on" : "off"}
                      onChange={(event) => setImapTls(event.target.value === "on")}
                    >
                      <option value="on">TLS</option>
                      <option value="off">Plain</option>
                    </select>
                  </div>
                </div>
                <div className="field-row">
                  <div className="field">
                    <label>SMTP host</label>
                    <input
                      className="input"
                      placeholder="smtp.example.com"
                      value={smtpHost}
                      onChange={(event) => setSmtpHost(event.target.value)}
                    />
                  </div>
                  <div className="field" style={{ maxWidth: 96 }}>
                    <label>Port</label>
                    <input
                      className="input"
                      inputMode="numeric"
                      value={smtpPort}
                      onChange={(event) => setSmtpPort(event.target.value)}
                    />
                  </div>
                  <div className="field" style={{ maxWidth: 74 }}>
                    <label>TLS</label>
                    <select
                      className="select"
                      value={smtpTls ? "on" : "off"}
                      onChange={(event) => setSmtpTls(event.target.value === "on")}
                    >
                      <option value="on">STARTTLS</option>
                      <option value="off">Plain</option>
                    </select>
                  </div>
                </div>
                <div className="field-row">
                  <div className="field">
                    <label>Username</label>
                    <input
                      className="input"
                      placeholder="defaults to email address"
                      value={username}
                      onChange={(event) => setUsername(event.target.value)}
                    />
                  </div>
                  <div className="field">
                    <label>Password</label>
                    <input
                      className="input"
                      type="password"
                      placeholder="Account password"
                      value={secret}
                      onChange={(event) => setSecret(event.target.value)}
                    />
                  </div>
                </div>
              </>
            )}

            {error && <span className="error-text">{error}</span>}
          </div>
        )}

        <div className="modal-foot">
          <span className="hint-text" style={{ flex: 1 }}>
            Refresh tokens and passwords stay in your OS keychain only.
          </span>
          {step === "customize" ? (
            <>
              <button
                className="btn"
                disabled={busy}
                onClick={() => void addAnotherOAuth()}
              >
                <Plus style={{ width: 13, height: 13 }} />
                Add another {provider === "outlookOAuth" ? "Outlook" : "Gmail"}
              </button>
              <button
                className="btn primary"
                disabled={busy}
                onClick={() => void finishCustomize()}
              >
                {busy ? "Saving…" : "Done"}
              </button>
            </>
          ) : (
            <>
              <button className="btn" disabled={busy} onClick={app.closeAddAccount}>
                Cancel
              </button>
              {step === "oauthSetup" && (
                <button
                  className="btn primary"
                  disabled={busy}
                  onClick={() => {
                    if (provider === "gmailOAuth") {
                      void launchOAuth("gmailOAuth", googleClientId, googleClientSecret);
                    } else if (provider === "outlookOAuth") {
                      void launchOAuth("outlookOAuth", outlookClientId);
                    }
                  }}
                >
                  <ExternalLink style={{ width: 13, height: 13 }} />
                  Continue to {oauthProviderName}
                </button>
              )}
              {step === "form" && (
                <button
                  className="btn primary"
                  disabled={busy}
                  onClick={() => void createPasswordAccount()}
                >
                  {busy ? "Connecting…" : "Add & sync account"}
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
