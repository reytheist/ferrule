import {
  Archive,
  CalendarClock,
  FileText,
  Flag,
  Inbox,
  Send,
  Sparkles,
  Star,
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
  return (
    <aside className="sidebar">
      <div className="sidebar-section">Mailboxes</div>
      {ENTRIES.map((entry) => {
        const Icon = entry.icon;
        const count = entry.count(app);
        const active = sameView(app.view, entry.view);
        return (
          <button
            key={entry.key}
            className={`nav-item${active ? " active" : ""}`}
            onClick={() => app.selectView(entry.view)}
            title={entry.label}
          >
            <Icon />
            <span className="nav-label">{entry.label}</span>
            {count !== null && <span className="nav-count">{count}</span>}
          </button>
        );
      })}

      <div className="sidebar-section">Library</div>
      <button className="nav-item" onClick={app.openTemplates} title="Templates">
        <FileText />
        <span className="nav-label">Templates</span>
        <span className="nav-count">{app.templates.length}</span>
      </button>

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
