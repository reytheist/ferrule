import { invoke } from "@tauri-apps/api/core";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function healthCheck(): Promise<string> {
  if (!isTauriRuntime()) {
    return "browser-preview";
  }
  return invoke<string>("health_check");
}
