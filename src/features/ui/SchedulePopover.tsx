import { useMemo, useState } from "react";
import type { MailApp } from "../shell/useMailApp";

interface Preset {
  label: string;
  at: number;
}

function atHour(dayOffset: number, hour: number): number {
  const at = new Date();
  at.setDate(at.getDate() + dayOffset);
  at.setHours(hour, 0, 0, 0);
  return at.getTime();
}

function laterToday(): number {
  const fivePm = atHour(0, 17);
  if (fivePm > Date.now()) return fivePm;
  return Date.now() + 2 * 3600_000;
}

function nextMonday9(): number {
  const at = atHour(0, 9);
  const day = new Date(at).getDay();
  const delta = (1 - day + 7) % 7 || 7;
  return at + delta * 86_400_000;
}

const TITLES = {
  snooze: "Snooze until",
  followUp: "Follow up if no reply by",
  sendLater: "Send later at",
} as const;

export function SchedulePopover({ app }: { app: MailApp }) {
  const popover = app.popover;
  const [custom, setCustom] = useState("");

  const presets = useMemo<Preset[]>(
    () => [
      { label: "Later today", at: laterToday() },
      { label: "Tomorrow 9 AM", at: atHour(1, 9) },
      { label: "Next Monday", at: nextMonday9() },
      { label: "In 3 days", at: atHour(3, 9) },
    ],
    [],
  );

  const position = useMemo(() => {
    if (!popover) return { left: 0, top: 0 };
    const width = 236;
    const estimatedHeight = 280;
    const left = Math.max(8, Math.min(popover.anchor.x, window.innerWidth - width - 12));
    const below = popover.anchor.y;
    const top =
      below + estimatedHeight > window.innerHeight - 8
        ? Math.max(8, below - estimatedHeight - 130)
        : below;
    return { left, top };
  }, [popover]);

  if (!popover) return null;

  const title = TITLES[popover.mode];

  return (
    <>
      <div
        className="cmd-backdrop"
        style={{ background: "transparent", zIndex: 59, paddingTop: 0 }}
        onMouseDown={app.closePopover}
      />
      <div className="sched-popover" style={{ left: position.left, top: position.top }}>
        <span className="sched-title">{title}</span>
        {presets.map((preset) => (
          <button
            key={preset.label}
            className="sched-opt"
            onClick={() => void app.pickPopoverTime(preset.at)}
          >
            {preset.label}
            <span className="when">
              {new Intl.DateTimeFormat(undefined, {
                weekday: "short",
                hour: "numeric",
                minute: "2-digit",
              }).format(preset.at)}
            </span>
          </button>
        ))}
        <div className="sched-custom">
          <input
            className="input"
            type="datetime-local"
            value={custom}
            onChange={(event) => setCustom(event.target.value)}
          />
          <button
            className="btn"
            disabled={custom.length === 0}
            onClick={() => {
              const at = new Date(custom).getTime();
              if (Number.isFinite(at)) void app.pickPopoverTime(at);
            }}
          >
            Go
          </button>
        </div>
      </div>
    </>
  );
}
