import { useState } from "react";
import { CalendarDays, CheckSquare, Mail, X } from "lucide-react";
import type {
  ActionItemStatus,
  ActionItemView,
  CalendarEventId,
  SaveActionItemInput,
  ThreadId,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export const PIGMENT_DEPARTMENTS = [
  "marketing",
  "sponsors",
  "exhibition",
  "volunteers",
  "operations",
  "workshops",
  "shopping-list",
  "code-monkeys",
  "general-business",
] as const;

export interface ActionItemDraftSeed {
  title?: string;
  department?: string | null;
  milestone?: string | null;
  sourceThreadId?: ThreadId | null;
  sourceEventId?: CalendarEventId | null;
  sourceSeriesId?: string | null;
}

function toDateInputValue(ts: number | null): string {
  if (ts === null) return "";
  const d = new Date(ts);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

function fromDateInputValue(raw: string): number | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const parts = trimmed.split("-").map(Number);
  if (parts.length !== 3 || parts.some((n) => !Number.isFinite(n))) return null;
  return Date.UTC(parts[0]!, parts[1]! - 1, parts[2]!);
}

export function ActionItemModal({
  app,
  initial,
  seed,
  onClose,
}: {
  app: MailApp;
  initial: ActionItemView | null;
  seed?: ActionItemDraftSeed | null;
  onClose(): void;
}) {
  const [title, setTitle] = useState(initial?.title ?? seed?.title ?? "");
  const [department, setDepartment] = useState(
    initial?.department ?? seed?.department ?? "operations",
  );
  const [milestone, setMilestone] = useState(initial?.milestone ?? seed?.milestone ?? "PMS 2027");
  const [assigneeHandle, setAssigneeHandle] = useState(initial?.assigneeHandle ?? "");
  const [status, setStatus] = useState<ActionItemStatus>(initial?.status ?? "open");
  const [dueDate, setDueDate] = useState(toDateInputValue(initial?.dueAt ?? null));
  const [startTime, setStartTime] = useState(initial?.startTime ?? "");
  const [endTime, setEndTime] = useState(initial?.endTime ?? "");
  const [location, setLocation] = useState(initial?.location ?? "");
  const [costNote, setCostNote] = useState(initial?.costNote ?? "");
  const [syncToCalendar, setSyncToCalendar] = useState(Boolean(initial?.calendarEventId));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sourceThreadId = initial?.sourceThreadId ?? seed?.sourceThreadId ?? null;
  const sourceEventId = initial?.sourceEventId ?? seed?.sourceEventId ?? null;
  const sourceSeriesId = initial?.sourceSeriesId ?? seed?.sourceSeriesId ?? null;

  async function submit(): Promise<void> {
    if (!title.trim()) {
      setError("Title / Task is required.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const input: SaveActionItemInput = {
        id: initial?.id ?? null,
        title: title.trim(),
        department: department.trim() || null,
        milestone: milestone.trim() || null,
        assigneeHandle: assigneeHandle.trim() || null,
        status,
        dueAt: fromDateInputValue(dueDate),
        startTime: startTime.trim() || null,
        endTime: endTime.trim() || null,
        location: location.trim() || null,
        costNote: costNote.trim() || null,
        sourceEventId,
        sourceSeriesId,
        sourceThreadId,
        syncToCalendar,
      };
      await app.persistActionItem(input);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-label={initial ? "Edit Action Item" : "New Action Item"}>
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <CheckSquare style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>{initial ? "Edit Action Item" : "New Action Item"}</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close">
            <X />
          </button>
        </div>

        <div className="modal-body">
          {(sourceThreadId || sourceEventId) && (
            <div className="action-source-banner">
              {sourceThreadId ? <Mail style={{ width: 12, height: 12 }} /> : <CalendarDays style={{ width: 12, height: 12 }} />}
              <span>
                Linked to originating {sourceThreadId ? "email thread" : "meeting doc"}
              </span>
            </div>
          )}

          <div className="field">
            <label>Title / Task</label>
            <input
              className="input"
              placeholder="Confirm Ballroom A power drop map with hotel AV"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
            />
          </div>

          <div className="field-row">
            <div className="field">
              <label>Dept.</label>
              <select
                className="select"
                value={department}
                onChange={(e) => setDepartment(e.target.value)}
              >
                <option value="">None</option>
                {PIGMENT_DEPARTMENTS.map((dept) => (
                  <option key={dept} value={dept}>
                    #{dept}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label>Event / Milestone (What)</label>
              <input
                className="input"
                placeholder="PMS 2027, ReaperCon 2027…"
                value={milestone}
                onChange={(e) => setMilestone(e.target.value)}
              />
            </div>
          </div>

          <div className="field-row">
            <div className="field">
              <label>Assignee</label>
              <select
                className="select"
                value={assigneeHandle}
                onChange={(e) => setAssigneeHandle(e.target.value)}
              >
                <option value="">Unassigned</option>
                {app.staffDirectory.map((member) => (
                  <option key={member.id} value={member.handle}>
                    @{member.handle} · {member.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="field" style={{ maxWidth: 140 }}>
              <label>Status</label>
              <select
                className="select"
                value={status}
                onChange={(e) => setStatus(e.target.value as ActionItemStatus)}
              >
                <option value="open">Open</option>
                <option value="done">Done</option>
              </select>
            </div>
          </div>

          <div className="field-row">
            <div className="field">
              <label>Due Date</label>
              <input
                type="date"
                className="input"
                value={dueDate}
                onChange={(e) => setDueDate(e.target.value)}
              />
            </div>
            <div className="field" style={{ maxWidth: 120 }}>
              <label>Start Time</label>
              <input
                type="time"
                className="input"
                value={startTime}
                onChange={(e) => setStartTime(e.target.value)}
              />
            </div>
            <div className="field" style={{ maxWidth: 120 }}>
              <label>End Time</label>
              <input
                type="time"
                className="input"
                value={endTime}
                onChange={(e) => setEndTime(e.target.value)}
              />
            </div>
          </div>

          <div className="field-row">
            <div className="field">
              <label>Where (Room / Venue)</label>
              <input
                className="input"
                placeholder="Ballroom A, Paint Lab, Registration…"
                value={location}
                onChange={(e) => setLocation(e.target.value)}
              />
            </div>
            <div className="field" style={{ maxWidth: 160 }}>
              <label>Cost / $$$</label>
              <input
                className="input"
                placeholder="$450 AV drop"
                value={costNote}
                onChange={(e) => setCostNote(e.target.value)}
              />
            </div>
          </div>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={syncToCalendar}
              disabled={!dueDate}
              onChange={(e) => setSyncToCalendar(e.target.checked)}
            />
            <span>
              Show on Pigment Shared Calendar
              {!dueDate && " (set a Due Date to enable)"}
            </span>
          </label>

          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          {initial && (
            <button
              className="btn danger"
              onClick={() => {
                void app.removeActionItem(initial.id);
                onClose();
              }}
            >
              Delete
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={saving} onClick={() => void submit()}>
            {saving ? "Saving…" : initial ? "Save changes" : "Create Action Item"}
          </button>
        </div>
      </div>
    </div>
  );
}
