import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { listen } from "@tauri-apps/api/event";

export type SyncState = "off" | "synced" | "syncing" | "conflict" | "error";

export interface SyncStatus {
  state: SyncState;
  message: string | null;
  repo_path: string | null;
  branch: string | null;
  last_sync: string | null;
}

export type SyncSource =
  | { kind: "url"; value: string }
  | { kind: "dir"; value: string };

export interface SyncEntries {
  settings: boolean;
  prompts: boolean;
  skills: boolean;
}

export interface SyncPrepared {
  repo_path: string;
  branch: string;
  empty: boolean;
  suggested_folder: string;
  found: SyncEntries;
}

export interface SyncConnectRequest {
  repo_path: string;
  folder: string;
  entries: SyncEntries;
  empty: boolean;
}

export async function getSettingsSyncStatus(): Promise<SyncStatus> {
  return invoke("get_settings_sync_status");
}

export async function syncSettingsNow(): Promise<SyncStatus> {
  return invoke("sync_settings_now");
}

export async function syncPrepare(source: SyncSource): Promise<SyncPrepared> {
  return invoke("sync_prepare", { source });
}

export async function syncConnect(
  request: SyncConnectRequest,
): Promise<SyncStatus> {
  return invoke("sync_connect", { request });
}

export async function syncCloneDir(): Promise<string> {
  return invoke("sync_clone_dir");
}

export function onSettingsSyncStatusChanged(
  callback: (status: SyncStatus) => void,
): Promise<UnlistenFn> {
  return listen<SyncStatus>("settings-sync-status-changed", (event) =>
    callback(event.payload),
  );
}
