import { emit } from "@tauri-apps/api/event";
import type { Fixture } from "../mocks";
import { fixture as pillFixture } from "./recording-pill";

export { label } from "./recording-pill";

export const platform = "MacIntel";

export const fixture: Fixture = {
  ...pillFixture,
  capture_screenshot_into_recording_widget: null,
  stop_chat_speech_recording: null,
  remove_recording_widget_item: null,
  get_context_items: [],
};

const image = { index: 0, kind: "image", label: "Image #1", preview: "" };

function emitModel(items: (typeof image)[], targetLocked: boolean) {
  return emit("recording-widget-model", {
    targets: [{ id: "__dictation__", label: "Dictation" }],
    selectedId: "__dictation__",
    selectedLabel: "Dictation",
    targetLocked,
    items,
    itemRows: items.length,
    contextVersion: items.length + 1,
    stopChatDestination: "",
  });
}

export const globals = {
  addImage: () => emitModel([image], false),
  lockTarget: () => emitModel([image], true),
};

export async function show() {
  await emitModel([], false);
  await emit("recording-widget-state", { state: "recording", level: 0.5, elapsedMs: 0, monochromatic: false });
}
