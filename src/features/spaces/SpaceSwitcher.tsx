import { useState } from "react";
import { Pencil, Trash2, X } from "lucide-react";
import type { AccountView, SpaceColor } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export const SPACE_COLORS: SpaceColor[] = [
  "azure",
  "berry",
  "teal",
  "amber",
  "violet",
  "emerald",
];

const SPACE_CLASSES: Record<SpaceColor, string> = {
  azure: "acct-azure",
  berry: "acct-berry",
  teal: "acct-teal",
  amber: "acct-amber",
  violet: "acct-violet",
  emerald: "acct-emerald",
};

export function spaceClass(color: SpaceColor): string {
  return SPACE_CLASSES[color];
}

export function SpaceSwitcher({ app }: { app: MailApp }) {
  const [editing, setEditing] = useState<AccountView | null>(null);
  const [name, setName] = useState("");
  const [color, setColor] = useState<SpaceColor>("azure");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const totalUnread = app.accounts.reduce((sum, a) => sum + a.unread, 0);

  function openEditor(account: AccountView): void {
    setEditing(account);
    setName(account.displayName);
    setColor(account.color);
    setError(null);
  }

  function closeEditor(): void {
    if (busy) return;
    setEditing(null);
    setError(null);
  }

  async function saveEdit(): Promise<void> {
    if (!editing) return;
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Enter a space name.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await app.updateAccount({
        accountId: editing.id,
        displayName: trimmed,
        color,
      });
      setEditing(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not update space.");
    } finally {
      setBusy(false);
    }
  }

  async function removeEditingAccount(): Promise<void> {
    if (!editing) return;
    setBusy(true);
    try {
      await app.removeAccount(editing.id);
      setEditing(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <nav className="spaces-strip" aria-label="Account spaces">
        <button
          className={`space-chip all-spaces${app.scopeAccount === null ? " active" : ""}`}
          onClick={() => app.selectSpace(null)}
        >
          <span className="acct-dot" />
          All Spaces
          <span className="sp-cnt">{totalUnread}</span>
        </button>

        {app.accounts.map((account) => {
          const isActive = app.scopeAccount === account.id;
          return (
            <div
              key={account.id}
              className={`space-chip ${spaceClass(account.color)}${isActive ? " active" : ""}`}
            >
              <button
                type="button"
                className="space-chip-hit"
                onClick={() => app.selectSpace(account.id)}
                onDoubleClick={() => openEditor(account)}
                title={`${account.displayName} · ${account.email} (double-click to edit)`}
              >
                <span className="acct-dot" />
                <span>{account.displayName}</span>
                <span className="sp-cnt">{account.unread}</span>
              </button>
              {isActive && (
                <button
                  type="button"
                  className="space-chip-edit"
                  onClick={() => openEditor(account)}
                  title="Edit space name & color"
                  aria-label={`Edit ${account.displayName}`}
                >
                  <Pencil />
                </button>
              )}
            </div>
          );
        })}
      </nav>

      {editing && (
        <div
          className="modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeEditor();
          }}
        >
          <div className="modal" role="dialog" aria-label="Edit space">
            <div className="modal-head">
              <h2>Edit space</h2>
              <button className="icon-btn" disabled={busy} onClick={closeEditor} title="Close">
                <X />
              </button>
            </div>
            <div className="modal-body">
              <div className={`connected-pill ${spaceClass(color)}`}>
                <span className="acct-dot" />
                <span className="cp-email">{editing.email}</span>
              </div>
              <div className="field">
                <label>Space name</label>
                <input
                  className="input"
                  autoFocus
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && !busy) {
                      event.preventDefault();
                      void saveEdit();
                    }
                  }}
                />
              </div>
              <div className="field">
                <label>Space color</label>
                <div className="color-picker">
                  {SPACE_COLORS.map((entry) => (
                    <button
                      key={entry}
                      type="button"
                      className={`color-swatch ${spaceClass(entry)}${color === entry ? " selected" : ""}`}
                      title={entry}
                      onClick={() => setColor(entry)}
                    />
                  ))}
                </div>
              </div>
              {error && <span className="error-text">{error}</span>}
            </div>
            <div className="modal-foot">
              <button
                className="btn danger"
                disabled={busy}
                onClick={() => void removeEditingAccount()}
                style={{ marginRight: "auto" }}
              >
                <Trash2 style={{ width: 12, height: 12 }} />
                Remove account
              </button>
              <button className="btn" disabled={busy} onClick={closeEditor}>
                Cancel
              </button>
              <button className="btn primary" disabled={busy} onClick={() => void saveEdit()}>
                {busy ? "Saving…" : "Save"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
