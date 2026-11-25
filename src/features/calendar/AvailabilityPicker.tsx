import { useEffect, useState } from "react";
import { CalendarClock, Check, X } from "lucide-react";
import { findAvailabilitySlots } from "../../ipc/core";
import type { AvailabilitySlot } from "../../types/mail";

export function AvailabilityPicker({
  onInsert,
  onClose,
}: {
  onInsert: (formattedText: string) => void;
  onClose: () => void;
}) {
  const [slotMinutes, setSlotMinutes] = useState(60);
  const [slots, setSlots] = useState<AvailabilitySlot[]>([]);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    const now = Date.now();
    const rangeEnd = now + 5 * 86_400_000;
    void findAvailabilitySlots({
      rangeStart: now + 1_800_000,
      rangeEnd,
      slotMinutes,
      workdayStartHour: 9,
      workdayEndHour: 18,
    })
      .then((res) => {
        if (!cancelled) {
          setSlots(res);
          setSelected(new Set(res.slice(0, 3).map((s) => `${s.startAt}-${s.endAt}`)));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slotMinutes]);

  function toggleSlot(slot: AvailabilitySlot): void {
    const key = `${slot.startAt}-${slot.endAt}`;
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function insertSelected(): void {
    const picked = slots.filter((s) => selected.has(`${s.startAt}-${s.endAt}`));
    if (picked.length === 0) return;
    const text = [
      "Here are a few times that work on my end:",
      ...picked.map((s) => `• ${s.label}`),
    ].join("\n");
    onInsert(text);
    onClose();
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-label="Insert Availability Slots">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <CalendarClock style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Insert Availability</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="field-row" style={{ alignItems: "center" }}>
            <span className="hint-text" style={{ flex: 1 }}>
              Open windows across Pigment Calendar &amp; enabled personal .ics overlays:
            </span>
            <select
              className="select"
              style={{ width: 110 }}
              value={slotMinutes}
              onChange={(e) => setSlotMinutes(Number(e.target.value))}
            >
              <option value={30}>30 min</option>
              <option value={60}>60 min</option>
              <option value={90}>90 min</option>
            </select>
          </div>

          {loading ? (
            <span className="hint-text">Scanning calendar…</span>
          ) : slots.length === 0 ? (
            <span className="hint-text">No open windows found in the next 5 days.</span>
          ) : (
            <div className="avail-slot-list">
              {slots.map((slot) => {
                const key = `${slot.startAt}-${slot.endAt}`;
                const active = selected.has(key);
                return (
                  <button
                    key={key}
                    type="button"
                    className={`avail-slot-item${active ? " active" : ""}`}
                    onClick={() => toggleSlot(slot)}
                  >
                    <span className="avail-check">{active && <Check style={{ width: 11, height: 11 }} />}</span>
                    <span>{slot.label}</span>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="modal-foot">
          <span className="hint-text" style={{ flex: 1 }}>
            {selected.size} slot{selected.size === 1 ? "" : "s"} selected
          </span>
          <button type="button" className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={selected.size === 0}
            onClick={insertSelected}
          >
            Insert into Email
          </button>
        </div>
      </div>
    </div>
  );
}
