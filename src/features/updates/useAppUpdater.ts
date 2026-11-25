import { useCallback, useEffect, useRef, useState } from "react";
import {
  checkForUpdates as checkForUpdatesCommand,
  downloadAndInstallUpdate as downloadAndInstallUpdateCommand,
  onUpdateDownloadProgress,
} from "../../ipc/core";
import type {
  UpdateDownloadProgress,
  UpdateReleaseInfo,
} from "../../types/mail";

const INITIAL_VERSION = "0.2.1";
const STARTUP_CHECK_DELAY_MS = 2_500;
const PERIODIC_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000;

export type UpdateDevPreset = "idle" | "checking" | "available" | "downloading" | "error";

export type AppUpdateState =
  | { kind: "idle"; currentVersion: string; lastCheckedAt: number | null }
  | { kind: "checking"; currentVersion: string; manual: boolean }
  | { kind: "available"; currentVersion: string; release: UpdateReleaseInfo }
  | {
      kind: "downloading";
      currentVersion: string;
      release: UpdateReleaseInfo;
      progress: UpdateDownloadProgress | null;
    }
  | {
      kind: "launched";
      currentVersion: string;
      release: UpdateReleaseInfo;
      message: string;
    }
  | {
      kind: "error";
      currentVersion: string;
      message: string;
      release: UpdateReleaseInfo | null;
    };

export interface AppUpdater {
  state: AppUpdateState;
  currentVersion: string;
  lastCheckedAt: number | null;
  checkForUpdates: (options?: { manual?: boolean }) => Promise<void>;
  installUpdate: () => Promise<void>;
  handlePrimaryAction: () => Promise<void>;
  simulateState: (preset: UpdateDevPreset) => void;
}

function formatUpdateError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error !== null && typeof error === "object" && "reason" in error) {
    const desc = Object.getOwnPropertyDescriptor(error, "reason");
    if (typeof desc?.value === "string" && desc.value.trim()) return desc.value;
  }
  return "Update check failed";
}

function mockPreviewRelease(): UpdateReleaseInfo {
  return {
    version: "0.3.0",
    tagName: "v0.3.0",
    name: "Ferrule v0.3.0",
    notes: "Preview release for testing in-app update states.",
    releaseUrl: "https://github.com/reytheist/ferrule/releases/tag/v0.3.0",
    publishedAt: new Date().toISOString(),
    asset: {
      name: "Ferrule_0.3.0_x64-setup.exe",
      downloadUrl:
        "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_x64-setup.exe",
      sizeBytes: 12_400_000,
    },
  };
}

export function useAppUpdater(showToast: (message: string) => void): AppUpdater {
  const [state, setState] = useState<AppUpdateState>({
    kind: "idle",
    currentVersion: INITIAL_VERSION,
    lastCheckedAt: null,
  });
  const [lastCheckedAt, setLastCheckedAt] = useState<number | null>(null);

  const stateRef = useRef(state);
  stateRef.current = state;
  const lastCheckedAtRef = useRef<number | null>(lastCheckedAt);
  lastCheckedAtRef.current = lastCheckedAt;
  const inFlightRef = useRef(false);
  const showToastRef = useRef(showToast);
  showToastRef.current = showToast;

  // Listen for streaming download progress from the Rust updater (or mock store).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;

    void onUpdateDownloadProgress((progress) => {
      if (disposed) return;
      setState((prev) => {
        if (prev.kind !== "downloading") return prev;
        return { ...prev, progress };
      });
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const checkForUpdates = useCallback(async (options?: { manual?: boolean }) => {
    const manual = options?.manual ?? false;
    const current = stateRef.current;
    if (inFlightRef.current || current.kind === "downloading") return;
    if (!manual && current.kind === "available") return;

    inFlightRef.current = true;
    const prevState = current;
    setState({
      kind: "checking",
      currentVersion: current.currentVersion,
      manual,
    });

    try {
      const result = await checkForUpdatesCommand();
      const checkedAt = Date.now();
      setLastCheckedAt(checkedAt);

      if (result.updateAvailable && result.release) {
        setState({
          kind: "available",
          currentVersion: result.currentVersion,
          release: result.release,
        });
        if (manual) {
          showToastRef.current(
            `Ferrule v${result.release.version} is available — click the refresh icon to upgrade`,
          );
        }
      } else {
        setState({
          kind: "idle",
          currentVersion: result.currentVersion,
          lastCheckedAt: checkedAt,
        });
        if (manual) {
          showToastRef.current(`Ferrule v${result.currentVersion} is up to date`);
        }
      }
    } catch (err) {
      if (manual) {
        const message = formatUpdateError(err);
        setState({
          kind: "error",
          currentVersion: prevState.currentVersion,
          message,
          release:
            prevState.kind === "available" || prevState.kind === "error"
              ? prevState.release
              : null,
        });
        showToastRef.current(`Update check failed: ${message}`);
      } else {
        // Silent on automatic background checks (e.g. offline / captive portal).
        setState(
          prevState.kind === "checking"
            ? {
                kind: "idle",
                currentVersion: prevState.currentVersion,
                lastCheckedAt: lastCheckedAtRef.current,
              }
            : prevState,
        );
      }
    } finally {
      inFlightRef.current = false;
    }
  }, []);

  const installUpdate = useCallback(async () => {
    const current = stateRef.current;
    const release =
      current.kind === "available"
        ? current.release
        : current.kind === "error"
          ? current.release
          : null;
    if (!release || inFlightRef.current || current.kind === "downloading") return;

    inFlightRef.current = true;
    setState({
      kind: "downloading",
      currentVersion: current.currentVersion,
      release,
      progress: release.asset
        ? {
            version: release.version,
            downloadedBytes: 0,
            totalBytes: release.asset.sizeBytes,
            percent: 0,
          }
        : null,
    });

    try {
      const outcome = await downloadAndInstallUpdateCommand({
        version: release.version,
        releaseUrl: release.releaseUrl,
        assetUrl: release.asset?.downloadUrl ?? null,
        assetName: release.asset?.name ?? null,
      });

      const message =
        outcome.kind === "installerLaunched"
          ? `Ferrule v${release.version} installer launched`
          : `Opened Ferrule v${release.version} release page`;

      setState({
        kind: "launched",
        currentVersion: current.currentVersion,
        release,
        message,
      });
      showToastRef.current(message);
    } catch (err) {
      const message = formatUpdateError(err);
      setState({
        kind: "error",
        currentVersion: current.currentVersion,
        message,
        release,
      });
      showToastRef.current(`Upgrade failed: ${message}`);
    } finally {
      inFlightRef.current = false;
    }
  }, []);

  const handlePrimaryAction = useCallback(async () => {
    const current = stateRef.current;
    if (current.kind === "checking" || current.kind === "downloading") return;
    if (current.kind === "available") {
      await installUpdate();
      return;
    }
    if (current.kind === "error" && current.release) {
      await installUpdate();
      return;
    }
    await checkForUpdates({ manual: true });
  }, [checkForUpdates, installUpdate]);

  const simulateState = useCallback((preset: UpdateDevPreset) => {
    const ver = stateRef.current.currentVersion;
    const release = mockPreviewRelease();
    if (preset === "idle") {
      setState({ kind: "idle", currentVersion: ver, lastCheckedAt: Date.now() });
    } else if (preset === "checking") {
      setState({ kind: "checking", currentVersion: ver, manual: true });
    } else if (preset === "available") {
      setState({ kind: "available", currentVersion: ver, release });
    } else if (preset === "downloading") {
      setState({
        kind: "downloading",
        currentVersion: ver,
        release,
        progress: {
          version: release.version,
          downloadedBytes: 5_208_000,
          totalBytes: 12_400_000,
          percent: 42,
        },
      });
    } else {
      setState({
        kind: "error",
        currentVersion: ver,
        message: "Simulated update network error",
        release,
      });
    }
  }, []);

  // Automatic non-blocking startup check + periodic background checks.
  useEffect(() => {
    const startupTimer = window.setTimeout(() => {
      void checkForUpdates({ manual: false });
    }, STARTUP_CHECK_DELAY_MS);

    const periodicTimer = window.setInterval(() => {
      void checkForUpdates({ manual: false });
    }, PERIODIC_CHECK_INTERVAL_MS);

    return () => {
      window.clearTimeout(startupTimer);
      window.clearInterval(periodicTimer);
    };
  }, [checkForUpdates]);

  return {
    state,
    currentVersion: state.currentVersion,
    lastCheckedAt,
    checkForUpdates,
    installUpdate,
    handlePrimaryAction,
    simulateState,
  };
}
