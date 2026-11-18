import { emit } from "@tauri-apps/api/event";
import type { Fixture } from "../mocks";
import { fixture as pillFixture } from "./recording-pill";

export { label } from "./recording-pill";

export const fixture: Fixture = {
  ...pillFixture,
  stop_chat_speech_recording: null,
  remove_recording_widget_item: null,
  get_context_items: [],
};

function emitModel(targetLocked: boolean) {
  return emit("recording-widget-model", {
    targets: [{ id: "__dictation__", label: "Dictation" }],
    selectedId: "__dictation__",
    selectedLabel: "Dictation",
    targetLocked,
    items: [{ index: 0, kind: "image", label: "Image #1", preview: "" }],
    itemRows: 1,
    contextVersion: 1,
    stopChatDestination: "",
  });
}

export const globals = { lockTarget: () => emitModel(true) };

export async function show() {
  await emitModel(false);
  await emit("recording-widget-state", { state: "recording", level: 0.5, elapsedMs: 0, monochromatic: false });
}
