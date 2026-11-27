import { useEffect, useState } from "react";
import { Bot, Moon, PenLine, Plus, RefreshCw, Search, Settings, Sun } from "lucide-react";
import type { MailApp } from "../shell/useMailApp";
import { FerruleGlyph } from "../ui/FerruleGlyph";

const IS_MAC = typeof navigator !== "undefined" && /mac/i.test(navigator.platform);

const SPLIT_ICON = (
  <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
    <rect x="2" y="3" width="12" height="10" rx="1.5" />
    <path d="M7 3v10" />
  </svg>
);

const CLASSIC_ICON = (
  <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
    <rect x="2" y="3" width="12" height="10" rx="1.5" />
    <path d="M4.5 6.2h7M4.5 9.8h7" />
  </svg>
);

function syncLabel(app: MailApp): string {
  if (app.syncing) return "Syncing…";
  const lastOk = app.accounts.reduce<number | null>(
    (latest, a) => (a.sync.kind === "idle" && a.sync.lastOk !== null ? Math.max(latest ?? 0, a.sync.lastOk) : latest),
    null,
  );
  const count = app.accounts.length;
  if (count === 0) return "No accounts";
  if (lastOk === null) return `${count} ${count === 1 ? "acct" : "accts"} · never`;
  const minutes = Math.max(0, Math.round((Date.now() - lastOk) / 60000));
  if (minutes < 1) return `${count} ${count === 1 ? "acct" : "accts"} · just now`;
  if (minutes < 60) return `${count} ${count === 1 ? "acct" : "accts"} · ${minutes}m ago`;
  return `${count} ${count === 1 ? "acct" : "accts"} · ${Math.round(minutes / 60)}h ago`;
}

export function Topbar({ app }: { app: MailApp }) {
  const [, setClockTick] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => setClockTick((n) => n + 1), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <header className="topbar">
      <div className="brand-mark">
        <span className="logo-dot">
          <FerruleGlyph />
        </span>
        <span>Ferrule</span>
      </div>

      <button
        className="search-trigger"
        data-tour="command-bar"
        onClick={app.openCommand}
        title="Search mail, labels, or commands"
      >
        <span className="s-left">
          <Search />
          Search mail, labels, or commands…
        </span>
        <span className="kbd">{IS_MAC ? "⌘ K" : "Ctrl K"}</span>
      </button>

      <div className="topbar-right">
        <span className={`sync-pill${app.syncing ? " busy" : ""}`} title="Automatic sync every 2 min & on window focus">
          <i />
          {syncLabel(app)}
        </span>
        <button className="ghost-btn" onClick={() => void app.syncNow()} title="Sync all accounts now">
          <RefreshCw />
          Sync Now
        </button>
        {(app.settings.developerToolsEnabled ||
          app.pendingAgentDrafts.length > 0 ||
          app.agentActivity.length > 0) && (
          <button
            className="ghost-btn"
            onClick={app.openAgentModal}
            title="Hermes Agent Activity & Pending Approval Drafts"
          >
            <Bot />
            Agent
            {app.pendingAgentDrafts.length > 0 && (
              <span className="kbd">{app.pendingAgentDrafts.length}</span>
            )}
          </button>
        )}
        <button
          className="icon-btn"
          onClick={app.toggleTheme}
          title={app.theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
        >
          {app.theme === "dark" ? <Sun /> : <Moon />}
        </button>
        <button
          className="icon-btn"
          onClick={app.toggleLayout}
          title={app.layout === "split" ? "Switch to classic layout" : "Switch to split layout"}
        >
          {app.layout === "split" ? SPLIT_ICON : CLASSIC_ICON}
        </button>
        <button
          className="icon-btn"
          data-tour="settings"
          onClick={app.openSettings}
          title={IS_MAC ? "Settings (⌘,)" : "Settings (Ctrl+,)"}
          aria-label="Settings"
        >
          <Settings />
        </button>
        <button className="ghost-btn" onClick={app.openAddAccount} title="Add account">
          <Plus />
          Add Account
        </button>
        <button className="compose-btn" onClick={app.openCompose}>
          <PenLine />
          Compose
        </button>
      </div>
    </header>
  );
}
