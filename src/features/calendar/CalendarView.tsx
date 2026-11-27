import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Layers,
  MapPin,
  Pencil,
  Plus,
  Repeat,
  Sparkles,
  Trash2,
} from "lucide-react";
import type {
  CalendarEventView,
  CalendarOccurrence,
  OverlayBusyBlock,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";
import { EventEditorModal } from "./EventEditorModal";
import { IcsOverlaysModal } from "./IcsOverlaysModal";

export type CalendarMode = "day" | "week" | "month";

const DEPARTMENTS = [
  "all",
  "operations",
  "sponsors",
  "exhibition",
  "marketing",
  "volunteers",
  "workshops",
] as const;

function startOfDay(date: Date): Date {
  const d = new Date(date.getTime());
  d.setHours(0, 0, 0, 0);
  return d;
}

function addDays(date: Date, days: number): Date {
  const d = new Date(date.getTime());
  d.setDate(d.getDate() + days);
  return d;
}

function startOfWeek(date: Date): Date {
  const d = startOfDay(date);
  const diff = d.getDay();
  return addDays(d, -diff);
}

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function formatTimeShort(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

function deptColorClass(dept: string | null, kind: string): string {
  if (kind === "deadline") return "cal-pill-deadline";
  if (kind === "milestone") return "cal-pill-milestone";
  if (kind === "roomSchedule") return "cal-pill-room";
  switch (dept) {
    case "operations":
      return "cal-pill-ops";
    case "sponsors":
      return "cal-pill-sponsors";
    case "exhibition":
      return "cal-pill-exhibition";
    case "marketing":
      return "cal-pill-marketing";
    case "volunteers":
      return "cal-pill-volunteers";
    case "workshops":
      return "cal-pill-workshops";
    default:
      return "cal-pill-default";
  }
}

export function CalendarView({ app }: { app: MailApp }) {
  const [mode, setMode] = useState<CalendarMode>("week");
  const [cursor, setCursor] = useState<Date>(() => startOfDay(new Date()));
  const [deptFilter, setDeptFilter] = useState<string>("all");
  const [quickText, setQuickText] = useState("");
  const [quickBusy, setQuickBusy] = useState(false);
  const [editorEvent, setEditorEvent] = useState<CalendarEventView | null | undefined>(undefined);
  const [overlaysOpen, setOverlaysOpen] = useState(false);
  const [selectedOverlay, setSelectedOverlay] = useState<OverlayBusyBlock | null>(null);
  const quickRef = useRef<HTMLInputElement>(null);

  const range = useMemo(() => {
    if (mode === "day") {
      const start = startOfDay(cursor);
      const end = addDays(start, 1);
      return { start, end };
    }
    if (mode === "week") {
      const start = startOfWeek(cursor);
      const end = addDays(start, 7);
      return { start, end };
    }
    const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
    const start = startOfWeek(first);
    const end = addDays(start, 42);
    return { start, end };
  }, [mode, cursor]);

  const { loadCalendarRange } = app;
  useEffect(() => {
    void loadCalendarRange(range.start.getTime(), range.end.getTime());
  }, [range.start, range.end, loadCalendarRange]);

  const stepRange = useCallback(
    (dir: -1 | 1) => {
      setCursor((prev) => {
        if (mode === "day") return addDays(prev, dir);
        if (mode === "week") return addDays(prev, dir * 7);
        return new Date(prev.getFullYear(), prev.getMonth() + dir, 1);
      });
    },
    [mode],
  );

  // Keyboard navigation while in Calendar view.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (editorEvent !== undefined || overlaysOpen) return;
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      if (tag === "input" || tag === "textarea" || tag === "select") return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;

      if (event.key === "d") {
        setMode("day");
      } else if (event.key === "w") {
        setMode("week");
      } else if (event.key === "m") {
        setMode("month");
      } else if (event.key === "t") {
        setCursor(startOfDay(new Date()));
      } else if (event.key === "ArrowLeft" || event.key === "h") {
        stepRange(-1);
      } else if (event.key === "ArrowRight" || event.key === "l") {
        stepRange(1);
      } else if (event.key === "n") {
        event.preventDefault();
        quickRef.current?.focus();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [editorEvent, overlaysOpen, stepRange]);

  const filteredOccurrences = useMemo(() => {
    if (deptFilter === "all") return app.calendarOccurrences;
    return app.calendarOccurrences.filter((o) => o.event.department === deptFilter);
  }, [app.calendarOccurrences, deptFilter]);

  const daysToRender = useMemo(() => {
    const days: Date[] = [];
    let d = new Date(range.start.getTime());
    while (d < range.end) {
      days.push(new Date(d.getTime()));
      d = addDays(d, 1);
    }
    return days;
  }, [range.start, range.end]);

  const heading = useMemo(() => {
    if (mode === "day") {
      return cursor.toLocaleDateString(undefined, {
        weekday: "long",
        month: "long",
        day: "numeric",
        year: "numeric",
      });
    }
    if (mode === "week") {
      const end = addDays(range.end, -1);
      return `${range.start.toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${end.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
    }
    return cursor.toLocaleDateString(undefined, { month: "long", year: "numeric" });
  }, [mode, cursor, range.start, range.end]);

  async function submitQuickAdd(): Promise<void> {
    if (!quickText.trim() || quickBusy) return;
    setQuickBusy(true);
    try {
      const created = await app.quickAddEvent(quickText);
      setQuickText("");
      await app.loadCalendarRange(range.start.getTime(), range.end.getTime());
      await app.selectCalendarEvent(created);
    } finally {
      setQuickBusy(false);
    }
  }

  function itemsForDay(day: Date): {
    events: CalendarOccurrence[];
    blocks: OverlayBusyBlock[];
  } {
    const dayStart = startOfDay(day).getTime();
    const dayEnd = addDays(day, 1).getTime();
    return {
      events: filteredOccurrences.filter(
        (o) => o.occurrenceEnd > dayStart && o.occurrenceStart < dayEnd,
      ),
      blocks: app.overlayBusyBlocks.filter((b) => b.endAt > dayStart && b.startAt < dayEnd),
    };
  }

  const today = startOfDay(new Date());
  const classicShowReader =
    app.layout === "classic" && (app.selectedCalendarEvent !== null || selectedOverlay !== null);

  return (
    <section className={`calendar-workspace${classicShowReader ? " classic-reading" : ""}`}>
      <div className="calendar-main">
        <div className="calendar-toolbar">
          <div className="calendar-toolbar-left">
            <CalendarDays style={{ width: 16, height: 16, color: "var(--app-accent)" }} />
            <h2 className="calendar-title">{heading}</h2>
            <div className="cal-nav-group">
              <button
                type="button"
                className="t-btn"
                onClick={() => stepRange(-1)}
                title="Previous (← / h)"
              >
                <ChevronLeft />
              </button>
              <button
                type="button"
                className="t-btn"
                onClick={() => setCursor(startOfDay(new Date()))}
                title="Today (t)"
              >
                Today
              </button>
              <button
                type="button"
                className="t-btn"
                onClick={() => stepRange(1)}
                title="Next (→ / l)"
              >
                <ChevronRight />
              </button>
            </div>
          </div>

          <div className="calendar-toolbar-right">
            <div className="cal-mode-switch" role="tablist" aria-label="Calendar view mode">
              {(["day", "week", "month"] as const).map((m) => (
                <button
                  key={m}
                  type="button"
                  role="tab"
                  aria-selected={mode === m}
                  className={`cal-mode-btn${mode === m ? " active" : ""}`}
                  onClick={() => setMode(m)}
                  title={`${m[0]?.toUpperCase()}${m.slice(1)} view (${m[0]})`}
                >
                  {m[0]?.toUpperCase()}
                  {m.slice(1)}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="t-btn"
              onClick={() => setOverlaysOpen(true)}
              title="Manage read-only Personal .ics Overlays"
            >
              <Layers />
              Overlays ({app.icsOverlays.filter((o) => o.enabled).length})
            </button>
            <button
              type="button"
              className="btn compact primary"
              onClick={() => setEditorEvent(null)}
            >
              <Plus />
              New Event
            </button>
          </div>
        </div>

        <div className="calendar-quickadd-row">
          <Sparkles style={{ width: 14, height: 14, color: "var(--app-accent)", flexShrink: 0 }} />
          <input
            ref={quickRef}
            className="calendar-quickadd-input"
            placeholder='Quick add: "Dentist Thu 2pm @Downtown" or "Weekly review Fri 10am weekly" (press n to focus)'
            value={quickText}
            disabled={quickBusy}
            onChange={(event) => setQuickText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void submitQuickAdd();
              }
            }}
          />
          <button
            type="button"
            className="btn compact"
            disabled={!quickText.trim() || quickBusy}
            onClick={() => void submitQuickAdd()}
          >
            Add
          </button>
        </div>

        <div className="calendar-dept-bar">
          {DEPARTMENTS.map((dept) => (
            <button
              key={dept}
              type="button"
              className={`cal-dept-chip${deptFilter === dept ? " active" : ""}`}
              onClick={() => setDeptFilter(dept)}
            >
              {dept === "all" ? "All Departments" : `#${dept}`}
            </button>
          ))}
          <span className="cal-kbd-hint">Keys: D/W/M view · T today · ←/→ move · N quick-add</span>
        </div>

        <div className={`calendar-grid mode-${mode}`}>
          {daysToRender.map((day) => {
            const { events, blocks } = itemsForDay(day);
            const isToday = sameDay(day, today);
            const isOtherMonth = mode === "month" && day.getMonth() !== cursor.getMonth();
            return (
              <div
                key={day.toISOString()}
                className={`cal-day-cell${isToday ? " today" : ""}${isOtherMonth ? " muted" : ""}`}
              >
                <div className="cal-day-head">
                  <span className="cal-day-weekday">
                    {day.toLocaleDateString(undefined, { weekday: "short" })}
                  </span>
                  <span className="cal-day-num">{day.getDate()}</span>
                </div>

                <div className="cal-day-items">
                  {events.map((occ) => {
                    const ev = occ.event;
                    const selected =
                      selectedOverlay === null && app.selectedCalendarEvent?.id === ev.id;
                    return (
                      <button
                        key={`${ev.id}-${occ.occurrenceStart}`}
                        type="button"
                        className={`cal-event-card ${deptColorClass(ev.department, ev.kind)}${selected ? " selected" : ""}`}
                        onClick={() => {
                          setSelectedOverlay(null);
                          void app.selectCalendarEvent(ev);
                        }}
                      >
                        <div className="cal-event-top">
                          <span className="cal-event-time">
                            {ev.allDay ? "All day" : formatTimeShort(occ.occurrenceStart)}
                          </span>
                          {ev.recurrence !== "none" && (
                            <Repeat style={{ width: 10, height: 10, opacity: 0.8 }} />
                          )}
                        </div>
                        <div className="cal-event-title">{ev.title}</div>
                        {(ev.location || ev.department) && (
                          <div className="cal-event-meta">
                            {ev.department && <span>#{ev.department}</span>}
                            {ev.location && (
                              <span className="cal-event-loc">
                                <MapPin style={{ width: 9, height: 9 }} />
                                {ev.location}
                              </span>
                            )}
                          </div>
                        )}
                      </button>
                    );
                  })}

                  {blocks.map((block) => {
                    const isSelectedOverlay =
                      selectedOverlay !== null &&
                      selectedOverlay.overlayId === block.overlayId &&
                      selectedOverlay.uid === block.uid &&
                      selectedOverlay.startAt === block.startAt;
                    return (
                      <button
                        key={`${block.overlayId}-${block.uid}-${block.startAt}`}
                        type="button"
                        className={`cal-overlay-block${isSelectedOverlay ? " selected" : ""}`}
                        onClick={() => setSelectedOverlay(block)}
                        title={`Private local-only overlay (${block.overlayName}) — click to read details`}
                      >
                        <span className="cal-event-time">
                          {block.allDay ? "All day" : formatTimeShort(block.startAt)}
                        </span>
                        <span className="cal-overlay-title">{block.summary}</span>
                        <span className="cal-overlay-tag">Private</span>
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <aside className="meeting-doc-panel">
        {selectedOverlay ? (
          <div className="meeting-doc-body">
            <div className="meeting-doc-head">
              {app.layout === "classic" && (
                <button
                  type="button"
                  className="t-btn"
                  onClick={() => setSelectedOverlay(null)}
                >
                  <ChevronLeft />
                  Calendar
                </button>
              )}
              <div style={{ flex: 1 }}>
                <h3>{selectedOverlay.summary}</h3>
                <span className="hint-text">
                  {selectedOverlay.overlayName} ·{" "}
                  {selectedOverlay.allDay
                    ? "All day"
                    : `${formatTimeShort(selectedOverlay.startAt)} – ${formatTimeShort(selectedOverlay.endAt)}`}
                </span>
              </div>
            </div>
            <p className="hint-text">Read-only local .ics overlay block.</p>
          </div>
        ) : app.selectedCalendarEvent ? (
          <div className="meeting-doc-body">
            <div className="meeting-doc-head">
              {app.layout === "classic" && (
                <button
                  type="button"
                  className="t-btn"
                  onClick={() => void app.selectCalendarEvent(null)}
                >
                  <ChevronLeft />
                  Calendar
                </button>
              )}
              <div style={{ flex: 1 }}>
                <h3>{app.selectedCalendarEvent.title}</h3>
                <span className="hint-text">
                  {app.selectedCalendarEvent.allDay
                    ? "All day"
                    : `${formatTimeShort(app.selectedCalendarEvent.startAt)} – ${formatTimeShort(app.selectedCalendarEvent.endAt)}`}
                  {app.selectedCalendarEvent.location
                    ? ` · ${app.selectedCalendarEvent.location}`
                    : ""}
                </span>
              </div>
              <button
                type="button"
                className="icon-btn"
                title="Edit event"
                onClick={() => setEditorEvent(app.selectedCalendarEvent)}
              >
                <Pencil />
              </button>
              <button
                type="button"
                className="icon-btn"
                title="Delete event"
                onClick={() => {
                  const id = app.selectedCalendarEvent?.id;
                  if (!id) return;
                  void app.removeCalendarEvent(id).then(() => {
                    void app.loadCalendarRange(range.start.getTime(), range.end.getTime());
                  });
                }}
              >
                <Trash2 />
              </button>
            </div>
            {app.selectedCalendarEvent.description && (
              <p style={{ whiteSpace: "pre-wrap", fontSize: 13 }}>
                {app.selectedCalendarEvent.description}
              </p>
            )}
            {app.selectedCalendarEvent.attendees.length > 0 && (
              <div className="hint-text">
                Attendees: {app.selectedCalendarEvent.attendees.join(", ")}
              </div>
            )}
          </div>
        ) : (
          <div className="meeting-doc-empty">
            <CalendarDays style={{ width: 24, height: 24, opacity: 0.4 }} />
            <strong>Select an event</strong>
            <span className="hint-text">
              Click any event or overlay block to inspect details, or use quick-add above.
            </span>
          </div>
        )}
      </aside>

      {editorEvent !== undefined && (
        <EventEditorModal
          app={app}
          initial={editorEvent}
          onClose={() => setEditorEvent(undefined)}
          onSaved={(saved) => {
            setEditorEvent(undefined);
            void app.loadCalendarRange(range.start.getTime(), range.end.getTime());
            void app.selectCalendarEvent(saved);
          }}
        />
      )}

      {overlaysOpen && (
        <IcsOverlaysModal
          app={app}
          onClose={() => {
            setOverlaysOpen(false);
            void app.loadCalendarRange(range.start.getTime(), range.end.getTime());
          }}
        />
      )}
    </section>
  );
}
