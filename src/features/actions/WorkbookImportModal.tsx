import { useState } from "react";
import { AlertCircle, FileSpreadsheet, Upload, X } from "lucide-react";
import type { WorkbookImportReport, WorkbookImportTarget } from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const bytes = new Uint8Array(reader.result as ArrayBuffer);
      let binary = "";
      const chunk = 0x8000;
      for (let i = 0; i < bytes.length; i += chunk) {
        binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
      }
      resolve(btoa(binary));
    };
    reader.onerror = () => reject(new Error("Could not read file"));
    reader.readAsArrayBuffer(file);
  });
}

export function WorkbookImportModal({
  app,
  initialTarget = "auto",
  onClose,
}: {
  app: MailApp;
  initialTarget?: WorkbookImportTarget;
  onClose(): void;
}) {
  const [target, setTarget] = useState<WorkbookImportTarget>(initialTarget);
  const [syncToCalendar, setSyncToCalendar] = useState(true);
  const [filename, setFilename] = useState<string | null>(null);
  const [dataBase64, setDataBase64] = useState<string | null>(null);
  const [preview, setPreview] = useState<WorkbookImportReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function runPreview(
    name: string,
    b64: string,
    nextTarget: WorkbookImportTarget,
    nextSync: boolean,
  ): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const report = await app.importWorkbook({
        filename: name,
        dataBase64: b64,
        target: nextTarget,
        syncToCalendar: nextSync,
        dryRun: true,
      });
      setPreview(report);
    } catch (err) {
      setPreview(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleFileChange(file: File | undefined): Promise<void> {
    if (!file) return;
    setFilename(file.name);
    try {
      const b64 = await fileToBase64(file);
      setDataBase64(b64);
      await runPreview(file.name, b64, target, syncToCalendar);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function commitImport(): Promise<void> {
    if (!filename || !dataBase64) return;
    setBusy(true);
    setError(null);
    try {
      await app.importWorkbook({
        filename,
        dataBase64,
        target,
        syncToCalendar,
        dryRun: false,
      });
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  const totalActionChanges =
    (preview?.actionItemsCreated ?? 0) + (preview?.actionItemsUpdated ?? 0);
  const totalContactChanges =
    (preview?.contactsCreated ?? 0) + (preview?.contactsUpdated ?? 0);
  const hasImportableRows = totalActionChanges + totalContactChanges > 0;

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Import Operations Workbook">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <FileSpreadsheet style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Import Operations Workbook (.xlsx / CSV)</h2>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <p className="hint-text" style={{ margin: 0 }}>
            Import <strong>2027 Operations Workbook</strong> tasks into Action Items (and optionally
            Pigment Shared Calendar) and contact tabs (Sponsors / Vendors, Instructors, VIPs,
            Volunteers) into Shared Show Contacts. Dense visual sheets (Floor Plan, Vendor
            MapPayment, Room Supplies, Class Schedule) are automatically skipped so they stay in
            Google Drive via Pinned Staff Links.
          </p>

          <div className="field-row">
            <div className="field">
              <label>Workbook or CSV File</label>
              <label className="workbook-dropzone">
                <Upload style={{ width: 14, height: 14 }} />
                <span>{filename ?? "Choose .xlsx, .xls, or .csv file…"}</span>
                <input
                  type="file"
                  accept=".xlsx,.xls,.ods,.csv"
                  style={{ display: "none" }}
                  onChange={(e) => void handleFileChange(e.target.files?.[0])}
                />
              </label>
            </div>
            <div className="field" style={{ maxWidth: 200 }}>
              <label>Import Mode</label>
              <select
                className="select"
                value={target}
                onChange={(e) => {
                  const next = e.target.value as WorkbookImportTarget;
                  setTarget(next);
                  if (filename && dataBase64) {
                    void runPreview(filename, dataBase64, next, syncToCalendar);
                  }
                }}
              >
                <option value="auto">Auto-detect sheets</option>
                <option value="actionItems">Action Items only</option>
                <option value="contacts">Show Contacts only</option>
              </select>
            </div>
          </div>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={syncToCalendar}
              onChange={(e) => {
                const next = e.target.checked;
                setSyncToCalendar(next);
                if (filename && dataBase64) {
                  void runPreview(filename, dataBase64, target, next);
                }
              }}
            />
            <span>Create Pigment Shared Calendar milestones/room events for rows with dates</span>
          </label>

          {error && (
            <div className="workbook-alert error">
              <AlertCircle style={{ width: 14, height: 14, flexShrink: 0 }} />
              <span>{error}</span>
            </div>
          )}

          {preview && (
            <div className="workbook-preview">
              <div className="workbook-stats">
                <div className="wb-stat">
                  <span className="wb-stat-num">{preview.actionItemsCreated}</span>
                  <span className="wb-stat-label">New Tasks</span>
                </div>
                <div className="wb-stat">
                  <span className="wb-stat-num">{preview.actionItemsUpdated}</span>
                  <span className="wb-stat-label">Updated Tasks</span>
                </div>
                <div className="wb-stat">
                  <span className="wb-stat-num">{preview.calendarEventsCreated}</span>
                  <span className="wb-stat-label">Calendar Events</span>
                </div>
                <div className="wb-stat">
                  <span className="wb-stat-num">{preview.contactsCreated + preview.contactsUpdated}</span>
                  <span className="wb-stat-label">
                    Contacts ({preview.contactsCreated} new / {preview.contactsUpdated} upd)
                  </span>
                </div>
                <div className="wb-stat">
                  <span className="wb-stat-num">
                    {preview.actionItemsSkipped + preview.contactsSkipped}
                  </span>
                  <span className="wb-stat-label">Skipped Rows</span>
                </div>
              </div>

              {preview.skippedSheets.length > 0 && (
                <div className="workbook-alert">
                  <AlertCircle style={{ width: 14, height: 14, flexShrink: 0 }} />
                  <span>
                    Kept in Google Drive (skipped):{" "}
                    <strong>{preview.skippedSheets.join(", ")}</strong>
                  </span>
                </div>
              )}

              {preview.warnings.map((w, i) => (
                <div key={i} className="workbook-alert">
                  <AlertCircle style={{ width: 14, height: 14, flexShrink: 0 }} />
                  <span>{w}</span>
                </div>
              ))}

              {preview.previewRows.length > 0 && (
                <div className="wb-preview-list">
                  {preview.previewRows.map((row, idx) => (
                    <div key={idx} className="wb-preview-row">
                      <span className={`wb-badge ${row.status}`}>{row.status}</span>
                      <span className="wb-badge kind">
                        {row.kind === "actionItem" ? "Task" : "Contact"}
                      </span>
                      <span className="wb-row-title">{row.title}</span>
                      <span className="wb-row-detail">{row.detail}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        <div className="modal-foot">
          <span className="hint-text">
            {busy ? "Analyzing workbook…" : preview ? "Preview ready — nothing saved yet." : ""}
          </span>
          <span style={{ flex: 1 }} />
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={busy || !hasImportableRows}
            onClick={() => void commitImport()}
          >
            Import {hasImportableRows ? `(${totalActionChanges + totalContactChanges})` : ""}
          </button>
        </div>
      </div>
    </div>
  );
}
