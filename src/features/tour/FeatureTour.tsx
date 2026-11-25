import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, Sparkles, X } from "lucide-react";
import type { MailApp } from "../shell/useMailApp";

interface TourStop {
  id: string;
  selector: string;
  title: string;
  body: string;
  placement: "bottom" | "right" | "bottom-end";
}

interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

const POPOVER_WIDTH = 340;
const POPOVER_HEIGHT = 210;
const PAD = 6;
const GAP = 14;

export function FeatureTour({ app }: { app: MailApp }) {
  const [rect, setRect] = useState<Rect | null>(null);

  const stops: TourStop[] = [
    {
      id: "spaces",
      selector: '[data-tour="spaces"]',
      title: "Account Spaces",
      body: "Each email account lives in its own color-coded Account Space. Personal spaces keep Staff Sharing disabled by default, while Pigment Staff spaces lock the From address and unlock team collaboration.",
      placement: "bottom",
    },
    {
      id: "inbox",
      selector: '[data-tour="inbox"]',
      title: "Focused Inbox & Privacy",
      body: "Mail syncs locally into SQLite with full-text search and automatic Primary, Receipts, and Newsletters sorting. Tracking pixels and remote beacons are stripped before messages ever render.",
      placement: "right",
    },
    {
      id: "command-bar",
      selector: '[data-tour="command-bar"]',
      title: "Command Bar (Ctrl / Cmd + K)",
      body: "Jump between spaces, search every local message instantly, compose, switch themes, or replay this tour from anywhere with a single shortcut.",
      placement: "bottom",
    },
    {
      id: "schedule",
      selector: '[data-tour="schedule"]',
      title: "Snooze, Follow-ups & Send Later",
      body: "Ferrule runs a built-in local scheduler while resident in your system tray — snooze threads, set reply follow-up reminders, and schedule sends with Undo Send.",
      placement: "right",
    },
    {
      id: "staff",
      selector: '[data-tour="staff"]',
      title: "Pigment Staff Collaboration",
      body: app.staffWorkspace.connected
        ? "Coordinate on shared threads with internal @mentions, claim exclusive reply draft locks, and manage the shared show calendar, meeting docs, and action items."
        : "Coordinate on shared threads, reply draft locks, shared calendar, and action items. You haven't connected Pigment Staff Sync yet — you can connect anytime in Settings.",
      placement: "right",
    },
    {
      id: "settings",
      selector: '[data-tour="settings"]',
      title: "Settings & Replay",
      body: "Manage Account Spaces, appearance, Pigment Staff Sync, and Developer Tools here — or click Replay Feature Tour whenever you want a refresher.",
      placement: "bottom-end",
    },
  ];

  const total = stops.length;
  const index = Math.min(Math.max(app.tourStep, 0), total - 1);
  const current = stops[index];

  useEffect(() => {
    if (!app.tourOpen || !current) {
      setRect(null);
      return;
    }

    const measure = () => {
      const el = document.querySelector<HTMLElement>(current.selector);
      if (!el) {
        setRect(null);
        return;
      }
      const box = el.getBoundingClientRect();
      setRect({
        top: Math.max(4, box.top - PAD),
        left: Math.max(4, box.left - PAD),
        width: box.width + PAD * 2,
        height: box.height + PAD * 2,
      });
    };

    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [app.tourOpen, current]);

  if (!app.tourOpen || !current) return null;

  const vw = typeof window !== "undefined" ? window.innerWidth : 1280;
  const vh = typeof window !== "undefined" ? window.innerHeight : 820;

  let popoverTop = Math.max(16, (vh - POPOVER_HEIGHT) / 2);
  let popoverLeft = Math.max(16, (vw - POPOVER_WIDTH) / 2);

  if (rect) {
    if (current.placement === "right") {
      popoverLeft = Math.min(vw - POPOVER_WIDTH - 16, rect.left + rect.width + GAP);
      popoverTop = Math.min(vh - POPOVER_HEIGHT - 16, Math.max(16, rect.top));
    } else if (current.placement === "bottom-end") {
      popoverLeft = Math.max(16, Math.min(vw - POPOVER_WIDTH - 16, rect.left + rect.width - POPOVER_WIDTH));
      popoverTop = Math.min(vh - POPOVER_HEIGHT - 16, rect.top + rect.height + GAP);
    } else {
      popoverLeft = Math.max(16, Math.min(vw - POPOVER_WIDTH - 16, rect.left));
      popoverTop = Math.min(vh - POPOVER_HEIGHT - 16, rect.top + rect.height + GAP);
    }
  }

  const isLast = index === total - 1;

  return (
    <div className="tour-layer" role="dialog" aria-label={`Feature tour: ${current.title}`}>
      <div
        className={`tour-scrim${rect ? " has-cutout" : ""}`}
        onClick={() => void app.skipTour()}
      />

      {rect && (
        <div
          className="tour-spotlight"
          style={{
            top: rect.top,
            left: rect.left,
            width: rect.width,
            height: rect.height,
          }}
        />
      )}

      <div
        className="tour-popover"
        style={{
          top: popoverTop,
          left: popoverLeft,
          width: POPOVER_WIDTH,
        }}
      >
        <div className="tour-popover-head">
          <span className="tour-badge">
            <Sparkles size={12} />
            Step {index + 1} of {total}
          </span>
          <button
            type="button"
            className="icon-btn"
            onClick={() => void app.skipTour()}
            title="Skip tour (Esc)"
            aria-label="Skip tour"
          >
            <X />
          </button>
        </div>

        <h3 className="tour-title">{current.title}</h3>
        <p className="tour-body">{current.body}</p>

        <div className="tour-dots" aria-hidden="true">
          {stops.map((stop, i) => (
            <span key={stop.id} className={`tour-dot${i === index ? " active" : ""}`} />
          ))}
        </div>

        <div className="tour-actions">
          <button
            type="button"
            className="tour-skip-btn"
            onClick={() => void app.skipTour()}
          >
            Skip tour
          </button>
          <div className="tour-nav-btns">
            {index > 0 && (
              <button type="button" className="btn compact" onClick={app.prevTourStep}>
                <ArrowLeft size={13} />
                Back
              </button>
            )}
            <button
              type="button"
              className="btn compact primary"
              onClick={() => {
                if (isLast) {
                  void app.completeTour();
                } else {
                  app.nextTourStep();
                }
              }}
            >
              {isLast ? "Finish" : "Next"}
              {!isLast && <ArrowRight size={13} />}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
