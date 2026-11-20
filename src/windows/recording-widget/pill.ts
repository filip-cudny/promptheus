import { invoke } from "@tauri-apps/api/core";
import { error as logError } from "@tauri-apps/plugin-log";
import claudeIcon from "$lib/icons/providers/claude.svg?raw";
import chatgptIcon from "$lib/icons/providers/chatgpt.svg?raw";

export interface PillTarget {
  id: string;
  label: string;
}

export interface PillItem {
  index: number;
  kind: "text" | "image";
  label: string;
  preview: string;
  thumbnail?: string;
  previewWidth?: number;
  previewHeight?: number;
}

export interface PillModel {
  targets: PillTarget[];
  selectedId: string;
  selectedLabel: string;
  targetLocked: boolean;
  items: PillItem[];
  itemRows: number;
  contextVersion: number;
  stopChatDestination: string;
}

export const PILL_WIDTH = 264;
export const RADIUS = 16;
export const PADDING_X = 14;
export const PREVIEW_MAX_HEIGHT = 160;
export const PREVIEW_GAP = 6;
export const PREVIEW_MIN_BOX = 32;
export const POINTER_OFFSET_BELOW = 20;
export const POINTER_OFFSET_ABOVE = 6;
export const TOOLTIP_HEIGHT = 24;
export const TOOLTIP_PADDING_X = 8;
export const DROPDOWN_ROW_HEIGHT = 28;
export const DROPDOWN_PADDING = 4;
export const DROPDOWN_SEPARATOR_HEIGHT = 9;
export const TARGET_ICONS: Record<string, string> = { "webview:claude": claudeIcon, "webview:chatgpt": chatgptIcon };
export const TARGET_ICON_SIZE = 14;
export const TARGET_ICON_GAP = 8;

export async function run(command: string, args?: Record<string, unknown>) {
  try {
    await invoke(command, args);
  } catch (e) {
    logError(`${command} failed: ${e}`);
  }
}

export function dropdownSeparatorIndex(targets: { id: string }[]): number {
  const lastWebview = targets.findLastIndex((target) => target.id.startsWith("webview:"));
  return lastWebview >= 0 && lastWebview < targets.length - 1 ? lastWebview : -1;
}
