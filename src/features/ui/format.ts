import type { InboxCategory, MailboxView, UnixMs } from "../../types/mail";
import { assertNever } from "../../lib/assertNever";

const TIME_FMT = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const DATE_FMT = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const WEEKDAY_FMT = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const FULL_FMT = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
});

function startOfDay(ms: UnixMs): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function formatClock(ms: UnixMs): string {
  return TIME_FMT.format(ms);
}

export function formatThreadTime(ms: UnixMs): string {
  const today = startOfDay(Date.now());
  const day = startOfDay(ms);
  if (day >= today) return TIME_FMT.format(ms);
  if (day >= today - 86_400_000) return "Yesterday";
  if (day >= today - 6 * 86_400_000) return WEEKDAY_FMT.format(ms);
  return DATE_FMT.format(ms);
}

export function formatFull(ms: UnixMs): string {
  return FULL_FMT.format(ms);
}

/** Human phrasing for a future timestamp in toasts: "today at 5:00 PM". */
export function formatDue(ms: UnixMs): string {
  const now = Date.now();
  const today = startOfDay(now);
  const day = startOfDay(ms);
  const clock = TIME_FMT.format(ms);
  if (day === today) return `today at ${clock}`;
  if (day === today + 86_400_000) return `tomorrow at ${clock}`;
  const diffDays = Math.round((day - today) / 86_400_000);
  if (diffDays > 1 && diffDays < 7) return `${WEEKDAY_FMT.format(ms)} at ${clock}`;
  return `${FULL_FMT.format(ms)}`;
}

export function viewTitle(view: MailboxView): string {
  switch (view.kind) {
    case "inbox":
      return "Inbox";
    case "snoozed":
      return "Snoozed";
    case "followUps":
      return "Follow-ups";
    case "scheduled":
      return "Scheduled";
    case "sent":
      return "Sent";
    case "drafts":
      return "Drafts";
    case "starred":
      return "Starred";
    case "archive":
      return "Archive";
    case "trash":
      return "Trash";
    case "label":
      return "Labelled";
    case "search":
      return `Search: “${view.text}”`;
    default:
      return assertNever(view);
  }
}

export function categoryLabel(category: InboxCategory): string {
  switch (category) {
    case "primary":
      return "Primary";
    case "receipts":
      return "Receipts";
    case "newsletters":
      return "Newsletters";
    case "all":
      return "All";
    default:
      return assertNever(category);
  }
}

export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const first = parts[0];
  if (!first) return "?";
  const second = parts[1];
  if (!second) return first.slice(0, 2).toUpperCase();
  return (first[0] ?? "").toUpperCase() + (second[0] ?? "").toUpperCase();
}
