import { useState } from "react";
import { Pencil, Trash2, X } from "lucide-react";
import type { TemplateId, TemplateView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

export function TemplatesModal({ app }: { app: MailApp }) {
  const [editingId, setEditingId] = useState<TemplateId | null>(null);
  const [name, setName] = useState("");
  const [shortcut, setShortcut] = useState("");
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);

  if (!app.templatesOpen) return null;

  function resetForm(): void {
    setEditingId(null);
    setName("");
    setShortcut("");
    setSubject("");
    setBody("");
    setError(null);
  }

  function loadTemplate(template: TemplateView): void {
    setEditingId(template.id);
    setName(template.name);
    setShortcut(template.shortcut ?? "");
    setSubject(template.subject);
    setBody(template.bodyText);
    setError(null);
  }

  async function save(): Promise<void> {
    if (name.trim().length === 0) {
      setError("Give the template a name.");
      return;
    }
    setError(null);
    await app.persistTemplate({
      id: editingId,
      name: name.trim(),
      shortcut: shortcut.trim() || null,
      subject,
      bodyText: body,
    });
    resetForm();
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeTemplates();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Templates">
        <div className="modal-head">
          <h2>Templates &amp; snippets</h2>
          <button className="icon-btn" onClick={app.closeTemplates} title="Close (Esc)">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="tpl-list">
            {app.templates.length === 0 ? (
              <span className="hint-text">No snippets yet — create the first one below.</span>
            ) : (
              app.templates.map((template) => (
                <div key={template.id} className="tpl-row">
                  <div className="tr-main">
                    <span className="tr-name">{template.name}</span>
                    <span className="tr-preview">{template.bodyText.split("\n")[0]}</span>
                  </div>
                  {template.shortcut && <span className="kbd">{template.shortcut}</span>}
                  <button
                    className="icon-btn"
                    title="Edit"
                    onClick={() => loadTemplate(template)}
                  >
                    <Pencil />
                  </button>
                  <button
                    className="icon-btn"
                    title="Delete"
                    onClick={() => void app.removeTemplate(template.id)}
                  >
                    <Trash2 />
                  </button>
                </div>
              ))
            )}
          </div>

          <div className="field-row">
            <div className="field">
              <label>Name</label>
              <input
                className="input"
                placeholder="Quick status check"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </div>
            <div className="field" style={{ maxWidth: 110 }}>
              <label>Shortcut</label>
              <input
                className="input"
                placeholder="/sc"
                value={shortcut}
                onChange={(event) => setShortcut(event.target.value)}
              />
            </div>
          </div>
          <div className="field">
            <label>Subject</label>
            <input
              className="input"
              placeholder="Optional default subject"
              value={subject}
              onChange={(event) => setSubject(event.target.value)}
            />
          </div>
          <div className="field">
            <label>Body</label>
            <textarea
              className="textarea"
              placeholder="Template body — inserted where the cursor is."
              value={body}
              onChange={(event) => setBody(event.target.value)}
            />
          </div>
          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          {editingId !== null && (
            <button className="btn" onClick={resetForm}>
              New snippet
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={app.closeTemplates}>
            Done
          </button>
          <button className="btn primary" onClick={() => void save()}>
            {editingId !== null ? "Update snippet" : "Create snippet"}
          </button>
        </div>
      </div>
    </div>
  );
}
