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
  Sun,
} from "lucide-react";
import { AddAccountModal } from "../accounts/AddAccountModal";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { FerruleGlyph } from "../ui/FerruleGlyph";

type OnboardingStep = 1 | 2;

export function OnboardingModal({ app }: { app: MailApp }) {
  const [step, setStep] = useState<OnboardingStep>(1);
  const [addingAnother, setAddingAnother] = useState(false);

  if (!app.onboardingOpen) return null;

  const hasAccounts = app.accounts.length > 0;
  const showAccountForm = !hasAccounts || addingAnother;

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
            <span className={`onboarding-step-pill${step === 2 ? " active" : ""}`}>
              2 Email Account
            </span>
          </div>
        </div>

        {step === 1 && (
          <>
            <div className="modal-body onboarding-body">
              <div className="onboarding-hero">
                <h1>Welcome to Ferrule</h1>
                <p>
                  Local-first personal desktop mail with OS keychain security, color-coded Account
                  Spaces, and built-in calendar &amp; scheduling. Let&apos;s set up your workspace.
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
                  Add Gmail, Outlook, IONOS, iCloud, Fastmail, or any IMAP/SMTP account. Passwords
                  and OAuth tokens are stored strictly in your OS keychain.
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
              {!hasAccounts ? (
                <button
                  type="button"
                  className="btn"
                  onClick={() => void app.completeOnboarding()}
                >
                  Skip for now
                </button>
              ) : (
                <button
                  type="button"
                  className="btn primary"
                  onClick={() => void app.completeOnboarding()}
                >
                  <Mail size={14} />
                  Open Workspace
                </button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
