import { useMemo, useState } from "react";
import {
  CalendarDays,
  CheckCircle2,
  CheckSquare,
  ChevronLeft,
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
  User,
} from "lucide-react";
import type { ActionItemId, ActionItemStatus, ActionItemView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { formatFull } from "../ui/format";
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

function formatDateLong(ts: number): string {
  return new Date(ts).toLocaleDateString(undefined, {
    weekday: "short",
    month: "long",
    day: "numeric",
    year: "numeric",
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
  const [selectedId, setSelectedId] = useState<ActionItemId | null>(null);
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

  const selectedItem = useMemo(
    () => (selectedId ? app.actionItems.find((item) => item.id === selectedId) ?? null : null),
    [app.actionItems, selectedId],
  );

  const selectedOverdue = selectedItem ? isOverdue(selectedItem, todayUtcStart) : false;
  const classicShowReader = app.layout === "classic" && selectedItem !== null;

  function toggleStatus(item: ActionItemView): void {
    const next: ActionItemStatus = item.status === "done" ? "open" : "done";
    void app.setActionItemStatus(item.id, next);
  }

  async function deleteItem(item: ActionItemView): Promise<void> {
    if (selectedId === item.id) setSelectedId(null);
    await app.removeActionItem(item.id);
  }

  return (
    <div className={`actions-workspace${classicShowReader ? " classic-reading" : ""}`}>
      <section className="actions-shell">
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
              Import
            </button>
            <button className="btn compact" onClick={() => void app.downloadActionItemsCsv()}>
              <Download style={{ width: 13, height: 13 }} />
              CSV
            </button>
            <button className="btn primary compact" onClick={() => setCreating(true)}>
              <Plus style={{ width: 13, height: 13 }} />
              New
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
                { id: "upcoming", label: `Upcoming (${counts.upcoming})` },
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
            <div className="search-inline" style={{ width: 190 }}>
              <Search style={{ width: 13, height: 13 }} />
              <input
                className="input compact-input"
                placeholder="Search tasks, @handle…"
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
              <span>
                Create a new task, promote a bullet from a Meeting Doc, or import your 2027
                Operations Workbook.
              </span>
            </div>
          ) : (
            filtered.map((item) => {
              const overdue = isOverdue(item, todayUtcStart);
              const selected = selectedItem?.id === item.id;
              return (
                <div
                  key={item.id}
                  className={`action-row${item.status === "done" ? " done" : ""}${overdue ? " overdue" : ""}${selected ? " selected" : ""}`}
                  role="button"
                  tabIndex={0}
                  onClick={() => setSelectedId(item.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelectedId(item.id);
                    }
                  }}
                >
                  <button
                    type="button"
                    className="action-check-btn"
                    onClick={(event) => {
                      event.stopPropagation();
                      toggleStatus(item);
                    }}
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
                </div>
              );
            })
          )}
        </div>
      </section>

      <main className="read-col actions-read-col">
        <div className="read-card">
          {!selectedItem ? (
            <div className="read-placeholder">
              <CheckSquare />
              <strong>No action item selected</strong>
              <span>
                Pick an action item on the left to read its full details, due window, budget note,
                and linked email or meeting doc.
              </span>
            </div>
          ) : (
            <>
              <div className="read-toolbar">
                {app.layout === "classic" && (
                  <button className="t-btn" onClick={() => setSelectedId(null)}>
                    <ChevronLeft />
                    Action Items
                  </button>
                )}
                <button
                  type="button"
                  className={`t-btn${selectedItem.status === "done" ? " on" : ""}`}
                  onClick={() => toggleStatus(selectedItem)}
                >
                  {selectedItem.status === "done" ? <CheckCircle2 /> : <Circle />}
                  {selectedItem.status === "done" ? "Completed" : "Mark Done"}
                </button>
                <button type="button" className="t-btn" onClick={() => setEditing(selectedItem)}>
                  <Pencil />
                  Edit
                </button>
                {selectedItem.sourceThreadId && (
                  <button
                    type="button"
                    className="t-btn"
                    onClick={() => app.openThread(selectedItem.sourceThreadId!)}
                  >
                    <Mail />
                    Email Thread
                  </button>
                )}
                {selectedItem.sourceEventId && (
                  <button
                    type="button"
                    className="t-btn"
                    onClick={() => void app.jumpToCalendarEvent(selectedItem.sourceEventId!)}
                  >
                    <CalendarDays />
                    Meeting Doc
                  </button>
                )}
                {selectedItem.calendarEventId &&
                  selectedItem.calendarEventId !== selectedItem.sourceEventId && (
                    <button
                      type="button"
                      className="t-btn"
                      onClick={() => void app.jumpToCalendarEvent(selectedItem.calendarEventId!)}
                    >
                      <CalendarDays />
                      Calendar
                    </button>
                  )}
                <span style={{ flex: 1 }} />
                <button
                  type="button"
                  className="t-btn"
                  title="Delete action item"
                  onClick={() => void deleteItem(selectedItem)}
                >
                  <Trash2 />
                  Delete
                </button>
              </div>

              <div className="read-scroll">
                <div className="read-header">
                  <div className="read-subject">{selectedItem.title}</div>
                  <div className="pane-badge-strip">
                    <span
                      className={`handoff-pill status-${selectedItem.status === "done" ? "done" : selectedOverdue ? "waiting" : "open"}`}
                    >
                      {selectedItem.status === "done"
                        ? "Done"
                        : selectedOverdue
                          ? "Overdue"
                          : "Open"}
                    </span>
                    {selectedItem.department && (
                      <span className="dept-chip">#{selectedItem.department}</span>
                    )}
                    {selectedItem.milestone && (
                      <span className="milestone-chip">{selectedItem.milestone}</span>
                    )}
                  </div>
                </div>

                <div className="detail-grid-card">
                  <div className="detail-grid">
                    <div className="detail-field">
                      <span className="detail-label">
                        <User style={{ width: 12, height: 12 }} />
                        Assignee
                      </span>
                      <span className="detail-value">
                        {selectedItem.assigneeHandle ? `@${selectedItem.assigneeHandle}` : "Unassigned"}
                      </span>
                    </div>

                    <div className="detail-field">
                      <span className="detail-label">
                        <Clock style={{ width: 12, height: 12 }} />
                        Due Date &amp; Time
                      </span>
                      <span className={`detail-value${selectedOverdue ? " overdue-text" : ""}`}>
                        {selectedItem.dueAt !== null ? (
                          <>
                            {formatDateLong(selectedItem.dueAt)}
                            {selectedItem.startTime
                              ? ` · ${selectedItem.startTime}${selectedItem.endTime ? `–${selectedItem.endTime}` : ""}`
                              : ""}
                          </>
                        ) : (
                          "No due date"
                        )}
                      </span>
                    </div>

                    <div className="detail-field">
                      <span className="detail-label">
                        <MapPin style={{ width: 12, height: 12 }} />
                        Location
                      </span>
                      <span className="detail-value">{selectedItem.location ?? "—"}</span>
                    </div>

                    <div className="detail-field">
                      <span className="detail-label">
                        <DollarSign style={{ width: 12, height: 12 }} />
                        Cost / Budget
                      </span>
                      <span className="detail-value">{selectedItem.costNote ?? "—"}</span>
                    </div>

                    <div className="detail-field">
                      <span className="detail-label">Last Updated</span>
                      <span className="detail-value">{formatFull(selectedItem.updatedAt)}</span>
                    </div>
                  </div>
                </div>

                <div className="detail-links-card">
                  <div className="meeting-section-title">Linked Context</div>
                  {selectedItem.sourceThreadId ||
                  selectedItem.sourceEventId ||
                  selectedItem.calendarEventId ? (
                    <div className="detail-links-list">
                      {selectedItem.sourceThreadId && (
                        <button
                          type="button"
                          className="btn compact"
                          onClick={() => app.openThread(selectedItem.sourceThreadId!)}
                        >
                          <Mail style={{ width: 13, height: 13 }} />
                          Open originating email thread
                        </button>
                      )}
                      {selectedItem.sourceEventId && (
                        <button
                          type="button"
                          className="btn compact"
                          onClick={() => void app.jumpToCalendarEvent(selectedItem.sourceEventId!)}
                        >
                          <CalendarDays style={{ width: 13, height: 13 }} />
                          Open originating Meeting Doc (
                          {eventTitleById.get(selectedItem.sourceEventId) ?? "Meeting"})
                        </button>
                      )}
                      {selectedItem.calendarEventId &&
                        selectedItem.calendarEventId !== selectedItem.sourceEventId && (
                          <button
                            type="button"
                            className="btn compact"
                            onClick={() =>
                              void app.jumpToCalendarEvent(selectedItem.calendarEventId!)
                            }
                          >
                            <CalendarDays style={{ width: 13, height: 13 }} />
                            View on Pigment Shared Calendar
                          </button>
                        )}
                    </div>
                  ) : (
                    <span className="hint-text">
                      Not linked to an email or calendar event yet. Click Edit to add a due date and
                      sync this task to the Pigment Shared Calendar.
                    </span>
                  )}
                </div>
              </div>
            </>
          )}
        </div>
      </main>

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
    </div>
  );
}
