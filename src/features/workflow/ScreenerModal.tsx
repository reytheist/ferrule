import { useState } from "react";
import { Ban, Check, Crown, Inbox, Newspaper, Receipt, ShieldCheck, Trash2, X } from "lucide-react";
import type { ScreenerDecision } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export function ScreenerModal({ app }: { app: MailApp }) {
  const [tab, setTab] = useState<"screener" | "vip">("screener");
  const [vipAddress, setVipAddress] = useState("");
  const [vipName, setVipName] = useState("");
  const [busy, setBusy] = useState(false);

  if (!app.screenerOpen) return null;

  async function decide(address: string, displayName: string, decision: ScreenerDecision) {
    if (busy) return;
    setBusy(true);
    try {
      await app.screenSender(address, displayName || null, decision);
    } finally {
      setBusy(false);
    }
  }

  async function addVip() {
    if (busy || !vipAddress.trim()) return;
    setBusy(true);
    try {
      await app.addVipSender(vipAddress.trim(), vipName.trim() || null);
      setVipAddress("");
      setVipName("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeScreener();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Sender Screener & VIPs">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <ShieldCheck style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Screener &amp; VIP Senders</h2>
          </div>
          <button className="icon-btn" onClick={app.closeScreener} title="Close (Esc)">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="segmented-control" role="tablist">
            <button
              type="button"
              className={`seg-btn${tab === "screener" ? " active" : ""}`}
              onClick={() => setTab("screener")}
            >
              First-Time Senders ({app.screenerSenders.length})
            </button>
            <button
              type="button"
              className={`seg-btn${tab === "vip" ? " active" : ""}`}
              onClick={() => setTab("vip")}
            >
              VIP Split Inbox ({app.vipSenders.length})
            </button>
          </div>

          {tab === "screener" ? (
            app.screenerSenders.length === 0 ? (
              <div className="empty-state" style={{ padding: "28px 16px" }}>
                <Check />
                <strong>All senders screened</strong>
                <span>New first-time senders will appear here for one-click routing.</span>
              </div>
            ) : (
              <div className="screener-list">
                {app.screenerSenders.map((sender) => (
                  <div key={sender.address} className="screener-card">
                    <div className="screener-card-info">
                      <strong>{sender.displayName || sender.address}</strong>
                      <span className="hint-text">
                        {sender.address} · {sender.threadCount}{" "}
                        {sender.threadCount === 1 ? "thread" : "threads"}
                      </span>
                      <span className="screener-subject">{sender.sampleSubject}</span>
                    </div>
                    <div className="screener-actions">
                      <button
                        type="button"
                        className="btn compact primary"
                        disabled={busy}
                        onClick={() => void decide(sender.address, sender.displayName, "inbox")}
                      >
                        <Inbox />
                        Inbox
                      </button>
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busy}
                        onClick={() =>
                          void decide(sender.address, sender.displayName, "paperTrail")
                        }
                      >
                        <Receipt />
                        Paper Trail
                      </button>
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busy}
                        onClick={() => void decide(sender.address, sender.displayName, "feed")}
                      >
                        <Newspaper />
                        Feed
                      </button>
                      <button
                        type="button"
                        className="btn compact"
                        disabled={busy}
                        onClick={() => void decide(sender.address, sender.displayName, "blocked")}
                      >
                        <Ban />
                        Block
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )
          ) : (
            <div className="signature-manager">
              <div className="signature-editor-box">
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    className="input"
                    placeholder="vip@example.com"
                    value={vipAddress}
                    onChange={(e) => setVipAddress(e.target.value)}
                  />
                  <input
                    className="input"
                    placeholder="Name (optional)"
                    value={vipName}
                    onChange={(e) => setVipName(e.target.value)}
                  />
                  <button
                    type="button"
                    className="btn compact primary"
                    disabled={busy || !vipAddress.trim()}
                    onClick={() => void addVip()}
                  >
                    <Crown />
                    Add VIP
                  </button>
                </div>
              </div>

              {app.vipSenders.length === 0 ? (
                <span className="hint-text">
                  VIP senders appear in a dedicated VIP Split Inbox section at the top of your
                  Inbox.
                </span>
              ) : (
                <div className="signature-list">
                  {app.vipSenders.map((vip) => (
                    <div key={vip.address} className="signature-item">
                      <Crown style={{ width: 13, height: 13, color: "var(--space-amber)" }} />
                      <div className="signature-item-main">
                        <strong>{vip.displayName ?? vip.address}</strong>
                        <span className="hint-text">{vip.address}</span>
                      </div>
                      <button
                        type="button"
                        className="icon-btn"
                        title="Remove VIP"
                        onClick={() => void app.removeVipSender(vip.address)}
                      >
                        <Trash2 />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        <div className="modal-foot">
          <span style={{ flex: 1 }} />
          <button type="button" className="btn primary" onClick={app.closeScreener}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
