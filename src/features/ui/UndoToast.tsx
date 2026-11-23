import { useEffect } from "react";
import type { MailApp } from "../shell/useMailApp";

export function UndoToast({ app }: { app: MailApp }) {
  const toast = app.toast;
  const toastId = toast?.id ?? null;
  const durationMs = toast?.durationMs ?? 0;
  const dismiss = app.dismissToast;

  useEffect(() => {
    if (toastId === null) return;
    const timer = window.setTimeout(dismiss, durationMs);
    return () => window.clearTimeout(timer);
  }, [toastId, durationMs, dismiss]);

  if (!toast) return null;

  return (
    <div className="toast" role="status">
      <span className="toast-msg">{toast.message}</span>
      {toast.onUndo && (
        <button
          onClick={() => {
            const undo = toast.onUndo;
            dismiss();
            if (undo) void undo();
          }}
        >
          Undo
        </button>
      )}
      <span className="toast-bar" style={{ animationDuration: `${durationMs}ms` }} />
    </div>
  );
}
