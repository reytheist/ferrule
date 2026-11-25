import { useState } from "react";
import { ExternalLink, Link2, Pencil, Trash2, X } from "lucide-react";
import type { PinnedLinkId, PinnedLinkView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export function PinnedLinksModal({ app }: { app: MailApp }) {
  const [editingId, setEditingId] = useState<PinnedLinkId | null>(null);
  const [title, setTitle] = useState("");
  const [url, setUrl] = useState("");
  const [category, setCategory] = useState("operations");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);

  if (!app.pinnedLinksOpen) return null;

  function resetForm(): void {
    setEditingId(null);
    setTitle("");
    setUrl("");
    setCategory("operations");
    setDescription("");
    setError(null);
  }

  function loadForEdit(link: PinnedLinkView): void {
    setEditingId(link.id);
    setTitle(link.title);
    setUrl(link.url);
    setCategory(link.category);
    setDescription(link.description ?? "");
    setError(null);
  }

  async function save(): Promise<void> {
    if (!title.trim()) {
      setError("Give the link a title.");
      return;
    }
    if (!/^(https?:|discord:)\/\//i.test(url.trim())) {
      setError("URL must start with https://, http://, or discord://.");
      return;
    }
    setError(null);
    await app.persistPinnedLink({
      id: editingId,
      title: title.trim(),
      url: url.trim(),
      category: category.trim() || "operations",
      description: description.trim() || null,
    });
    resetForm();
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closePinnedLinks();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Pinned Staff Links">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Link2 style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Pinned Staff Links</h2>
          </div>
          <button className="icon-btn" onClick={app.closePinnedLinks} title="Close (Esc)">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="tpl-list">
            {app.pinnedLinks.length === 0 ? (
              <span className="hint-text">
                Pin frequently used Pigment docs (Operations Workbook, Admin Portal, Canva, Discord).
              </span>
            ) : (
              app.pinnedLinks.map((link) => (
                <div key={link.id} className="tpl-row">
                  <div className="tr-main">
                    <span className="tr-name">
                      {link.title}
                      <span className="kbd">{link.category}</span>
                    </span>
                    <span className="tr-preview">{link.description ?? link.url}</span>
                  </div>
                  <button
                    className="icon-btn"
                    title="Open link"
                    onClick={() => void app.launchPinnedLink(link.url)}
                  >
                    <ExternalLink />
                  </button>
                  <button className="icon-btn" title="Edit" onClick={() => loadForEdit(link)}>
                    <Pencil />
                  </button>
                  <button
                    className="icon-btn"
                    title="Delete"
                    onClick={() => void app.removePinnedLink(link.id)}
                  >
                    <Trash2 />
                  </button>
                </div>
              ))
            )}
          </div>

          <div className="field-row">
            <div className="field">
              <label>Title</label>
              <input
                className="input"
                placeholder="2027 Operations Workbook"
                value={title}
                onChange={(event) => setTitle(event.target.value)}
              />
            </div>
            <div className="field" style={{ maxWidth: 140 }}>
              <label>Category</label>
              <input
                className="input"
                placeholder="operations"
                value={category}
                onChange={(event) => setCategory(event.target.value)}
              />
            </div>
          </div>
          <div className="field">
            <label>URL</label>
            <input
              className="input"
              placeholder="https://docs.google.com/spreadsheets/…"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
            />
          </div>
          <div className="field">
            <label>Description (optional)</label>
            <input
              className="input"
              placeholder="Floor plan, vendor map, and room supplies"
              value={description}
              onChange={(event) => setDescription(event.target.value)}
            />
          </div>
          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          {editingId !== null && (
            <button className="btn" onClick={resetForm}>
              New link
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={app.closePinnedLinks}>
            Done
          </button>
          <button className="btn primary" onClick={() => void save()}>
            {editingId !== null ? "Update link" : "Pin link"}
          </button>
        </div>
      </div>
    </div>
  );
}
