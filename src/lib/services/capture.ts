import { invoke } from "@tauri-apps/api/core";
import { error } from "@tauri-apps/plugin-log";

export type CaptureOwner = "context-menu" | "conversation-dialog";

let available: Promise<boolean> | null = null;

export function captureAvailable(): Promise<boolean> {
  available ??= invoke<boolean>("capture_available").catch((e) => {
    error("capture_available failed: " + e);
    return false;
  });
  return available;
}

export function captureScreenshotForWindow(owner: CaptureOwner): Promise<string | null> {
  return invoke("capture_screenshot_for_window", { owner });
}

export function captureSelectionForWindow(owner: CaptureOwner): Promise<string | null> {
  return invoke("capture_selection_for_window", { owner });
}

export function cancelWindowCapture(owner: CaptureOwner): Promise<void> {
  return invoke("cancel_window_capture", { owner });
}
