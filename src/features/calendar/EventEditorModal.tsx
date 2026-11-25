import { useState } from "react";
import { CalendarDays, X } from "lucide-react";
import type {
  CalendarEventKind,
  CalendarEventView,
  RecurrenceRule,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

function toLocalDatetimeValue(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function EventEditorModal({
  app,
  initial,
  onClose,
  onSaved,
}: {
  app: MailApp;
  initial: CalendarEventView | null;
  onClose: () => void;
  onSaved: (event: CalendarEventView) => void;
}) {
  const defaultStart = Date.now() + 3_600_000;
  const [title, setTitle] = useState(initial?.title ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [startStr, setStartStr] = useState(() =>
    toLocalDatetimeValue(initial?.startAt ?? defaultStart),
  );
  const [endStr, setEndStr] = useState(() =>
    toLocalDatetimeValue(initial?.endAt ?? defaultStart + 3_600_000),
  );
  const [allDay, setAllDay] = useState(initial?.allDay ?? false);
  const [kind, setKind] = useState<CalendarEventKind>(initial?.kind ?? "meeting");
  const [department, setDepartment] = useState(initial?.department ?? "operations");
  const [location, setLocation] = useState(initial?.location ?? "");
  const [recurrence, setRecurrence] = useState<RecurrenceRule>(initial?.recurrence ?? "none");
  const [discordUrl, setDiscordUrl] = useState(initial?.discordUrl ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function handleSave(): Promise<void> {
    if (busy) return;
    const startAt = new Date(startStr).getTime();
    const endAt = new Date(endStr).getTime();
    if (!title.trim()) {
      setError("Title is required.");
      return;
    }
    if (Number.isNaN(startAt) || Number.isNaN(endAt) || endAt <= startAt) {
      setError("End time must be after start time.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await app.persistCalendarEvent({
        id: initial?.id ?? null,
        seriesId: initial?.seriesId ?? null,
        title: title.trim(),
        description,
        startAt,
        endAt,
        allDay,
        kind,
        department: department.trim() || null,
        location: location.trim() || null,
        recurrence,
        discordUrl: discordUrl.trim() || null,
        attendees: initial?.attendees ?? [],
      });
      onSaved(saved);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Pigment Calendar Event">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <CalendarDays style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>{initial ? "Edit Calendar Event" : "New Pigment Calendar Event"}</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="field">
            <label>Title</label>
            <input
              className="input"
              placeholder="e.g. Pigment Weekly Staff Sync"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>

          <div className="field-row">
            <div className="field">
              <label>Start</label>
              <input
                className="input"
                type="datetime-local"
                value={startStr}
                onChange={(e) => setStartStr(e.target.value)}
              />
            </div>
            <div className="field">
              <label>End</label>
              <input
                className="input"
                type="datetime-local"
                value={endStr}
                onChange={(e) => setEndStr(e.target.value)}
              />
            </div>
          </div>

          <div className="field-row">
            <div className="field">
              <label>Type</label>
              <select
                className="select"
                value={kind}
                onChange={(e) => setKind(e.target.value as CalendarEventKind)}
              >
                <option value="meeting">Staff Meeting</option>
                <option value="milestone">Show Milestone</option>
                <option value="deadline">Payment / Deadline</option>
                <option value="roomSchedule">Show-Weekend Room Schedule</option>
                <option value="showEvent">Show Event</option>
              </select>
            </div>
            <div className="field">
              <label>Department</label>
              <select
                className="select"
                value={department}
                onChange={(e) => setDepartment(e.target.value)}
              >
                <option value="operations">operations</option>
                <option value="sponsors">sponsors</option>
                <option value="exhibition">exhibition</option>
                <option value="marketing">marketing</option>
                <option value="volunteers">volunteers</option>
                <option value="workshops">workshops</option>
                <option value="general-business">general-business</option>
              </select>
            </div>
            <div className="field">
              <label>Recurrence</label>
              <select
                className="select"
                value={recurrence}
                onChange={(e) => setRecurrence(e.target.value as RecurrenceRule)}
              >
                <option value="none">One-time</option>
                <option value="weekly">Weekly</option>
                <option value="biweekly">Biweekly</option>
                <option value="monthly">Monthly</option>
              </select>
            </div>
          </div>

          <div className="field-row">
            <div className="field">
              <label>Where / Room Tag</label>
              <input
                className="input"
                placeholder="e.g. Ballroom A, Paint Lab 2, Discord #staff-voice"
                value={location}
                onChange={(e) => setLocation(e.target.value)}
              />
            </div>
            <div className="field">
              <label>Discord Voice / Channel Link</label>
              <input
                className="input"
                placeholder="discord://... or https://discord.com/channels/..."
                value={discordUrl}
                onChange={(e) => setDiscordUrl(e.target.value)}
              />
            </div>
          </div>

          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12 }}>
            <input
              type="checkbox"
              checked={allDay}
              onChange={(e) => setAllDay(e.target.checked)}
            />
            All-day event
          </label>

          <div className="field">
            <label>Description</label>
            <textarea
              className="textarea"
              rows={3}
              placeholder="Optional context or prep notes…"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>

          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          <span className="hint-text" style={{ flex: 1 }}>
            Synced to Pigment Staff Calendar when connected.
          </span>
          <button type="button" className="btn" disabled={busy} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy}
            onClick={() => void handleSave()}
          >
            Save Event
          </button>
        </div>
      </div>
    </div>
  );
}
