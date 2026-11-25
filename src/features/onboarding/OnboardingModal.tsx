import { useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  Columns2,
  LayoutList,
  Mail,
  Moon,
  Plus,
  Shield,
  Sun,
  Users,
} from "lucide-react";
import { AddAccountModal } from "../accounts/AddAccountModal";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { FerruleGlyph } from "../ui/FerruleGlyph";

type OnboardingStep = 1 | 2 | 3;

export function OnboardingModal({ app }: { app: MailApp }) {
  const [step, setStep] = useState<OnboardingStep>(1);
  const [addingAnother, setAddingAnother] = useState(false);
  const [serverUrl, setServerUrl] = useState(() => app.staffWorkspace.serverUrl || "");
  const [token, setToken] = useState("");
  const [staffName, setStaffName] = useState(() => app.staffWorkspace.staffName || "");
  const [staffHandle, setStaffHandle] = useState(() => app.staffWorkspace.staffHandle || "");
  const [staffEmail, setStaffEmail] = useState(() => app.staffWorkspace.staffEmail || "");
  const [staffRole, setStaffRole] = useState(() => app.staffWorkspace.staffRole || "");
  const [staffBusy, setStaffBusy] = useState(false);
  const [staffError, setStaffError] = useState<string | null>(null);

  if (!app.onboardingOpen) return null;

  const hasAccounts = app.accounts.length > 0;
  const showAccountForm = !hasAccounts || addingAnother;

  async function connectAndFinish(): Promise<void> {
    if (staffBusy) return;
    if (!serverUrl.trim()) {
      setStaffError("Enter the Staff Sync Server URL.");
      return;
    }
    if (!token.trim()) {
      setStaffError("Enter your Invite / Staff Token.");
      return;
    }
    if (!staffName.trim() || !staffHandle.trim()) {
      setStaffError("Enter your name and @handle.");
      return;
    }
    setStaffBusy(true);
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
      await app.completeOnboarding();
    } catch (err) {
      setStaffError(err instanceof Error ? err.message : String(err));
    } finally {
      setStaffBusy(false);
    }
  }

  return (
    <div className="modal-backdrop onboarding-backdrop">
      <div className="modal onboarding-modal" role="dialog" aria-label="Welcome to Ferrule">
        <div className="onboarding-head">
          <div className="brand-mark">
            <span className="logo-dot">
              <FerruleGlyph />
            </span>
            <span>Ferrule</span>
          </div>
          <div className="onboarding-stepper" aria-label="Setup steps">
            <span className={`onboarding-step-pill${step === 1 ? " active" : step > 1 ? " done" : ""}`}>
              {step > 1 ? <Check size={12} /> : "1"} Appearance
            </span>
            <span className="onboarding-step-sep">→</span>
            <span className={`onboarding-step-pill${step === 2 ? " active" : step > 2 ? " done" : ""}`}>
              {step > 2 ? <Check size={12} /> : "2"} Email Account
            </span>
            <span className="onboarding-step-sep">→</span>
            <span className={`onboarding-step-pill${step === 3 ? " active" : ""}`}>
              3 Pigment Staff
            </span>
          </div>
        </div>

        {step === 1 && (
          <>
            <div className="modal-body onboarding-body">
              <div className="onboarding-hero">
                <h1>Welcome to Ferrule</h1>
                <p>
                  Local-first desktop mail with OS keychain security, isolated Account Spaces, and
                  optional Pigment Staff collaboration. Let&apos;s set up your workspace in under a
                  minute.
                </p>
              </div>

              <div className="onboarding-section">
                <label className="onboarding-label">Theme</label>
                <div className="onboarding-choice-grid">
                  <button
                    type="button"
                    className={`onboarding-choice-card${app.theme === "dark" ? " selected" : ""}`}
                    onClick={() => app.setTheme("dark")}
                  >
                    <Moon size={18} />
                    <div className="onboarding-choice-text">
                      <strong>Dark</strong>
                      <span>Low-glare Mosaic surfaces for long sessions</span>
                    </div>
                  </button>
                  <button
                    type="button"
                    className={`onboarding-choice-card${app.theme === "light" ? " selected" : ""}`}
                    onClick={() => app.setTheme("light")}
                  >
                    <Sun size={18} />
                    <div className="onboarding-choice-text">
                      <strong>Light</strong>
                      <span>Crisp daytime surfaces with high contrast</span>
                    </div>
                  </button>
                </div>
              </div>

              <div className="onboarding-section">
                <label className="onboarding-label">Reading Pane Layout</label>
                <div className="onboarding-choice-grid">
                  <button
                    type="button"
                    className={`onboarding-choice-card${app.layout === "split" ? " selected" : ""}`}
                    onClick={() => app.setLayout("split")}
                  >
                    <Columns2 size={18} />
                    <div className="onboarding-choice-text">
                      <strong>Split view</strong>
                      <span>Thread list and reading pane side by side</span>
                    </div>
                  </button>
                  <button
                    type="button"
                    className={`onboarding-choice-card${app.layout === "classic" ? " selected" : ""}`}
                    onClick={() => app.setLayout("classic")}
                  >
                    <LayoutList size={18} />
                    <div className="onboarding-choice-text">
                      <strong>Classic view</strong>
                      <span>Full-width list that opens into each thread</span>
                    </div>
                  </button>
                </div>
              </div>
            </div>

            <div className="modal-foot">
              <span className="hint-text" style={{ flex: 1 }}>
                You can change theme and layout anytime from the top bar or Settings.
              </span>
              <button type="button" className="btn primary" onClick={() => setStep(2)}>
                Continue
                <ArrowRight size={14} />
              </button>
            </div>
          </>
        )}

        {step === 2 && (
          <>
            <div className="modal-body onboarding-body">
              <div className="onboarding-hero compact">
                <h1>Connect your first email account</h1>
                <p>
                  Add a Pigment Staff IONOS role mailbox, Gmail, Outlook, or any IMAP/SMTP account.
                  Passwords and OAuth tokens are stored strictly in your OS keychain.
                </p>
              </div>

              {hasAccounts && (
                <div className="onboarding-connected-box">
                  <div className="onboarding-connected-head">
                    <span className="onboarding-label" style={{ margin: 0 }}>
                      Connected Account Spaces ({app.accounts.length})
                    </span>
                    {!showAccountForm && (
                      <button
                        type="button"
                        className="btn compact"
                        onClick={() => setAddingAnother(true)}
                      >
                        <Plus size={13} />
                        Add another account
                      </button>
                    )}
                  </div>
                  <div className="space-list-mini">
                    {app.accounts.map((account) => (
                      <div key={account.id} className="space-list-row">
                        <span className={`acct-dot ${spaceClass(account.color)}`} />
                        <div className="space-list-main">
                          <span className="space-list-name">{account.displayName}</span>
                          <span className="space-list-email">{account.email}</span>
                        </div>
                        <span
                          className={`space-kind-badge${account.spaceKind === "pigmentStaff" ? " staff" : ""}`}
                        >
                          {account.spaceKind === "pigmentStaff" ? (
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
                    ))}
                  </div>
                </div>
              )}

              {showAccountForm && (
                <div className="onboarding-embedded-wrap">
                  <AddAccountModal
                    app={app}
                    embedded
                    onAccountConnected={() => setAddingAnother(false)}
                  />
                </div>
              )}
            </div>

            <div className="modal-foot">
              <button type="button" className="btn" onClick={() => setStep(1)}>
                <ArrowLeft size={14} />
                Back
              </button>
              <div style={{ flex: 1 }} />
              {!hasAccounts && (
                <button type="button" className="btn" onClick={() => setStep(3)}>
                  Skip for now
                </button>
              )}
              {hasAccounts && (
                <button type="button" className="btn primary" onClick={() => setStep(3)}>
                  Continue
                  <ArrowRight size={14} />
                </button>
              )}
            </div>
          </>
        )}

        {step === 3 && (
          <>
            <div className="modal-body onboarding-body">
              <div className="onboarding-hero compact">
                <h1>Pigment Staff Sync (Optional)</h1>
                <p>
                  Connect to your Pigment Staff Sync server to share threads, hand off reply drafts,
                  and sync the shared calendar and action items. Using Ferrule for personal mail
                  only? You can skip this step.
                </p>
              </div>

              {app.staffWorkspace.connected ? (
                <div className="staff-ws-card">
                  <div className="staff-ws-head">
                    <Users style={{ width: 15, height: 15, color: "var(--app-shield)" }} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <strong>
                        {app.staffWorkspace.staffName} (@{app.staffWorkspace.staffHandle})
                      </strong>
                      <div className="hint-text">
                        {app.staffWorkspace.staffRole} · {app.staffWorkspace.serverUrl}
                      </div>
                    </div>
                    <span className="space-kind-badge staff">Connected</span>
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
                        placeholder="e.g. Alex Rivera"
                        value={staffName}
                        onChange={(event) => setStaffName(event.target.value)}
                      />
                    </div>
                    <div className="field" style={{ maxWidth: 160 }}>
                      <label>@handle</label>
                      <input
                        className="input"
                        placeholder="e.g. alex"
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
                        placeholder="e.g. Show Director"
                        value={staffRole}
                        onChange={(event) => setStaffRole(event.target.value)}
                      />
                    </div>
                  </div>
                  {staffError && <span className="error-text">{staffError}</span>}
                </div>
              )}
            </div>

            <div className="modal-foot">
              <button
                type="button"
                className="btn"
                disabled={staffBusy}
                onClick={() => setStep(2)}
              >
                <ArrowLeft size={14} />
                Back
              </button>
              <div style={{ flex: 1 }} />
              {app.staffWorkspace.connected ? (
                <button
                  type="button"
                  className="btn primary"
                  onClick={() => void app.completeOnboarding()}
                >
                  <Mail size={14} />
                  Open Workspace
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    className="btn"
                    disabled={staffBusy}
                    onClick={() => void app.completeOnboarding()}
                  >
                    Skip — personal mail only
                  </button>
                  <button
                    type="button"
                    className="btn primary"
                    disabled={staffBusy}
                    onClick={() => void connectAndFinish()}
                  >
                    {staffBusy ? "Connecting…" : "Connect & Finish"}
                  </button>
                </>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
