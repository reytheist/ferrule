import { useId } from "react";
import type { AppUpdater } from "./useAppUpdater";

function buildTooltip(updater: AppUpdater): string {
  const { state, lastCheckedAt } = updater;
  switch (state.kind) {
    case "checking":
      return "Checking for Ferrule updates…";
    case "available":
      return `Ferrule v${state.release.version} available (current v${state.currentVersion}) — Click to download and upgrade`;
    case "downloading": {
      const pct = state.progress?.percent;
      return pct !== null && pct !== undefined
        ? `Downloading Ferrule v${state.release.version}… ${pct}%`
        : `Downloading Ferrule v${state.release.version}…`;
    }
    case "launched":
      return `${state.message} — Click to check again`;
    case "error":
      return state.release
        ? `Upgrade failed: ${state.message} — Click to retry v${state.release.version}`
        : `Update check failed: ${state.message} — Click to retry`;
    case "idle": {
      if (lastCheckedAt === null) {
        return `Check for updates (Ferrule v${state.currentVersion})`;
      }
      const mins = Math.max(0, Math.round((Date.now() - lastCheckedAt) / 60_000));
      const checkedStr = mins < 1 ? "just now" : mins < 60 ? `${mins}m ago` : `${Math.round(mins / 60)}h ago`;
      return `Ferrule v${state.currentVersion} is up to date · checked ${checkedStr} — Click to check`;
    }
  }
}

export function UpdateButton({ updater }: { updater: AppUpdater }) {
  const gradientId = useId();
  const { state } = updater;
  const isAvailable = state.kind === "available";
  const isChecking = state.kind === "checking";
  const isDownloading = state.kind === "downloading";
  const isLaunched = state.kind === "launched";
  const isError = state.kind === "error";

  const tooltip = buildTooltip(updater);
  const progressPct = isDownloading ? state.progress?.percent ?? null : null;

  const className = [
    "icon-btn",
    "update-btn",
    isAvailable ? "available" : "",
    isChecking ? "checking" : "",
    isDownloading ? "downloading" : "",
    isLaunched ? "launched" : "",
    isError ? "error" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <button
      type="button"
      className={className}
      onClick={() => void updater.handlePrimaryAction()}
      disabled={isChecking || isDownloading}
      aria-busy={isChecking || isDownloading}
      aria-label={tooltip}
      title={tooltip}
      data-update-state={state.kind}
    >
      <svg
        className={`update-refresh-icon${isAvailable ? " rainbow" : ""}${
          isChecking || isDownloading ? " spinning" : ""
        }`}
        viewBox="0 0 24 24"
        fill="none"
        stroke={isAvailable ? `url(#${gradientId})` : "currentColor"}
        strokeWidth="2.1"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        {isAvailable && (
          <defs>
            <linearGradient id={gradientId} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#ff5f6d" />
              <stop offset="22%" stopColor="#fbbf24" />
              <stop offset="44%" stopColor="#34d399" />
              <stop offset="66%" stopColor="#38bdf8" />
              <stop offset="84%" stopColor="#818cf8" />
              <stop offset="100%" stopColor="#e879f9" />
            </linearGradient>
          </defs>
        )}
        <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
        <path d="M21 3v5h-5" />
        <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
        <path d="M8 16H3v5" />
      </svg>
      {isDownloading && (
        <span className="update-progress-label">
          {progressPct !== null ? `${progressPct}%` : "…"}
        </span>
      )}
    </button>
  );
}
