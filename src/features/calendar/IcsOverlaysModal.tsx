import { useState } from "react";
import { Layers, Plus, RefreshCw, Shield, Trash2, X } from "lucide-react";
import type { MailApp } from "../shell/useMailApp";

export function IcsOverlaysModal({
  app,
  onClose,
}: {
  app: MailApp;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function addOverlay(): Promise<void> {
    if (!name.trim() || !url.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const saved = await app.persistIcsOverlay({
        id: null,
        name: name.trim(),
        url: url.trim(),
        color: "violet",
        enabled: true,
      });
      await app.syncIcsOverlay(saved.id);
      setName("");
      setUrl("");
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
      <div className="modal wide" role="dialog" aria-label="Personal Calendar Overlays">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Layers style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Personal .ics Overlays</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <section className="settings-section">
            <div className="settings-section-title">
              <Shield style={{ width: 12, height: 12 }} />
              Read-Only Personal .ics Overlays (Local Only)
            </div>
            <span className="hint-text">
              Paste private read-only .ics URLs from Google Calendar, Apple iCloud, or Outlook.
              Feed URLs and busy blocks stay 100% local in your SQLite database.
            </span>

            {app.icsOverlays.length > 0 && (
              <div className="overlay-list">
                {app.icsOverlays.map((ov) => (
                  <div key={ov.id} className="overlay-row">
                    <input
                      type="checkbox"
                      checked={ov.enabled}
                      onChange={() =>
                        void app.persistIcsOverlay({
                          id: ov.id,
                          name: ov.name,
                          url: ov.url,
                          color: ov.color,
                          enabled: !ov.enabled,
                        })
                      }
                    />
                    <div className="overlay-info">
                      <strong>{ov.name}</strong>
                      <span className="hint-text">
                        {ov.eventCount} blocks · {ov.url}
                      </span>
                      {ov.lastError && <span className="error-text">{ov.lastError}</span>}
                    </div>
                    <button
                      type="button"
                      className="icon-btn"
                      title="Refresh .ics feed"
                      disabled={busy}
                      onClick={() => void app.syncIcsOverlay(ov.id)}
                    >
                      <RefreshCw />
                    </button>
                    <button
                      type="button"
                      className="icon-btn"
                      title="Remove overlay"
                      disabled={busy}
                      onClick={() => void app.removeIcsOverlay(ov.id)}
                    >
                      <Trash2 />
                    </button>
                  </div>
                ))}
              </div>
            )}

            <div className="field-row">
              <div className="field" style={{ maxWidth: 180 }}>
                <label>Label</label>
                <input
                  className="input"
                  placeholder="Personal Google"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </div>
              <div className="field">
                <label>Private .ics URL</label>
                <input
                  className="input"
                  placeholder="https://calendar.google.com/calendar/ical/.../basic.ics"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                />
              </div>
              <button
                type="button"
                className="btn compact primary"
                style={{ alignSelf: "flex-end" }}
                disabled={!name.trim() || !url.trim() || busy}
                onClick={() => void addOverlay()}
              >
                <Plus />
                Add Feed
              </button>
            </div>
          </section>

          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          <span style={{ flex: 1 }} />
          <button type="button" className="btn primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
