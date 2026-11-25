import { useMemo, useState } from "react";
import { Contact, Download, FileSpreadsheet, Pencil, Search, Trash2, X } from "lucide-react";
import type { ContactCategory, ContactId, ShowContactView } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

const CATEGORIES: Array<{ value: ContactCategory | "all"; label: string }> = [
  { value: "all", label: "All" },
  { value: "sponsor", label: "Sponsors" },
  { value: "vendor", label: "Vendors" },
  { value: "instructor", label: "Instructors" },
  { value: "vip", label: "VIPs" },
  { value: "volunteer", label: "Volunteers" },
  { value: "venue", label: "Venue" },
  { value: "other", label: "Other" },
];

export function ContactsModal({ app }: { app: MailApp }) {
  const [filterCat, setFilterCat] = useState<ContactCategory | "all">("all");
  const [query, setQuery] = useState("");
  const [editingId, setEditingId] = useState<ContactId | null>(null);
  const [name, setName] = useState("");
  const [emails, setEmails] = useState("");
  const [category, setCategory] = useState<ContactCategory>("sponsor");
  const [organization, setOrganization] = useState("");
  const [tier, setTier] = useState("");
  const [notes, setNotes] = useState("");
  const [error, setError] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return app.showContacts.filter((c) => {
      if (filterCat !== "all" && c.category !== filterCat) return false;
      if (!q) return true;
      return (
        c.name.toLowerCase().includes(q) ||
        c.emails.some((e) => e.toLowerCase().includes(q)) ||
        (c.organization?.toLowerCase().includes(q) ?? false) ||
        (c.tier?.toLowerCase().includes(q) ?? false)
      );
    });
  }, [app.showContacts, filterCat, query]);

  if (!app.contactsOpen) return null;

  function resetForm(): void {
    setEditingId(null);
    setName("");
    setEmails("");
    setCategory("sponsor");
    setOrganization("");
    setTier("");
    setNotes("");
    setError(null);
  }

  function loadForEdit(contact: ShowContactView): void {
    setEditingId(contact.id);
    setName(contact.name);
    setEmails(contact.emails.join(", "));
    setCategory(contact.category);
    setOrganization(contact.organization ?? "");
    setTier(contact.tier ?? "");
    setNotes(contact.notes);
    setError(null);
  }

  async function save(): Promise<void> {
    const parsedEmails = emails
      .split(/[,\s;]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (!name.trim()) {
      setError("Contact name is required.");
      return;
    }
    if (parsedEmails.length === 0) {
      setError("Add at least one email address.");
      return;
    }
    setError(null);
    await app.persistShowContact({
      id: editingId,
      name: name.trim(),
      emails: parsedEmails,
      category,
      organization: organization.trim() || null,
      tier: tier.trim() || null,
      notes: notes.trim(),
    });
    resetForm();
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeContacts();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Shared Show Contacts">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Contact style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Shared Show Contacts</h2>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <button
              type="button"
              className="btn compact"
              onClick={() => {
                app.closeContacts();
                app.openWorkbookImport("contacts");
              }}
              title="Import contacts from .xlsx or CSV"
            >
              <FileSpreadsheet style={{ width: 12, height: 12 }} />
              Import
            </button>
            <button
              type="button"
              className="btn compact"
              onClick={() => void app.downloadShowContactsCsv()}
              title="Export contacts to CSV"
            >
              <Download style={{ width: 12, height: 12 }} />
              Export CSV
            </button>
            <button className="icon-btn" onClick={app.closeContacts} title="Close (Esc)">
              <X />
            </button>
          </div>
        </div>

        <div className="modal-body">
          <div className="contacts-filter-bar">
            <div className="search-inline">
              <Search style={{ width: 13, height: 13 }} />
              <input
                className="input compact-input"
                placeholder="Search sponsors, vendors, instructors, venue…"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <div className="cat-pill-row">
              {CATEGORIES.map((cat) => (
                <button
                  key={cat.value}
                  type="button"
                  className={`cat-filter-btn${filterCat === cat.value ? " active" : ""}`}
                  onClick={() => setFilterCat(cat.value)}
                >
                  {cat.label}
                </button>
              ))}
            </div>
          </div>

          <div className="tpl-list">
            {filtered.length === 0 ? (
              <span className="hint-text">No matching show contacts yet.</span>
            ) : (
              filtered.map((contact) => (
                <div key={contact.id} className="tpl-row">
                  <div className="tr-main">
                    <span className="tr-name">
                      {contact.name}
                      <span className={`contact-cat-pill cat-${contact.category}`}>
                        {contact.category}
                      </span>
                      {contact.tier && <span className="kbd">{contact.tier}</span>}
                    </span>
                    <span className="tr-preview">
                      {contact.organization ? `${contact.organization} · ` : ""}
                      {contact.emails.join(", ")}
                    </span>
                  </div>
                  <button className="icon-btn" title="Edit" onClick={() => loadForEdit(contact)}>
                    <Pencil />
                  </button>
                  <button
                    className="icon-btn"
                    title="Delete"
                    onClick={() => void app.removeShowContact(contact.id)}
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
                placeholder="Dana Okafor"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </div>
            <div className="field" style={{ maxWidth: 150 }}>
              <label>Category</label>
              <select
                className="select"
                value={category}
                onChange={(event) => setCategory(event.target.value as ContactCategory)}
              >
                <option value="sponsor">Sponsor</option>
                <option value="vendor">Vendor</option>
                <option value="instructor">Instructor</option>
                <option value="vip">VIP</option>
                <option value="volunteer">Volunteer</option>
                <option value="venue">Venue</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>
          <div className="field">
            <label>Email(s)</label>
            <input
              className="input"
              placeholder="dana@ferngate.co"
              value={emails}
              onChange={(event) => setEmails(event.target.value)}
            />
          </div>
          <div className="field-row">
            <div className="field">
              <label>Organization</label>
              <input
                className="input"
                placeholder="Ferngate Miniatures"
                value={organization}
                onChange={(event) => setOrganization(event.target.value)}
              />
            </div>
            <div className="field">
              <label>Tier / Role</label>
              <input
                className="input"
                placeholder="Gold Sponsor, Booth #12, Keynote…"
                value={tier}
                onChange={(event) => setTier(event.target.value)}
              />
            </div>
          </div>
          <div className="field">
            <label>Internal staff notes</label>
            <textarea
              className="textarea"
              rows={2}
              placeholder="Booth preferences, A/V requirements, invoice status…"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
            />
          </div>
          {error && <span className="error-text">{error}</span>}
        </div>

        <div className="modal-foot">
          {editingId !== null && (
            <button className="btn" onClick={resetForm}>
              New contact
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={app.closeContacts}>
            Done
          </button>
          <button className="btn primary" onClick={() => void save()}>
            {editingId !== null ? "Update contact" : "Add contact"}
          </button>
        </div>
      </div>
    </div>
  );
}
