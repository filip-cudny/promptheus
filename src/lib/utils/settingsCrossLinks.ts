import type { KeymapGroup, SurfaceKind } from "../types";
import { actionShortcuts, formatShortcut } from "./keymapShortcuts.ts";
import { isMac } from "./platform.ts";

export const ASSIGNMENT_SURFACES = [
  "chat",
  "quick_actions",
  "title_generation",
  "settings_agent",
  "speech_to_text",
] as const satisfies readonly SurfaceKind[];

const ASSIGNMENT_PREFIX = "assignment:";
const ENV_PREFIX = "env:";

export const CROSS_LINKS = {
  contextMenuShortcuts: "keymaps.open_context_menu",
  speechShortcuts: "keymaps.speech_to_text_toggle",
  chatModels: "assignment:chat",
  generalShortcuts: "section:shortcuts",
} as const;

export function isAssignmentKey(key: string): boolean {
  return key.startsWith(ASSIGNMENT_PREFIX);
}

export function parseAssignmentLink(key: string): SurfaceKind | null {
  if (!isAssignmentKey(key)) return null;
  const surface = key.slice(ASSIGNMENT_PREFIX.length);
  return ASSIGNMENT_SURFACES.find((s) => s === surface) ?? null;
}

export function envLink(name: string): string {
  return `${ENV_PREFIX}${name}`;
}

export function isEnvKey(key: string): boolean {
  return key.startsWith(ENV_PREFIX);
}

export function parseEnvLink(key: string): string | null {
  if (!isEnvKey(key)) return null;
  return key.slice(ENV_PREFIX.length) || null;
}

export function shortcutsText(keymaps: KeymapGroup[], action: string): string {
  const shortcuts = actionShortcuts(keymaps, action, isMac ? "macos" : "linux");
  return shortcuts.length > 0 ? shortcuts.map(formatShortcut).join(", ") : "not set";
}
