import { useState } from "react";
import { ArrowLeft, ExternalLink, X } from "lucide-react";
import type { NewAccountInput, ProviderAuth, SpaceColor } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";

type ProviderStep =
  | "gmailOAuth"
  | "outlookOAuth"
  | "gmailAppPassword"
  | "imapPassword";

interface ProviderOption {
  kind: ProviderStep;
  name: string;
  description: string;
}

const PROVIDERS: ProviderOption[] = [
  {
    kind: "gmailOAuth",
    name: "Gmail · Sign in with Google",
    description: "One-click browser OAuth sign-in using your free Google Desktop Client ID.",
  },
  {
    kind: "outlookOAuth",
    name: "Outlook · Sign in with Microsoft",
    description: "One-click browser OAuth sign-in using a free Microsoft Entra Public Client.",
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

const COLORS: SpaceColor[] = ["azure", "berry", "teal", "amber", "violet", "emerald"];
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

export function AddAccountModal({ app }: { app: MailApp }) {
  const [step, setStep] = useState<"provider" | "form">("provider");
  const [provider, setProvider] = useState<ProviderStep>("gmailOAuth");
  const [displayName, setDisplayName] = useState("");
  const [color, setColor] = useState<SpaceColor>("azure");
  const [address, setAddress] = useState("");
  const [secret, setSecret] = useState("");
  const [googleClientId, setGoogleClientId] = useState(() => readPref(GOOGLE_CLIENT_KEY));
  const [googleClientSecret, setGoogleClientSecret] = useState("");
  const [showGoogleCreds, setShowGoogleCreds] = useState(() => readPref(GOOGLE_CLIENT_KEY).length === 0);
  const [outlookClientId, setOutlookClientId] = useState(() => readPref(OUTLOOK_CLIENT_KEY));
  const [showOutlookCreds, setShowOutlookCreds] = useState(() => readPref(OUTLOOK_CLIENT_KEY).length === 0);
  const [tenant, setTenant] = useState(() => readPref(OUTLOOK_TENANT_KEY, "common"));
  const [username, setUsername] = useState("");
  const [imapHost, setImapHost] = useState("");
  const [imapPort, setImapPort] = useState("993");
  const [imapTls, setImapTls] = useState(true);
  const [smtpHost, setSmtpHost] = useState("");
  const [smtpPort, setSmtpPort] = useState("587");
  const [smtpTls, setSmtpTls] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!app.addAccountOpen) return null;

  const isOAuth = provider === "gmailOAuth" || provider === "outlookOAuth";

  function pick(next: ProviderStep): void {
    setProvider(next);
    setStep("form");
    setError(null);
  }

  function buildAuth(): ProviderAuth | null {
    const mail = address.trim();
    if (!mail.includes("@")) {
      setError("Enter a valid email address.");
      return null;
    }
    switch (provider) {
      case "gmailAppPassword":
        if (secret.replace(/\s+/g, "").length < 8) {
          setError("Enter the 16-character Google App Password.");
          return null;
        }
        return { kind: "gmailAppPassword", address: mail };
      case "gmailOAuth":
        if (googleClientId.trim().length === 0) {
          setError("Enter your Google OAuth Desktop Client ID.");
          return null;
        }
        savePref(GOOGLE_CLIENT_KEY, googleClientId);
        return {
          kind: "gmailOAuth",
          address: mail,
          client: {
            clientId: googleClientId.trim(),
            hasClientSecret: googleClientSecret.trim().length > 0,
          },
        };
      case "outlookOAuth":
        if (outlookClientId.trim().length === 0) {
          setError("Enter your Microsoft Entra Public Client ID.");
          return null;
        }
        savePref(OUTLOOK_CLIENT_KEY, outlookClientId);
        savePref(OUTLOOK_TENANT_KEY, tenant.trim() || "common");
        return {
          kind: "outlookOAuth",
          address: mail,
          clientId: outlookClientId.trim(),
          tenant: tenant.trim() || "common",
        };
      case "imapPassword":
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
      default: {
        const exhaustive: never = provider;
        throw new Error(`Unexpected provider: ${JSON.stringify(exhaustive)}`);
      }
    }
  }

  async function create(): Promise<void> {
    const auth = buildAuth();
    if (!auth) return;
    setError(null);
    const input: NewAccountInput = {
      displayName: displayName.trim() || auth.address.split("@")[0] || "New account",
      color,
      auth,
      secret:
        auth.kind === "gmailOAuth"
          ? googleClientSecret.trim() || null
          : auth.kind === "outlookOAuth"
            ? null
            : secret,
    };
    setBusy(true);
    try {
      if (isOAuth) {
        await app.oauthSignInAccount(input);
      } else {
        await app.addAccount(input);
      }
      app.closeAddAccount();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not connect — check details and retry.");
    } finally {
      setBusy(false);
    }
  }

  function submitLabel(): string {
    if (busy) {
      return isOAuth ? "Waiting for browser sign-in…" : "Connecting…";
    }
    if (provider === "gmailOAuth") return "Sign in with Google";
    if (provider === "outlookOAuth") return "Sign in with Microsoft";
    return "Add & sync account";
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) app.closeAddAccount();
      }}
    >
      <div className="modal" role="dialog" aria-label="Add account">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {step === "form" && (
              <button
                className="icon-btn"
                disabled={busy}
                onClick={() => setStep("provider")}
                title="Back"
              >
                <ArrowLeft />
              </button>
            )}
            <h2>Add account</h2>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className="wizard-steps">
              <span className={step === "provider" ? "step-on" : ""}>1 · provider</span>
              <span>→</span>
              <span className={step === "form" ? "step-on" : ""}>2 · sign in</span>
            </span>
            <button
              className="icon-btn"
              disabled={busy}
              onClick={app.closeAddAccount}
              title="Close (Esc)"
            >
              <X />
            </button>
          </div>
        </div>

        {step === "provider" ? (
          <div className="modal-body">
            <span className="hint-text">Pick how this mailbox signs in.</span>
            <div className="provider-grid">
              {PROVIDERS.map((option) => (
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
        ) : (
          <div className="modal-body">
            <div className="field-row">
              <div className="field">
                <label>Space name</label>
                <input
                  className="input"
                  placeholder="e.g. Personal Gmail"
                  value={displayName}
                  onChange={(event) => setDisplayName(event.target.value)}
                />
              </div>
              <div className="field">
                <label>Email address</label>
                <input
                  className="input"
                  placeholder="you@gmail.com"
                  value={address}
                  onChange={(event) => setAddress(event.target.value)}
                />
              </div>
            </div>

            <div className="field">
              <label>Space color</label>
              <div className="color-picker">
                {COLORS.map((entry) => (
                  <button
                    key={entry}
                    className={`color-swatch ${spaceClass(entry)}${color === entry ? " selected" : ""}`}
                    title={entry}
                    onClick={() => setColor(entry)}
                  />
                ))}
              </div>
            </div>

            {provider === "gmailOAuth" && (
              <>
                {!showGoogleCreds && googleClientId.trim().length > 0 ? (
                  <div className="field">
                    <span className="hint-text">
                      Using saved Google OAuth client ({googleClientId.slice(0, 18)}…).{" "}
                      <button
                        type="button"
                        className="t-btn"
                        style={{ marginLeft: 6 }}
                        onClick={() => setShowGoogleCreds(true)}
                      >
                        Change
                      </button>
                    </span>
                  </div>
                ) : (
                  <>
                    <div className="field">
                      <label>Google Desktop OAuth Client ID (saved once for all Gmail accounts)</label>
                      <input
                        className="input"
                        placeholder="123456789-xxxx.apps.googleusercontent.com"
                        value={googleClientId}
                        onChange={(event) => setGoogleClientId(event.target.value)}
                      />
                    </div>
                    <div className="field">
                      <label>Google Client Secret (stored in OS keychain, reused across Gmail accounts)</label>
                      <input
                        className="input"
                        type="password"
                        placeholder="Leave blank to reuse saved secret, or paste GOCSPX-…"
                        value={googleClientSecret}
                        onChange={(event) => setGoogleClientSecret(event.target.value)}
                      />
                    </div>
                  </>
                )}
              </>
            )}

            {provider === "outlookOAuth" && (
              <>
                {!showOutlookCreds && outlookClientId.trim().length > 0 ? (
                  <div className="field">
                    <span className="hint-text">
                      Using saved Microsoft Entra client ({outlookClientId.slice(0, 14)}…).{" "}
                      <button
                        type="button"
                        className="t-btn"
                        style={{ marginLeft: 6 }}
                        onClick={() => setShowOutlookCreds(true)}
                      >
                        Change
                      </button>
                    </span>
                  </div>
                ) : (
                  <>
                    <div className="field">
                      <label>Microsoft Entra Public Client ID</label>
                      <input
                        className="input"
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
              </>
            )}

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
          <button className="btn" disabled={busy} onClick={app.closeAddAccount}>
            Cancel
          </button>
          {step === "form" && (
            <button className="btn primary" disabled={busy} onClick={() => void create()}>
              {isOAuth && <ExternalLink style={{ width: 13, height: 13 }} />}
              {submitLabel()}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
