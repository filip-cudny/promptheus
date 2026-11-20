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

const THUMBNAIL_SIZE = 32;
const IMAGES_PER_ROW = 3;
const MAX_ITEM_ROWS = 3;

function drawThumbnail(): string {
  const canvas = document.createElement("canvas");
  canvas.width = THUMBNAIL_SIZE;
  canvas.height = THUMBNAIL_SIZE;
  const g = canvas.getContext("2d")!;
  g.fillStyle = "#1f2330";
  g.fillRect(0, 0, THUMBNAIL_SIZE, THUMBNAIL_SIZE);
  g.fillStyle = "#c3cad9";
  [4, 10, 16].forEach((y, i) => g.fillRect(3, y, 14 + (i % 2) * 8, 2));
  g.fillStyle = "#5b2230";
  g.fillRect(0, 22, THUMBNAIL_SIZE, 10);
  g.fillStyle = "#ff9aa8";
  g.fillRect(3, 26, 20, 2);
  return canvas.toDataURL("image/png");
}

const image = {
  index: 0,
  kind: "image",
  label: "#1",
  preview: "",
  thumbnail: drawThumbnail(),
  previewWidth: 20,
  previewHeight: 20,
};

function itemRows(images: number, texts: number) {
  return Math.min(Math.ceil(images / IMAGES_PER_ROW) + texts, MAX_ITEM_ROWS);
}

function emitModel(items: (typeof image)[], targetLocked: boolean) {
  return emit("recording-widget-model", {
    targets: [{ id: "__dictation__", label: "Dictation" }],
    selectedId: "__dictation__",
    selectedLabel: "Dictation",
    targetLocked,
    items,
    itemRows: itemRows(items.length, 0),
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
