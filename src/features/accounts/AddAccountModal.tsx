import { useState } from "react";
import { ArrowLeft, X } from "lucide-react";
import type { NewAccountInput, ProviderAuth, SpaceColor } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";

type ProviderStep =
  | "gmailAppPassword"
  | "gmailOAuth"
  | "outlookOAuth"
  | "imapPassword";

interface ProviderOption {
  kind: ProviderStep;
  name: string;
  description: string;
}

const PROVIDERS: ProviderOption[] = [
  {
    kind: "gmailAppPassword",
    name: "Gmail · App Password",
    description: "Sign in with a Google App Password (2-step verification on).",
  },
  {
    kind: "gmailOAuth",
    name: "Gmail · Your OAuth client",
    description: "Bring your own Google OAuth client ID.",
  },
  {
    kind: "outlookOAuth",
    name: "Outlook · OAuth",
    description: "Microsoft Entra public-client sign-in, no client secret.",
  },
  {
    kind: "imapPassword",
    name: "Custom IMAP / SMTP",
    description: "Any standards-compliant server: host, port, credentials.",
  },
];

const COLORS: SpaceColor[] = ["azure", "berry", "teal", "amber", "violet", "emerald"];

function validPort(raw: string): boolean {
  const value = Number(raw);
  return Number.isInteger(value) && value > 0 && value <= 65535;
}

export function AddAccountModal({ app }: { app: MailApp }) {
  const [step, setStep] = useState<"provider" | "form">("provider");
  const [provider, setProvider] = useState<ProviderStep>("gmailAppPassword");
  const [displayName, setDisplayName] = useState("");
  const [color, setColor] = useState<SpaceColor>("violet");
  const [address, setAddress] = useState("");
  const [secret, setSecret] = useState("");
  const [clientId, setClientId] = useState("");
  const [tenant, setTenant] = useState("common");
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
        if (secret.trim().length < 8) {
          setError("Enter the 16-character Google App Password.");
          return null;
        }
        return { kind: "gmailAppPassword", address: mail };
      case "gmailOAuth":
        if (clientId.trim().length === 0) {
          setError("Enter your Google OAuth client ID.");
          return null;
        }
        return {
          kind: "gmailOAuth",
          address: mail,
          client: { clientId: clientId.trim(), hasClientSecret: false },
        };
      case "outlookOAuth":
        if (clientId.trim().length === 0) {
          setError("Enter your Microsoft Entra client ID.");
          return null;
        }
        return {
          kind: "outlookOAuth",
          address: mail,
          clientId: clientId.trim(),
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
    const input: NewAccountInput = {
      displayName: displayName.trim() || auth.address.split("@")[0] || "New account",
      color,
      auth,
      secret: auth.kind === "gmailOAuth" || auth.kind === "outlookOAuth" ? null : secret,
    };
    setBusy(true);
    try {
      await app.addAccount(input);
      app.closeAddAccount();
    } catch {
      setError("Could not add the account — check the details and try again.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeAddAccount();
      }}
    >
      <div className="modal" role="dialog" aria-label="Add account">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {step === "form" && (
              <button className="icon-btn" onClick={() => setStep("provider")} title="Back">
                <ArrowLeft />
              </button>
            )}
            <h2>Add account</h2>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className="wizard-steps">
              <span className={step === "provider" ? "step-on" : ""}>1 · provider</span>
              <span>→</span>
              <span className={step === "form" ? "step-on" : ""}>2 · details</span>
            </span>
            <button className="icon-btn" onClick={app.closeAddAccount} title="Close (Esc)">
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
                <label>Display name</label>
                <input
                  className="input"
                  placeholder="e.g. Studio Gmail"
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

            {provider === "gmailAppPassword" && (
              <div className="field">
                <label>Google App Password</label>
                <input
                  className="input"
                  type="password"
                  placeholder="16-character app password"
                  value={secret}
                  onChange={(event) => setSecret(event.target.value)}
                />
                <span className="hint-text">
                  Google Account → Security → 2-Step Verification → App passwords.
                </span>
              </div>
            )}

            {provider === "gmailOAuth" && (
              <div className="field">
                <label>OAuth client ID</label>
                <input
                  className="input"
                  placeholder="….apps.googleusercontent.com"
                  value={clientId}
                  onChange={(event) => setClientId(event.target.value)}
                />
                <span className="hint-text">
                  Stored locally; the client secret never leaves your machine.
                </span>
              </div>
            )}

            {provider === "outlookOAuth" && (
              <>
                <div className="field">
                  <label>Entra client ID</label>
                  <input
                    className="input"
                    placeholder="00000000-0000-0000-0000-000000000000"
                    value={clientId}
                    onChange={(event) => setClientId(event.target.value)}
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
            Secrets go to the OS keychain — never to the WebView.
          </span>
          <button className="btn" onClick={app.closeAddAccount}>
            Cancel
          </button>
          {step === "form" && (
            <button className="btn primary" disabled={busy} onClick={() => void create()}>
              {busy ? "Adding…" : "Create account"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
