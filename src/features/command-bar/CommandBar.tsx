import { useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType, KeyboardEvent as ReactKeyboardEvent } from "react";
import {
  Archive,
  Clock,
  FileText,
  Flag,
  Inbox,
  Moon,
  PenLine,
  Plus,
  RefreshCw,
  Search,
  Send,
  Settings,
  Star,
  Sun,
  Trash2,
} from "lucide-react";
import { listThreads } from "../../ipc/core";
import type { Scope, ThreadSummary } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { spaceClass } from "../spaces/SpaceSwitcher";
import { formatThreadTime } from "../ui/format";

interface CommandItem {
  id: string;
  label: string;
  hint?: string;
  icon?: ComponentType<{ size?: number }>;
  run(): void;
}

function tomorrowAt9(): number {
  const at = new Date();
  at.setDate(at.getDate() + 1);
  at.setHours(9, 0, 0, 0);
  return at.getTime();
}

export function CommandBar({ app }: { app: MailApp }) {
  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState(0);
  const [results, setResults] = useState<ThreadSummary[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const scope: Scope = useMemo(
    () =>
      app.scopeAccount
        ? { kind: "account", accountId: app.scopeAccount }
        : { kind: "unified" },
    [app.scopeAccount],
  );

  const open = app.commandOpen;

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setResults([]);
    setHighlight(0);
    inputRef.current?.focus();
  }, [open]);

  // Instant mail search, debounced so typing stays smooth.
  useEffect(() => {
    const text = query.trim();
    if (!open || text.length === 0) {
      setResults([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void listThreads({ scope, view: { kind: "search", text }, limit: 8, offset: 0 })
        .then((page) => {
          if (!cancelled) setResults(page.items);
        })
        .catch(() => {
          if (!cancelled) setResults([]);
        });
    }, 120);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, open, scope]);

  const commands = useMemo<CommandItem[]>(() => {
    const selected = app.selectedId;
    const items: CommandItem[] = [
      { id: "compose", label: "Compose new message", hint: "C", icon: PenLine, run: app.openCompose },
      { id: "sync", label: "Sync all accounts", icon: RefreshCw, run: () => void app.syncNow() },
      {
        id: "theme",
        label: app.theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
        icon: app.theme === "dark" ? Sun : Moon,
        run: app.toggleTheme,
      },
      {
        id: "layout",
        label: app.layout === "split" ? "Switch to classic layout" : "Switch to split layout",
        run: app.toggleLayout,
      },
      { id: "add-account", label: "Add account", icon: Plus, run: app.openAddAccount },
      { id: "settings", label: "Open Settings", hint: "Ctrl ,", icon: Settings, run: app.openSettings },
      { id: "templates", label: "Manage templates & snippets", icon: FileText, run: app.openTemplates },
      {
        id: "archive",
        label: "Archive current thread",
        hint: "E",
        icon: Archive,
        run: () => {
          if (selected) void app.archiveThread(selected);
        },
      },
      {
        id: "snooze",
        label: "Snooze until tomorrow 9:00 AM",
        hint: "H",
        icon: Clock,
        run: () => {
          if (selected) void app.snooze(selected, tomorrowAt9());
        },
      },
      {
        id: "star",
        label: "Star / unstar current thread",
        hint: "S",
        icon: Star,
        run: () => {
          if (selected) void app.starThread(selected);
        },
      },
      {
        id: "trash",
        label: "Move current thread to trash",
        icon: Trash2,
        run: () => {
          if (selected) void app.trashThread(selected);
        },
      },
      { id: "go-inbox", label: "Go to Unified Inbox", icon: Inbox, run: () => app.selectView({ kind: "inbox", category: "all" }) },
      { id: "go-snoozed", label: "Go to Snoozed", icon: Clock, run: () => app.selectView({ kind: "snoozed" }) },
      { id: "go-followups", label: "Go to Follow-ups", icon: Flag, run: () => app.selectView({ kind: "followUps" }) },
      { id: "go-scheduled", label: "Go to Scheduled", icon: Send, run: () => app.selectView({ kind: "scheduled" }) },
      { id: "go-starred", label: "Go to Starred", icon: Star, run: () => app.selectView({ kind: "starred" }) },
      { id: "go-sent", label: "Go to Sent", icon: Send, run: () => app.selectView({ kind: "sent" }) },
      { id: "go-archive", label: "Go to Archive", icon: Archive, run: () => app.selectView({ kind: "archive" }) },
      { id: "go-trash", label: "Go to Trash", icon: Trash2, run: () => app.selectView({ kind: "trash" }) },
    ];
    for (const template of app.templates) {
      items.push({
        id: `tpl-${template.id}`,
        label: `Insert snippet: ${template.name}`,
        hint: template.shortcut ?? undefined,
        icon: FileText,
        run: () => app.insertTemplate(template),
      });
    }
    return items;
  }, [app]);

  const trimmed = query.trim();
  const lowered = trimmed.toLowerCase();
  const seeAll: CommandItem | null =
    trimmed.length > 0
      ? {
          id: "see-all",
          label: `See all results for “${trimmed}”`,
          icon: Search,
          run: () => app.selectView({ kind: "search", text: trimmed }),
        }
      : null;
  const filteredCommands = seeAll
    ? [...commands.filter((c) => c.label.toLowerCase().includes(lowered)), seeAll]
    : commands;

  const total = filteredCommands.length + results.length;
  const activeIndex = total === 0 ? 0 : Math.min(highlight, total - 1);

  useEffect(() => {
    setHighlight(0);
  }, [query]);

  if (!open) return null;

  function runIndex(index: number): void {
    const command = filteredCommands[index];
    if (command) {
      app.closeCommand();
      command.run();
      return;
    }
    const thread = results[index - filteredCommands.length];
    if (thread) {
      app.closeCommand();
      app.openThread(thread.id);
    }
  }

  function onKeyDown(event: ReactKeyboardEvent<HTMLInputElement>): void {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setHighlight((prev) => (total === 0 ? 0 : (prev + 1) % total));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlight((prev) => (total === 0 ? 0 : (prev - 1 + total) % total));
    } else if (event.key === "Enter") {
      event.preventDefault();
      runIndex(activeIndex);
    }
  }

  return (
    <div
      className="cmd-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeCommand();
      }}
    >
      <div className="cmd-box" role="dialog" aria-label="Command palette">
        <input
          ref={inputRef}
          className="cmd-input"
          placeholder="Type a command, or search mail… (j/k to move, Enter to run)"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="cmd-list">
          {filteredCommands.length > 0 && (
            <span className="cmd-group-label">{trimmed ? "Commands" : "Actions"}</span>
          )}
          {filteredCommands.map((command, index) => {
            const Icon = command.icon;
            return (
              <button
                key={command.id}
                className={`cmd-item${index === activeIndex ? " hi" : ""}`}
                onMouseEnter={() => setHighlight(index)}
                onClick={() => runIndex(index)}
              >
                {Icon && <Icon />}
                <span className="ci-main">{command.label}</span>
                {command.hint && <span className="kbd">{command.hint}</span>}
              </button>
            );
          })}

          {results.length > 0 && <span className="cmd-group-label">Mail</span>}
          {results.map((thread, index) => (
            <button
              key={thread.id}
              className={`cmd-item${index + filteredCommands.length === activeIndex ? " hi" : ""}`}
              onMouseEnter={() => setHighlight(index + filteredCommands.length)}
              onClick={() => runIndex(index + filteredCommands.length)}
            >
              <span className={`acct-dot ${spaceClass(thread.accountColor)}`} />
              <span className="ci-main">{thread.subject}</span>
              <span className="ci-sub">
                {thread.senderName} · {formatThreadTime(thread.lastAt)}
              </span>
            </button>
          ))}

          {total === 0 && (
            <div className="cmd-empty">
              {trimmed ? "No commands or mail match that." : "No commands available."}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
