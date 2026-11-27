import {
  Archive,
  Bookmark,
  CalendarClock,
  CalendarDays,
  FileText,
  Filter,
  Flag,
  Inbox,
  Send,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Star,
  Tag,
  Trash2,
  Clock,
} from "lucide-react";
import type { ComponentType } from "react";
import type { MailboxView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

interface NavEntry {
  key: string;
  label: string;
  icon: ComponentType<{ size?: number }>;
  view: MailboxView;
  count: (app: MailApp) => number | null;
}

const ENTRIES: NavEntry[] = [
  {
    key: "inbox",
    label: "Unified Inbox",
    icon: Inbox,
    view: { kind: "inbox", category: "all" },
    count: (app) => app.counts?.inbox ?? null,
  },
  {
    key: "snoozed",
    label: "Snoozed",
    icon: Clock,
    view: { kind: "snoozed" },
    count: (app) => app.counts?.snoozed ?? null,
  },
  {
    key: "followUps",
    label: "Follow-ups",
    icon: Flag,
    view: { kind: "followUps" },
    count: (app) => app.counts?.followUps ?? null,
  },
  {
    key: "scheduled",
    label: "Scheduled",
    icon: CalendarClock,
    view: { kind: "scheduled" },
    count: (app) => app.counts?.scheduled ?? null,
  },
  {
    key: "starred",
    label: "Starred",
    icon: Star,
    view: { kind: "starred" },
    count: () => null,
  },
  {
    key: "sent",
    label: "Sent",
    icon: Send,
    view: { kind: "sent" },
    count: () => null,
  },
  {
    key: "archive",
    label: "Archive",
    icon: Archive,
    view: { kind: "archive" },
    count: () => null,
  },
  {
    key: "spam",
    label: "Spam",
    icon: ShieldAlert,
    view: { kind: "spam" },
    count: () => null,
  },
  {
    key: "trash",
    label: "Trash",
    icon: Trash2,
    view: { kind: "trash" },
    count: () => null,
  },
];

function sameView(a: MailboxView, b: MailboxView): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "inbox" && b.kind === "inbox") return a.category === b.category;
  if (a.kind === "search" && b.kind === "search") return a.text === b.text;
  if (a.kind === "label" && b.kind === "label") return a.labelId === b.labelId;
  return true;
}

export function MailboxSidebar({ app }: { app: MailApp }) {
  const renderEntry = (entry: NavEntry) => {
    const Icon = entry.icon;
    const count = entry.count(app);
    const active = app.activeSection === "mail" && sameView(app.view, entry.view);
    return (
      <button
        key={entry.key}
        className={`nav-item${active ? " active" : ""}`}
        data-tour={entry.key === "inbox" ? "inbox" : undefined}
        onClick={() => app.selectView(entry.view)}
        title={entry.label}
      >
        <Icon />
        <span className="nav-label">{entry.label}</span>
        {count !== null && <span className="nav-count">{count}</span>}
      </button>
    );
  };

  const inboxEntry = ENTRIES[0];
  const scheduleEntries = ENTRIES.slice(1, 4);
  const restEntries = ENTRIES.slice(4);

  return (
    <aside className="sidebar">
      <div className="sidebar-section">Mailboxes</div>
      {inboxEntry && renderEntry(inboxEntry)}
      <div className="sidebar-tour-group" data-tour="schedule">
        {scheduleEntries.map(renderEntry)}
      </div>
      {restEntries.map(renderEntry)}

      <div className="sidebar-section">Workspace</div>
      <button
        className={`nav-item${app.activeSection === "calendar" ? " active" : ""}`}
        onClick={app.openCalendarSection}
        title="Calendar"
      >
        <CalendarDays />
        <span className="nav-label">Calendar</span>
        <span className="nav-count">{app.calendarOccurrences.length}</span>
      </button>

      <div className="sidebar-section">Library</div>
      <button className="nav-item" onClick={app.openScreener} title="Sender Screener & VIPs">
        <ShieldCheck />
        <span className="nav-label">Screener &amp; VIP</span>
        {app.screenerSenders.length > 0 && (
          <span className="nav-count">{app.screenerSenders.length}</span>
        )}
      </button>
      <button className="nav-item" onClick={app.openRules} title="Rules & Auto-Labeling">
        <Filter />
        <span className="nav-label">Rules &amp; Labels</span>
        {app.rules.length > 0 && <span className="nav-count">{app.rules.length}</span>}
      </button>
      <button className="nav-item" onClick={app.openTemplates} title="Templates">
        <FileText />
        <span className="nav-label">Templates</span>
        <span className="nav-count">{app.templates.length}</span>
      </button>

      {app.savedSearches.length > 0 && (
        <>
          <div className="sidebar-section">Saved Searches</div>
          {app.savedSearches.map((saved) => {
            const active =
              app.activeSection === "mail" &&
              app.view.kind === "search" &&
              app.view.text === saved.query;
            return (
              <button
                key={saved.id}
                className={`nav-item${active ? " active" : ""}`}
                onClick={() => app.selectView({ kind: "search", text: saved.query })}
                title={saved.query}
              >
                <Bookmark />
                <span className="nav-label">{saved.name}</span>
              </button>
            );
          })}
        </>
      )}

      {app.labels.length > 0 && (
        <>
          <div className="sidebar-section">Labels</div>
          {app.labels.map((label) => {
            const active =
              app.activeSection === "mail" &&
              app.view.kind === "label" &&
              app.view.labelId === label.id;
            return (
              <button
                key={label.id}
                className={`nav-item${active ? " active" : ""}`}
                onClick={() => app.selectView({ kind: "label", labelId: label.id })}
                title={label.name}
              >
                <Tag />
                <span className="nav-label">{label.name}</span>
                {label.threadCount > 0 && <span className="nav-count">{label.threadCount}</span>}
              </button>
            );
          })}
        </>
      )}

      {app.settings.developerToolsEnabled && (
        <div className="sidebar-footer">
          <div className="agent-slot" title="Developer Tools: Local MCP agent draft approval queue">
            <Sparkles />
            <span>Agent Queue · Dev</span>
          </div>
        </div>
      )}
    </aside>
  );
}
