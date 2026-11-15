import { emit } from "@tauri-apps/api/event";
import type { Fixture } from "../mocks";

export const label = "recording-widget";

export const fixture: Fixture = {};

export async function show() {
  await emit("recording-widget-model", {
    targets: [
      { id: "__dictation__", label: "Dictation" },
      { id: "webview:chatgpt", label: "ChatGPT" },
      { id: "webview:claude", label: "Claude" },
      { id: "fix-grammar", label: "Fix grammar" },
    ],
    selectedId: "__dictation__",
    selectedLabel: "Dictation",
    targetLocked: false,
    items: [],
    itemRows: 0,
    contextVersion: 1,
    stopChatDestination: "",
  });
  await emit("recording-widget-state", { state: "recording", level: 0.7, elapsedMs: 14_000, monochromatic: false });
}
