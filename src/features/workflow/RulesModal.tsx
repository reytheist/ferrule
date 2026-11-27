import { useState } from "react";
import { Filter, Play, Plus, Tag, Trash2, X } from "lucide-react";
import type {
  InboxCategory,
  LabelId,
  RuleAction,
  RuleCondition,
  SpaceColor,
} from "../../types/mail";
import type { MailApp } from "../shell/useMailApp";

const COLORS: SpaceColor[] = ["azure", "berry", "teal", "amber", "violet", "emerald"];

export function RulesModal({ app }: { app: MailApp }) {
  const [tab, setTab] = useState<"rules" | "labels">("rules");
  const [busy, setBusy] = useState(false);

  // Label form
  const [labelName, setLabelName] = useState("");
  const [labelColor, setLabelColor] = useState<SpaceColor>("azure");

  // Rule form
  const [ruleName, setRuleName] = useState("");
  const [condKind, setCondKind] = useState<RuleCondition["kind"]>("fromContains");
  const [condValue, setCondValue] = useState("");
  const [actKind, setActKind] = useState<RuleAction["kind"]>("applyLabel");
  const [actLabelId, setActLabelId] = useState<string>("");
  const [actCategory, setActCategory] = useState<InboxCategory>("receipts");

  if (!app.rulesOpen) return null;

  async function handleSaveLabel() {
    if (busy || !labelName.trim()) return;
    setBusy(true);
    try {
      await app.persistLabel({ id: null, name: labelName.trim(), color: labelColor });
      setLabelName("");
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveRule() {
    if (busy || !ruleName.trim()) return;
    const condition: RuleCondition =
      condKind === "hasAttachment"
        ? { kind: "hasAttachment" }
        : { kind: condKind, value: condValue.trim() };

    if (condKind !== "hasAttachment" && !condValue.trim()) return;

    let action: RuleAction;
    if (actKind === "applyLabel") {
      const targetLabel = actLabelId || app.labels[0]?.id;
      if (!targetLabel) {
        app.showToast("Create a label first before using Apply Label.");
        return;
      }
      action = { kind: "applyLabel", labelId: targetLabel as LabelId };
    } else if (actKind === "setCategory") {
      action = { kind: "setCategory", category: actCategory };
    } else {
      action = { kind: actKind };
    }

    setBusy(true);
    try {
      await app.persistRule({
        id: null,
        name: ruleName.trim(),
        enabled: true,
        matchAll: true,
        conditions: [condition],
        actions: [action],
      });
      setRuleName("");
      setCondValue("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) app.closeRules();
      }}
    >
      <div className="modal wide" role="dialog" aria-label="Rules & Labels">
        <div className="modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Filter style={{ width: 15, height: 15, color: "var(--app-accent)" }} />
            <h2>Rules &amp; Auto-Labeling</h2>
          </div>
          <button className="icon-btn" onClick={app.closeRules} title="Close (Esc)">
            <X />
          </button>
        </div>

        <div className="modal-body">
          <div className="segmented-control" role="tablist">
            <button
              type="button"
              className={`seg-btn${tab === "rules" ? " active" : ""}`}
              onClick={() => setTab("rules")}
            >
              Rules ({app.rules.length})
            </button>
            <button
              type="button"
              className={`seg-btn${tab === "labels" ? " active" : ""}`}
              onClick={() => setTab("labels")}
            >
              Labels ({app.labels.length})
            </button>
          </div>

          {tab === "rules" ? (
            <div className="signature-manager">
              <div className="signature-editor-box">
                <input
                  className="input"
                  placeholder="Rule name (e.g. Auto-label GitHub notifications)"
                  value={ruleName}
                  onChange={(e) => setRuleName(e.target.value)}
                />
                <div style={{ display: "flex", gap: 6 }}>
                  <select
                    className="select"
                    value={condKind}
                    onChange={(e) => setCondKind(e.target.value as RuleCondition["kind"])}
                  >
                    <option value="fromContains">From contains</option>
                    <option value="domainEquals">Sender domain equals</option>
                    <option value="subjectContains">Subject contains</option>
                    <option value="bodyContains">Body contains</option>
                    <option value="hasAttachment">Has attachment</option>
                  </select>
                  {condKind !== "hasAttachment" && (
                    <input
                      className="input"
                      placeholder="Match value…"
                      value={condValue}
                      onChange={(e) => setCondValue(e.target.value)}
                    />
                  )}
                </div>
                <div style={{ display: "flex", gap: 6 }}>
                  <select
                    className="select"
                    value={actKind}
                    onChange={(e) => setActKind(e.target.value as RuleAction["kind"])}
                  >
                    <option value="applyLabel">Apply label</option>
                    <option value="archive">Archive</option>
                    <option value="markRead">Mark read</option>
                    <option value="star">Star</option>
                    <option value="setCategory">Route to tab</option>
                    <option value="trash">Move to Trash</option>
                  </select>

                  {actKind === "applyLabel" && (
                    <select
                      className="select"
                      value={actLabelId || app.labels[0]?.id || ""}
                      onChange={(e) => setActLabelId(e.target.value)}
                    >
                      {app.labels.length === 0 ? (
                        <option value="">No labels yet</option>
                      ) : (
                        app.labels.map((lbl) => (
                          <option key={lbl.id} value={lbl.id}>
                            {lbl.name}
                          </option>
                        ))
                      )}
                    </select>
                  )}

                  {actKind === "setCategory" && (
                    <select
                      className="select"
                      value={actCategory}
                      onChange={(e) => setActCategory(e.target.value as InboxCategory)}
                    >
                      <option value="primary">Primary</option>
                      <option value="receipts">Paper Trail / Receipts</option>
                      <option value="newsletters">Feed / Newsletters</option>
                    </select>
                  )}

                  <button
                    type="button"
                    className="btn compact primary"
                    disabled={busy || !ruleName.trim()}
                    onClick={() => void handleSaveRule()}
                  >
                    <Plus />
                    Add Rule
                  </button>
                </div>
              </div>

              {app.rules.length === 0 ? (
                <span className="hint-text">
                  Rules run automatically on incoming mail and can be applied retroactively with one
                  click.
                </span>
              ) : (
                <div className="signature-list">
                  {app.rules.map((rule) => (
                    <div key={rule.id} className="signature-item">
                      <div className="signature-item-main">
                        <strong>{rule.name}</strong>
                        <span className="kbd">{rule.enabled ? "Active" : "Paused"}</span>
                      </div>
                      <button
                        type="button"
                        className="btn compact"
                        title="Run rule on existing mail now"
                        onClick={() => void app.runRuleNow(rule.id)}
                      >
                        <Play />
                        Run now
                      </button>
                      <button
                        type="button"
                        className="icon-btn"
                        title="Delete rule"
                        onClick={() => void app.removeRule(rule.id)}
                      >
                        <Trash2 />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ) : (
            <div className="signature-manager">
              <div className="signature-editor-box">
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    className="input"
                    placeholder="Label name (e.g. Clients, Tax, Travel)"
                    value={labelName}
                    onChange={(e) => setLabelName(e.target.value)}
                  />
                  <select
                    className="select"
                    value={labelColor}
                    onChange={(e) => setLabelColor(e.target.value as SpaceColor)}
                  >
                    {COLORS.map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="btn compact primary"
                    disabled={busy || !labelName.trim()}
                    onClick={() => void handleSaveLabel()}
                  >
                    <Tag />
                    Create Label
                  </button>
                </div>
              </div>

              {app.labels.length === 0 ? (
                <span className="hint-text">No labels created yet.</span>
              ) : (
                <div className="signature-list">
                  {app.labels.map((lbl) => (
                    <div key={lbl.id} className="signature-item">
                      <span className={`acct-dot space-${lbl.color}`} />
                      <div className="signature-item-main">
                        <strong>{lbl.name}</strong>
                        <span className="hint-text">{lbl.threadCount} threads</span>
                      </div>
                      <button
                        type="button"
                        className="icon-btn"
                        title="Delete label"
                        onClick={() => void app.removeLabel(lbl.id)}
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
          <button type="button" className="btn primary" onClick={app.closeRules}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
