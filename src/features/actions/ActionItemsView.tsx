import { useMemo, useState } from "react";
import {
  CalendarDays,
  CheckCircle2,
  CheckSquare,
  Circle,
  Clock,
  DollarSign,
  Download,
  FileSpreadsheet,
  Mail,
  MapPin,
  Pencil,
  Plus,
  Search,
  Trash2,
} from "lucide-react";
import type { ActionItemStatus, ActionItemView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { ActionItemModal, PIGMENT_DEPARTMENTS } from "./ActionItemModal";
import { WorkbookImportModal } from "./WorkbookImportModal";

type ActionFilter = "all" | "mine" | "unassigned" | "byDept" | "byMeeting" | "upcoming";
type StatusFilter = "open" | "done" | "all";

function formatDateShort(ts: number): string {
  return new Date(ts).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });
}

function isOverdue(item: ActionItemView, todayUtcStart: number): boolean {
  return item.status === "open" && item.dueAt !== null && item.dueAt < todayUtcStart;
}

export function ActionItemsView({ app }: { app: MailApp }) {
  const [filter, setFilter] = useState<ActionFilter>("all");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("open");
  const [deptFilter, setDeptFilter] = useState<string>("all");
  const [meetingFilter, setMeetingFilter] = useState<string>("all");
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<ActionItemView | null>(null);
  const [creating, setCreating] = useState(false);
  const [importOpen, setImportOpen] = useState(false);

  const myHandle = (app.staffWorkspace.staffHandle || "reyth").toLowerCase();
  const todayUtcStart = useMemo(() => {
    const d = new Date();
    return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
  }, []);

  const eventTitleById = useMemo(() => {
    const map = new Map<string, string>();
    for (const occ of app.calendarOccurrences) {
      map.set(occ.event.id, occ.event.title);
    }
    return map;
  }, [app.calendarOccurrences]);

  const meetingsInItems = useMemo(() => {
    const seen = new Map<string, string>();
    for (const item of app.actionItems) {
      if (item.sourceEventId) {
        seen.set(
          item.sourceEventId,
          eventTitleById.get(item.sourceEventId) ?? "Meeting Doc",
        );
      }
    }
    return Array.from(seen.entries());
  }, [app.actionItems, eventTitleById]);

  const counts = useMemo(() => {
    let open = 0;
    let done = 0;
    let mine = 0;
    let unassigned = 0;
    let upcoming = 0;
    for (const item of app.actionItems) {
      if (item.status === "done") done += 1;
      else open += 1;
      if (item.status === "open" && item.assigneeHandle?.toLowerCase() === myHandle) mine += 1;
      if (item.status === "open" && !item.assigneeHandle) unassigned += 1;
      if (item.status === "open" && item.dueAt !== null) upcoming += 1;
    }
    return { open, done, mine, unassigned, upcoming };
  }, [app.actionItems, myHandle]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return app.actionItems
      .filter((item) => {
        if (statusFilter !== "all" && item.status !== statusFilter) return false;
        if (filter === "mine" && item.assigneeHandle?.toLowerCase() !== myHandle) return false;
        if (filter === "unassigned" && item.assigneeHandle !== null) return false;
        if (filter === "byDept" && deptFilter !== "all" && item.department !== deptFilter) {
          return false;
        }
        if (filter === "byMeeting") {
          if (!item.sourceEventId) return false;
          if (meetingFilter !== "all" && item.sourceEventId !== meetingFilter) return false;
        }
        if (filter === "upcoming" && item.dueAt === null) return false;
        if (q) {
          const hay = [
            item.title,
            item.department ?? "",
            item.milestone ?? "",
            item.assigneeHandle ?? "",
            item.location ?? "",
            item.costNote ?? "",
          ]
            .join(" ")
            .toLowerCase();
          if (!hay.includes(q)) return false;
        }
        return true;
      })
      .sort((a, b) => {
        if (a.status !== b.status) return a.status === "open" ? -1 : 1;
        const da = a.dueAt ?? Number.MAX_SAFE_INTEGER;
        const db = b.dueAt ?? Number.MAX_SAFE_INTEGER;
        if (da !== db) return da - db;
        return b.updatedAt - a.updatedAt;
      });
  }, [app.actionItems, statusFilter, filter, deptFilter, meetingFilter, query, myHandle]);

  function toggleStatus(item: ActionItemView): void {
    const next: ActionItemStatus = item.status === "done" ? "open" : "done";
    void app.setActionItemStatus(item.id, next);
  }

  return (
    <main className="actions-shell">
      <div className="actions-header">
        <div className="actions-title-group">
          <CheckSquare style={{ width: 17, height: 17, color: "var(--app-accent)" }} />
          <div>
            <h1>Action Items</h1>
            <span className="hint-text">
              {counts.open} open · {counts.done} done · Pigment 2027 Operations
            </span>
          </div>
        </div>

        <div className="actions-header-btns">
          <button className="btn compact" onClick={() => setImportOpen(true)}>
            <FileSpreadsheet style={{ width: 13, height: 13 }} />
            Import .xlsx / CSV
          </button>
          <button className="btn compact" onClick={() => void app.downloadActionItemsCsv()}>
            <Download style={{ width: 13, height: 13 }} />
            Export CSV
          </button>
          <button className="btn primary compact" onClick={() => setCreating(true)}>
            <Plus style={{ width: 13, height: 13 }} />
            New Action Item
          </button>
        </div>
      </div>

      <div className="actions-toolbar">
        <div className="actions-filter-pills">
          {(
            [
              { id: "all", label: "All" },
              { id: "mine", label: `Mine (${counts.mine})` },
              { id: "unassigned", label: `Unassigned (${counts.unassigned})` },
              { id: "byDept", label: "By Dept." },
              { id: "byMeeting", label: "By Meeting" },
              { id: "upcoming", label: `Upcoming / Overdue (${counts.upcoming})` },
            ] as const
          ).map((pill) => (
            <button
              key={pill.id}
              type="button"
              className={`cat-filter-btn${filter === pill.id ? " active" : ""}`}
              onClick={() => setFilter(pill.id)}
            >
              {pill.label}
            </button>
          ))}
        </div>

        <div className="actions-toolbar-right">
          {filter === "byDept" && (
            <select
              className="select compact-select"
              value={deptFilter}
              onChange={(e) => setDeptFilter(e.target.value)}
            >
              <option value="all">All departments</option>
              {PIGMENT_DEPARTMENTS.map((d) => (
                <option key={d} value={d}>
                  #{d}
                </option>
              ))}
            </select>
          )}
          {filter === "byMeeting" && (
            <select
              className="select compact-select"
              value={meetingFilter}
              onChange={(e) => setMeetingFilter(e.target.value)}
            >
              <option value="all">All meetings</option>
              {meetingsInItems.map(([id, title]) => (
                <option key={id} value={id}>
                  {title}
                </option>
              ))}
            </select>
          )}
          <div className="segmented-mini">
            {(["open", "done", "all"] as const).map((s) => (
              <button
                key={s}
                type="button"
                className={statusFilter === s ? "active" : ""}
                onClick={() => setStatusFilter(s)}
              >
                {s === "open" ? "Open" : s === "done" ? "Done" : "All"}
              </button>
            ))}
          </div>
          <div className="search-inline" style={{ width: 210 }}>
            <Search style={{ width: 13, height: 13 }} />
            <input
              className="input compact-input"
              placeholder="Search tasks, rooms, @handle…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        </div>
      </div>

      <div className="actions-list">
        {filtered.length === 0 ? (
          <div className="actions-empty">
            <CheckSquare style={{ width: 24, height: 24, opacity: 0.4 }} />
            <strong>No matching action items</strong>
            <span>Create a new task, promote a bullet from a Meeting Doc, or import your 2027 Operations Workbook.</span>
          </div>
        ) : (
          filtered.map((item) => {
            const overdue = isOverdue(item, todayUtcStart);
            return (
              <div
                key={item.id}
                className={`action-row${item.status === "done" ? " done" : ""}${overdue ? " overdue" : ""}`}
              >
                <button
                  type="button"
                  className="action-check-btn"
                  onClick={() => toggleStatus(item)}
                  title={item.status === "done" ? "Mark open" : "Mark done"}
                >
                  {item.status === "done" ? (
                    <CheckCircle2 style={{ width: 16, height: 16, color: "var(--app-shield)" }} />
                  ) : (
                    <Circle style={{ width: 16, height: 16 }} />
                  )}
                </button>

                <div className="action-row-main">
                  <div className="action-row-top">
                    <span className="action-row-title">{item.title}</span>
                    {item.department && <span className="dept-chip">#{item.department}</span>}
                    {item.milestone && <span className="milestone-chip">{item.milestone}</span>}
                  </div>
                  <div className="action-row-meta">
                    <span className={`assignee-chip${item.assigneeHandle ? "" : " unassigned"}`}>
                      {item.assigneeHandle ? `@${item.assigneeHandle}` : "Unassigned"}
                    </span>
                    {item.dueAt !== null && (
                      <span className={`action-meta-item${overdue ? " overdue-text" : ""}`}>
                        <Clock style={{ width: 11, height: 11 }} />
                        {overdue ? "Overdue · " : ""}
                        {formatDateShort(item.dueAt)}
                        {item.startTime
                          ? ` · ${item.startTime}${item.endTime ? `–${item.endTime}` : ""}`
                          : ""}
                      </span>
                    )}
                    {item.location && (
                      <span className="action-meta-item">
                        <MapPin style={{ width: 11, height: 11 }} />
                        {item.location}
                      </span>
                    )}
                    {item.costNote && (
                      <span className="action-meta-item cost">
                        <DollarSign style={{ width: 11, height: 11 }} />
                        {item.costNote}
                      </span>
                    )}
                  </div>
                </div>

                <div className="action-row-links">
                  {item.sourceThreadId && (
                    <button
                      type="button"
                      className="source-link-btn"
                      onClick={() => app.openThread(item.sourceThreadId!)}
                      title="Jump to originating email thread"
                    >
                      <Mail style={{ width: 11, height: 11 }} />
                      Email
                    </button>
                  )}
                  {item.sourceEventId && (
                    <button
                      type="button"
                      className="source-link-btn"
                      onClick={() => void app.jumpToCalendarEvent(item.sourceEventId!)}
                      title="Jump to originating Meeting Doc"
                    >
                      <CalendarDays style={{ width: 11, height: 11 }} />
                      Meeting
                    </button>
                  )}
                  {item.calendarEventId && item.calendarEventId !== item.sourceEventId && (
                    <button
                      type="button"
                      className="source-link-btn cal"
                      onClick={() => void app.jumpToCalendarEvent(item.calendarEventId!)}
                      title="View on Pigment Shared Calendar"
                    >
                      <CalendarDays style={{ width: 11, height: 11 }} />
                      Calendar
                    </button>
                  )}
                  <button
                    type="button"
                    className="icon-btn"
                    title="Edit action item"
                    onClick={() => setEditing(item)}
                  >
                    <Pencil />
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    title="Delete action item"
                    onClick={() => void app.removeActionItem(item.id)}
                  >
                    <Trash2 />
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>

      {(creating || editing !== null) && (
        <ActionItemModal
          app={app}
          initial={editing}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
        />
      )}

      {importOpen && <WorkbookImportModal app={app} onClose={() => setImportOpen(false)} />}
    </main>
  );
}
