import { Plus, SlidersHorizontal, Sparkles, Users } from "lucide-react";
import type { MailApp } from "../shell/useMailApp";
import { FerruleGlyph } from "../ui/FerruleGlyph";

export function EmptyWorkspace({ app }: { app: MailApp }) {
  return (
    <section className="empty-workspace" aria-label="Empty workspace">
      <div className="empty-workspace-card">
        <div className="empty-workspace-emblem">
          <FerruleGlyph />
        </div>
        <h2>No email accounts connected yet</h2>
        <p>
          Connect a Pigment Staff IONOS mailbox, Gmail, Outlook, or custom IMAP/SMTP account to
          start syncing locally. All passwords and tokens stay strictly in your OS keychain.
        </p>
        <div className="empty-workspace-actions">
          <button type="button" className="btn primary" onClick={app.openAddAccount}>
            <Plus size={14} />
            Add Account
          </button>
          {!app.staffWorkspace.connected && (
            <button type="button" className="btn" onClick={app.openSettings}>
              <Users size={14} />
              Connect Pigment Staff Sync
            </button>
          )}
          <button type="button" className="btn" onClick={app.openOnboarding}>
            <SlidersHorizontal size={14} />
            Re-open Onboarding
          </button>
        </div>
        <button
          type="button"
          className="empty-workspace-tour-link"
          onClick={app.startTour}
        >
          <Sparkles size={13} />
          Take the 60-second Feature Tour
        </button>
      </div>
    </section>
  );
}
