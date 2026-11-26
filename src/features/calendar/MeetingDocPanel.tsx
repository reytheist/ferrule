import { useEffect, useState } from "react";
import {
  ArrowUpRight,
  CalendarDays,
  CheckCircle2,
  ChevronLeft,
  Circle,
  ClipboardCopy,
  Clock,
  Edit3,
  ExternalLink,
  Layers,
  ListTodo,
  Lock,
  MapPin,
  MessageSquare,
  Plus,
  Repeat,
  Send,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import type { AgendaItem, CalendarEventView, OverlayBusyBlock } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

function formatEventRange(startAt: number, endAt: number, allDay: boolean): string {
  const startDate = new Date(startAt);
  const endDate = new Date(endAt);
  if (allDay) {
    return `${startDate.toLocaleDateString(undefined, {
      weekday: "short",
      month: "long",
      day: "numeric",
      year: "numeric",
    })} · All day`;
  }
  const datePart = startDate.toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
  });
  const startTime = startDate.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  const endTime = endDate.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  return `${datePart} · ${startTime} – ${endTime}`;
}

function eventKindLabel(kind: CalendarEventView["kind"]): string {
  switch (kind) {
    case "deadline":
      return "Deadline";
    case "milestone":
      return "Milestone";
    case "roomSchedule":
      return "Room Schedule";
    default:
      return "Meeting";
  }
}

export function MeetingDocPanel({
  app,
  selectedOverlay,
  onClearSelection,
  onEditEvent,
  onOpenOverlays,
  onAfterMutate,
}: {
  app: MailApp;
  selectedOverlay: OverlayBusyBlock | null;
  onClearSelection: () => void;
  onEditEvent: (event: CalendarEventView) => void;
  onOpenOverlays: () => void;
  onAfterMutate: () => void;
}) {
  const event = app.selectedCalendarEvent;
  const doc = app.activeMeetingDoc;
  const [newTopic, setNewTopic] = useState("");
  const [notesDraft, setNotesDraft] = useState("");
  const [editingNotes, setEditingNotes] = useState(false);
  const [newActionTitle, setNewActionTitle] = useState("");
  const [newActionAssignee, setNewActionAssignee] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setNotesDraft(doc?.notesMarkdown ?? "");
    setEditingNotes(false);
  }, [doc?.eventId, doc?.notesMarkdown]);

  if (selectedOverlay) {
    return (
      <main className="read-col calendar-read-col">
        <div className="read-card">
          <div className="read-toolbar">
            {app.layout === "classic" && (
              <button type="button" className="t-btn" onClick={onClearSelection}>
                <ChevronLeft />
                Calendar
              </button>
            )}
            <span className="badge badge-shield">
              <Layers style={{ width: 10, height: 10 }} />
              Private Overlay · {selectedOverlay.overlayName}
            </span>
            <span style={{ flex: 1 }} />
            <button type="button" className="t-btn" onClick={onOpenOverlays}>
              Manage Overlays
            </button>
          </div>

          <div className="read-scroll">
            <div className="read-header">
              <div className="read-subject">{selectedOverlay.summary}</div>
              <div className="privacy-ribbon">
                <span className="pr-left">
                  <ShieldCheck />
                  Read-only personal .ics overlay — kept 100% local and never synced to Staff Sync
                </span>
              </div>
            </div>

            <div className="detail-grid-card">
              <div className="detail-grid">
                <div className="detail-field">
                  <span className="detail-label">
                    <Clock style={{ width: 12, height: 12 }} />
                    When
                  </span>
                  <span className="detail-value">
                    {formatEventRange(
                      selectedOverlay.startAt,
                      selectedOverlay.endAt,
                      selectedOverlay.allDay,
                    )}
                  </span>
                </div>
                <div className="detail-field">
                  <span className="detail-label">
                    <Layers style={{ width: 12, height: 12 }} />
                    Source Calendar
                  </span>
                  <span className="detail-value">{selectedOverlay.overlayName}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>
    );
  }

  if (!event) {
    return (
      <main className="read-col calendar-read-col">
        <div className="read-card">
          <div className="read-placeholder">
            <CalendarDays />
            <strong>No calendar item selected</strong>
            <span>
              Select any Pigment calendar event or private overlay block to read its full details,
              agenda, shared notes, action items, and Discord recap.
            </span>
          </div>
        </div>
      </main>
    );
  }

  const when = formatEventRange(event.startAt, event.endAt, event.allDay);

  async function addAgendaTopic(): Promise<void> {
    if (!event || !doc || !newTopic.trim() || busy) return;
    setBusy(true);
    try {
      const next: AgendaItem[] = [
        ...doc.agenda,
        {
          id: `ag-${Date.now()}`,
          text: newTopic.trim(),
          checked: false,
          addedBy: app.staffWorkspace.staffHandle || "staff",
          promotedActionId: null,
        },
      ];
      await app.mutateMeetingDoc({
        eventId: event.id,
        action: { kind: "updateAgenda", agenda: next },
      });
      setNewTopic("");
    } finally {
      setBusy(false);
    }
  }

  async function toggleAgendaItem(itemId: string): Promise<void> {
    if (!event || !doc || busy) return;
    setBusy(true);
    try {
      const next = doc.agenda.map((a) => (a.id === itemId ? { ...a, checked: !a.checked } : a));
      await app.mutateMeetingDoc({
        eventId: event.id,
        action: { kind: "updateAgenda", agenda: next },
      });
    } finally {
      setBusy(false);
    }
  }

  async function startNotesEdit(): Promise<void> {
    if (!event || busy) return;
    setBusy(true);
    try {
      await app.mutateMeetingDoc({
        eventId: event.id,
        action: { kind: "claimNotesLock" },
      });
      setEditingNotes(true);
    } finally {
      setBusy(false);
    }
  }

  async function saveNotes(): Promise<void> {
    if (!event || busy) return;
    setBusy(true);
    try {
      await app.mutateMeetingDoc({
        eventId: event.id,
        action: { kind: "saveNotes", notesMarkdown: notesDraft, releaseLock: true },
      });
      setEditingNotes(false);
    } finally {
      setBusy(false);
    }
  }

  async function promoteBullet(item: AgendaItem): Promise<void> {
    if (!event || busy) return;
    setBusy(true);
    try {
      await app.promoteAgendaToAction({
        eventId: event.id,
        agendaItemId: item.id,
        title: item.text,
        department: event.department,
        assigneeHandle: null,
        dueAt: null,
      });
    } finally {
      setBusy(false);
    }
  }

  async function addDirectAction(): Promise<void> {
    if (!event || !newActionTitle.trim() || busy) return;
    setBusy(true);
    try {
      await app.promoteAgendaToAction({
        eventId: event.id,
        agendaItemId: null,
        title: newActionTitle.trim(),
        department: event.department,
        assigneeHandle: newActionAssignee.trim() || null,
        dueAt: null,
      });
      setNewActionTitle("");
      setNewActionAssignee("");
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(): Promise<void> {
    if (!event || busy) return;
    setBusy(true);
    try {
      await app.removeCalendarEvent(event.id);
      onAfterMutate();
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="read-col calendar-read-col">
      <div className="read-card">
        <div className="read-toolbar">
          {app.layout === "classic" && (
            <button type="button" className="t-btn" onClick={onClearSelection}>
              <ChevronLeft />
              Calendar
            </button>
          )}
          <span className="badge badge-staff">{eventKindLabel(event.kind)}</span>
          <button
            type="button"
            className="t-btn"
            title="Edit event"
            onClick={() => onEditEvent(event)}
          >
            <Edit3 />
            Edit
          </button>
          <button
            type="button"
            className="t-btn"
            onClick={() => void app.copyDiscordRecap(event.id)}
          >
            <ClipboardCopy />
            Copy Recap
          </button>
          {app.discordWebhookConfigured ? (
            <button
              type="button"
              className="t-btn on"
              onClick={() => void app.postDiscordRecap(event.id)}
            >
              <Send />
              Post Recap
            </button>
          ) : (
            <button
              type="button"
              className="t-btn"
              onClick={onOpenOverlays}
              title="Configure optional Discord webhook in OS keychain"
            >
              Webhook…
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button
            type="button"
            className="t-btn"
            title="Delete event"
            disabled={busy}
            onClick={() => void handleDelete()}
          >
            <Trash2 />
          </button>
        </div>

        <div className="read-scroll">
          <div className="read-header">
            <div className="read-subject">{event.title}</div>
            <div className="meeting-doc-meta">
              <span className="meeting-doc-loc">
                <Clock style={{ width: 11, height: 11 }} />
                {when}
              </span>
              {event.department && <span className="dept-chip">#{event.department}</span>}
              {event.location && (
                <span className="meeting-doc-loc">
                  <MapPin style={{ width: 11, height: 11 }} />
                  {event.location}
                </span>
              )}
              {event.recurrence !== "none" && (
                <span className="meeting-doc-loc">
                  <Repeat style={{ width: 11, height: 11 }} />
                  {event.recurrence}
                </span>
              )}
            </div>
            {event.description && <p className="meeting-doc-desc reader-prose">{event.description}</p>}

            {event.discordUrl && (
              <button
                type="button"
                className="btn compact discord-join-btn"
                onClick={() => void app.launchPinnedLink(event.discordUrl!)}
              >
                <MessageSquare />
                Join in Discord
                <ExternalLink style={{ width: 11, height: 11 }} />
              </button>
            )}
          </div>

          <div className="meeting-doc-sections">
            {doc && doc.carryoverItems.length > 0 && (
              <div className="meeting-section carryover-box">
                <div className="meeting-section-title">
                  <Repeat style={{ width: 12, height: 12 }} />
                  Carried Over from Previous Meeting ({doc.carryoverItems.length})
                </div>
                {doc.carryoverItems.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    className="meeting-action-row"
                    onClick={() =>
                      void app.setActionItemStatus(
                        item.id,
                        item.status === "done" ? "open" : "done",
                      )
                    }
                  >
                    {item.status === "done" ? (
                      <CheckCircle2 style={{ width: 14, height: 14, color: "var(--app-shield)" }} />
                    ) : (
                      <Circle style={{ width: 14, height: 14, opacity: 0.65 }} />
                    )}
                    <span className="meeting-action-title">{item.title}</span>
                    {item.assigneeHandle && <span className="kbd">@{item.assigneeHandle}</span>}
                  </button>
                ))}
              </div>
            )}

            <div className="meeting-section">
              <div className="meeting-section-title">Agenda</div>
              {doc && doc.agenda.length === 0 && (
                <span className="hint-text">No agenda topics yet — add one below.</span>
              )}
              {doc?.agenda.map((item) => (
                <div key={item.id} className="agenda-row">
                  <button
                    type="button"
                    className="agenda-check"
                    onClick={() => void toggleAgendaItem(item.id)}
                  >
                    {item.checked ? (
                      <CheckCircle2 style={{ width: 14, height: 14, color: "var(--app-shield)" }} />
                    ) : (
                      <Circle style={{ width: 14, height: 14, opacity: 0.65 }} />
                    )}
                    <span className={item.checked ? "done" : ""}>{item.text}</span>
                  </button>
                  {item.promotedActionId ? (
                    <span className="kbd" title="Promoted to shared Action Items">
                      Action
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="sidebar-mini-btn"
                      title="Promote to shared Action Item"
                      disabled={busy}
                      onClick={() => void promoteBullet(item)}
                    >
                      <ArrowUpRight style={{ width: 11, height: 11 }} />
                    </button>
                  )}
                </div>
              ))}
              <div className="agenda-add-row">
                <input
                  className="input"
                  placeholder="Add agenda topic…"
                  value={newTopic}
                  onChange={(e) => setNewTopic(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void addAgendaTopic();
                    }
                  }}
                />
                <button
                  type="button"
                  className="btn compact"
                  disabled={!newTopic.trim() || busy}
                  onClick={() => void addAgendaTopic()}
                >
                  <Plus />
                </button>
              </div>
            </div>

            <div className="meeting-section">
              <div className="meeting-section-head">
                <span className="meeting-section-title">Shared Notes (Markdown)</span>
                {doc?.notesEditorName && !editingNotes && (
                  <span className="kbd">
                    <Lock style={{ width: 10, height: 10 }} /> Editing: {doc.notesEditorName}
                  </span>
                )}
                {!editingNotes ? (
                  <button
                    type="button"
                    className="sidebar-mini-btn"
                    disabled={busy}
                    onClick={() => void startNotesEdit()}
                  >
                    Edit Notes
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn compact primary"
                    disabled={busy}
                    onClick={() => void saveNotes()}
                  >
                    Save &amp; Unlock
                  </button>
                )}
              </div>
              {editingNotes ? (
                <textarea
                  className="textarea meeting-notes-editor reader-prose"
                  rows={8}
                  value={notesDraft}
                  onChange={(e) => setNotesDraft(e.target.value)}
                  placeholder="Write shared Markdown meeting notes…"
                />
              ) : (
                <pre className="meeting-notes-preview reader-prose">
                  {doc?.notesMarkdown.trim() || "No notes yet. Click Edit Notes to start."}
                </pre>
              )}
            </div>

            <div className="meeting-section">
              <div className="meeting-section-title">
                <ListTodo style={{ width: 12, height: 12 }} />
                Action Items ({doc?.actionItems.length ?? 0})
              </div>
              {doc?.actionItems.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="meeting-action-row"
                  onClick={() =>
                    void app.setActionItemStatus(
                      item.id,
                      item.status === "done" ? "open" : "done",
                    )
                  }
                >
                  {item.status === "done" ? (
                    <CheckCircle2 style={{ width: 14, height: 14, color: "var(--app-shield)" }} />
                  ) : (
                    <Circle style={{ width: 14, height: 14, opacity: 0.65 }} />
                  )}
                  <span className={`meeting-action-title${item.status === "done" ? " done" : ""}`}>
                    {item.title}
                  </span>
                  {item.assigneeHandle && <span className="kbd">@{item.assigneeHandle}</span>}
                </button>
              ))}
              <div className="agenda-add-row">
                <input
                  className="input"
                  placeholder="New action item…"
                  value={newActionTitle}
                  onChange={(e) => setNewActionTitle(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void addDirectAction();
                    }
                  }}
                />
                <input
                  className="input"
                  style={{ maxWidth: 96 }}
                  placeholder="@handle"
                  value={newActionAssignee}
                  onChange={(e) => setNewActionAssignee(e.target.value)}
                />
                <button
                  type="button"
                  className="btn compact"
                  disabled={!newActionTitle.trim() || busy}
                  onClick={() => void addDirectAction()}
                >
                  <Plus />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}
