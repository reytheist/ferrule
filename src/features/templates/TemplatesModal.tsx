import { useState } from "react";
import { Pencil, Trash2, Users, X } from "lucide-react";
import type {
  SharedTemplateId,
  SharedTemplateView,
  TemplateId,
  TemplateView,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

type Tab = "personal" | "shared";

export function TemplatesModal({ app }: { app: MailApp }) {
  const [tab, setTab] = useState<Tab>("shared");
  const [editingPersonalId, setEditingPersonalId] = useState<TemplateId | null>(null);
  const [editingSharedId, setEditingSharedId] = useState<SharedTemplateId | null>(null);
  const [name, setName] = useState("");
  const [shortcut, setShortcut] = useState("");
  const [category, setCategory] = useState("sponsors");
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);

  if (!app.templatesOpen) return null;

  function resetForm(): void {
    setEditingPersonalId(null);
    setEditingSharedId(null);
    setName("");
    setShortcut("");
    setCategory("sponsors");
    setSubject("");
    setBody("");
    setError(null);
  }

  function switchTab(next: Tab): void {
    setTab(next);
    resetForm();
  }

  function loadPersonal(template: TemplateView): void {
    setTab("personal");
    setEditingPersonalId(template.id);
    setEditingSharedId(null);
    setName(template.name);
    setShortcut(template.shortcut ?? "");
    setSubject(template.subject);
    setBody(template.bodyText);
    setError(null);
  }

  function loadShared(template: SharedTemplateView): void {
    setTab("shared");
    setEditingSharedId(template.id);
    setEditingPersonalId(null);
    setName(template.name);
    setShortcut(template.shortcut ?? "");
    setCategory(template.category);
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
    if (tab === "shared") {
      await app.persistSharedTemplate({
        id: editingSharedId,
        name: name.trim(),
        shortcut: shortcut.trim() || null,
        category: category.trim() || "general",
        subject,
        bodyText: body,
      });
    } else {
      await app.persistTemplate({
        id: editingPersonalId,
        name: name.trim(),
        shortcut: shortcut.trim() || null,
        subject,
        bodyText: body,
      });
    }
    resetForm();
  }

  const isEditing = tab === "shared" ? editingSharedId !== null : editingPersonalId !== null;

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
          <div className="segmented-row">
            <button
              type="button"
              className={`btn compact${tab === "shared" ? " primary" : ""}`}
              onClick={() => switchTab("shared")}
            >
              <Users style={{ width: 12, height: 12 }} />
              Shared Staff Templates ({app.sharedTemplates.length})
            </button>
            <button
              type="button"
              className={`btn compact${tab === "personal" ? " primary" : ""}`}
              onClick={() => switchTab("personal")}
            >
              Personal Snippets ({app.templates.length})
            </button>
          </div>

          <div className="tpl-list">
            {tab === "shared" ? (
              app.sharedTemplates.length === 0 ? (
                <span className="hint-text">
                  No shared staff templates yet — create team-wide responses below.
                </span>
              ) : (
                app.sharedTemplates.map((template) => (
                  <div key={template.id} className="tpl-row shared-tpl-row">
                    <div className="tr-main">
                      <span className="tr-name">
                        <span className="staff-tpl-dot" />
                        {template.name}
                        <span className="badge badge-staff">{template.category}</span>
                      </span>
                      <span className="tr-preview">
                        {template.bodyText.split("\n")[0]} · updated by @{template.updatedBy}
                      </span>
                    </div>
                    {template.shortcut && <span className="kbd">{template.shortcut}</span>}
                    <button
                      className="icon-btn"
                      title="Edit"
                      onClick={() => loadShared(template)}
                    >
                      <Pencil />
                    </button>
                    <button
                      className="icon-btn"
                      title="Delete"
                      onClick={() => void app.removeSharedTemplate(template.id)}
                    >
                      <Trash2 />
                    </button>
                  </div>
                ))
              )
            ) : app.templates.length === 0 ? (
              <span className="hint-text">No personal snippets yet — create the first one below.</span>
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
                    onClick={() => loadPersonal(template)}
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
                placeholder={
                  tab === "shared" ? "Sponsor Prospectus Reply" : "Quick status check"
                }
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </div>
            {tab === "shared" && (
              <div className="field" style={{ maxWidth: 140 }}>
                <label>Category</label>
                <input
                  className="input"
                  placeholder="sponsors"
                  value={category}
                  onChange={(event) => setCategory(event.target.value)}
                />
              </div>
            )}
            <div className="field" style={{ maxWidth: 110 }}>
              <label>Shortcut</label>
              <input
                className="input"
                placeholder={tab === "shared" ? "/sponsor" : "/sc"}
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
          {isEditing && (
            <button className="btn" onClick={resetForm}>
              New {tab === "shared" ? "shared template" : "snippet"}
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={app.closeTemplates}>
            Done
          </button>
          <button className="btn primary" onClick={() => void save()}>
            {isEditing
              ? tab === "shared"
                ? "Update shared template"
                : "Update snippet"
              : tab === "shared"
                ? "Create shared template"
                : "Create snippet"}
          </button>
        </div>
      </div>
    </div>
  );
}
