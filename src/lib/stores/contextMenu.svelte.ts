import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { error, warn } from "@tauri-apps/plugin-log";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { MenuItem } from "$lib/types/menu";
import { startExecution, isExecuting, getExecutingSkillId, cancelExecution } from "$lib/stores/execution.svelte";
import { onContextChanged, onHistoryChanged } from "$lib/services/events";
import { openConversationDialog } from "$lib/services/conversationDialog";
import { getSettings } from "$lib/services/settings";
import { createDigitSelector } from "$lib/utils/digitSelect";
import { fetchSkillMetadata, getSkillMeta } from "$lib/services/skillMetadata.svelte";
import {
  cancelWindowCapture,
  captureScreenshotForWindow,
  captureSelectionForWindow,
} from "$lib/services/capture";
import { appendContext, appendContextImage } from "$lib/services/context";

interface WorkArea {
  cursorX: number;
  cursorY: number;
  workX: number;
  workY: number;
  workWidth: number;
  workHeight: number;
  shellPlacement: boolean;
}

let _items = $state<MenuItem[]>([]);
let _selectedIndex = $state(-1);
let _visible = $state(false);
let _isRecording = $state(false);
let _isTranscribing = $state(false);
let _suppressClose = $state(false);
let _recordingSkillId = $state<string | null>(null);
let _openTrigger = $state(0);
let _capture = $state<"screenshot" | "selection" | null>(null);
let _captureRun = 0;
let _openedAt = 0;
let _workArea: WorkArea | null = null;
let _firstPlacementPending = false;

const BLUR_GRACE_MS = 500;
const DEFAULT_DIGIT_DELAY_MS = 200;
let digitDelayMs = DEFAULT_DIGIT_DELAY_MS;
let digitIsAlternative = false;
let unlistens: UnlistenFn[] = [];

const digitSelector = createDigitSelector({
  getItemCount: () => _allSkillItems.length,
  getDelayMs: () => digitDelayMs,
  run: runNumberedItem,
  setTimer: (fn, ms) => setTimeout(fn, ms),
  clearTimer: (t) => clearTimeout(t),
});

const _navigableItems = $derived(
  _items.filter((item) => item.enabled),
);

const _allSkillItems = $derived(
  _items.filter((item) => item.item_type === "skill"),
);

function getItems(): MenuItem[] {
  return _items;
}

function getSelectedIndex(): number {
  return _selectedIndex;
}

function setSelectedIndex(index: number) {
  _selectedIndex = index;
}

function isVisible(): boolean {
  return _visible;
}

function getOpenTrigger(): number {
  return _openTrigger;
}

function isRecording(): boolean {
  return _isRecording;
}

function suppressClose() {
  _suppressClose = true;
}

function isSuppressed(): boolean {
  return _suppressClose;
}

function resumeClose() {
  _suppressClose = false;
}

function isInBlurGrace(): boolean {
  return Date.now() - _openedAt < BLUR_GRACE_MS;
}

function getCapture(): "screenshot" | "selection" | null {
  return _capture;
}

function beginCapture(kind: "screenshot" | "selection"): number {
  _capture = kind;
  return ++_captureRun;
}

function finishCapture(run: number) {
  if (run === _captureRun) _capture = null;
}

function endCapture() {
  if (_capture === null) return;
  _capture = null;
  _captureRun++;
  cancelWindowCapture("context-menu").catch((e) => error("Failed to cancel capture: " + e));
}

async function captureScreenshot() {
  const run = beginCapture("screenshot");
  try {
    const data = await captureScreenshotForWindow("context-menu");
    if (data && run === _captureRun) await appendContextImage(data, "image/png");
  } catch (e) {
    error("Screenshot capture failed: " + e);
  } finally {
    finishCapture(run);
  }
}

async function toggleSelection() {
  if (_capture === "selection") {
    endCapture();
    return;
  }
  const run = beginCapture("selection");
  try {
    const text = await captureSelectionForWindow("context-menu");
    if (text && run === _captureRun) await appendContext(text);
  } catch (e) {
    error("Selection capture failed: " + e);
  } finally {
    finishCapture(run);
  }
}

function getRecordingSkillId(): string | null {
  return _recordingSkillId;
}

function applyItemStates(items: MenuItem[]): MenuItem[] {
  if (isExecuting()) {
    const executingId = getExecutingSkillId();
    return items.map((item) => {
      if (item.item_type !== "skill") return item;
      const data = item.data as { skill_id: string } | null;
      if (data?.skill_id === executingId) return item;
      return { ...item, enabled: false };
    });
  }
  if (_isRecording) {
    if (_recordingSkillId) {
      return items.map((item) => {
        if (item.item_type === "speech") return { ...item, enabled: false };
        if (item.item_type !== "skill") return item;
        const data = item.data as { skill_id: string } | null;
        if (data?.skill_id === _recordingSkillId) return item;
        return { ...item, enabled: false };
      });
    }
    return items.map((item) =>
      item.item_type === "skill" ? { ...item, enabled: false } : item,
    );
  }
  if (_isTranscribing) {
    return items.map((item) =>
      item.item_type === "skill" || item.item_type === "speech"
        ? { ...item, enabled: false }
        : item,
    );
  }
  return items;
}

async function fetchRecordingState(): Promise<void> {
  try {
    const state = await invoke<{
      is_recording: boolean;
      is_transcribing: boolean;
      action_id: string | null;
    }>("get_recording_state");
    _isRecording = state.is_recording;
    _isTranscribing = state.is_transcribing;
    _recordingSkillId = state.action_id;
  } catch (e) {
    error("Failed to fetch recording state: " + e);
  }
}

function clearRecordingState() {
  _isRecording = false;
  _isTranscribing = false;
  _recordingSkillId = null;
}

function getWorkArea(): WorkArea | null {
  return _workArea;
}

function isFirstPlacementPending(): boolean {
  return _firstPlacementPending;
}

function clearFirstPlacementPending() {
  _firstPlacementPending = false;
}

async function openMenu(workArea: WorkArea | null) {
  endCapture();
  try {
    _workArea = workArea;
    _firstPlacementPending = workArea?.shellPlacement === true;
    await fetchRecordingState();
    const fetched = await invoke<MenuItem[]>("get_context_menu_items");
    _items = applyItemStates(fetched);
    _selectedIndex = -1;
    digitSelector.reset();
    digitDelayMs = await readDigitDelayMs();
    _openedAt = Date.now();
    _visible = true;
    _openTrigger++;
  } catch (e) {
    error("Failed to open context menu: " + e);
  }
}

async function closeMenu(restoreFocus = true) {
  if (!_visible) return;
  endCapture();
  _visible = false;
  _items = [];
  _selectedIndex = -1;
  digitSelector.reset();

  await invoke("hide_context_menu_panel", { restoreFocus });
}

function moveSelection(direction: 1 | -1) {
  if (_navigableItems.length === 0) return;

  const currentItem = _selectedIndex >= 0 ? _items[_selectedIndex] : null;
  const currentNavIndex = currentItem
    ? _navigableItems.indexOf(currentItem)
    : -1;

  let nextNavIndex: number;
  if (currentNavIndex === -1) {
    const firstSkill = _navigableItems.find((item) => item.item_type === "skill");
    nextNavIndex = firstSkill ? _navigableItems.indexOf(firstSkill) : 0;
  } else {
    nextNavIndex =
      (currentNavIndex + direction + _navigableItems.length) %
      _navigableItems.length;
  }

  const targetItem = _navigableItems[nextNavIndex];
  _selectedIndex = _items.indexOf(targetItem);
}

async function executeItem(index: number, shiftPressed: boolean = false) {
  const item = _items[index];
  if (!item) return;

  const itemId = item.id;
  const itemType = item.item_type;
  const itemEnabled = item.enabled;

  if (itemType === "skill") {
    const data = item.data as { skill_id: string; skill_name: string } | null;
    if (!data?.skill_id) return;

    const skillId = data.skill_id;

    if (isExecuting() && skillId === getExecutingSkillId()) {
      await cancelExecution();
      return;
    }

    const isRecordingThis = _isRecording && _recordingSkillId === skillId;
    if (!itemEnabled && !isRecordingThis) return;

    if (isRecordingThis) {
      clearRecordingState();
      try {
        await invoke("execute_menu_item", {
          itemId,
          shiftPressed: true,
        });
      } catch (e) {
        error("Failed to stop speech recording for prompt: " + e);
      }
      await closeMenu();
      return;
    }

    if (shiftPressed) {
      await runInDialog(index, skillId, data.skill_name ?? item.label);
      return;
    }

    if (!itemEnabled) return;
    await closeMenu();
    startExecution(skillId, "context_menu");
    return;
  }

  if (itemType === "speech") {
    if (!itemEnabled) return;
    if (_isRecording && !_recordingSkillId) {
      clearRecordingState();
      try {
        await invoke("execute_menu_item", { itemId, shiftPressed: false });
      } catch (e) {
        error("Failed to stop speech recording: " + e);
      }
      await closeMenu();
    } else if (!_isRecording) {
      _isRecording = true;
      _recordingSkillId = null;
      try {
        await invoke("execute_menu_item", { itemId, shiftPressed: false });
      } catch (e) {
        error("Failed to start speech recording: " + e);
        clearRecordingState();
      }
      await refreshItems();
    }
  }
}

async function runInDialog(index: number, skillId: string, skillName: string) {
  let text: string;
  try {
    text = await invoke<string>("get_clipboard_text");
  } catch (e) {
    warn(`Run in dialog: clipboard read failed for ${skillId}: ${e}`);
    await openDialogForItem(index);
    return;
  }
  const skillModel = await fetchSkillModel(skillId);
  await closeMenu(false);
  try {
    await openConversationDialog(skillId, skillName, undefined, text, true, false, skillModel, "alternative_execute");
  } catch (e) {
    error(`Run in dialog: failed to open dialog for ${skillId}: ${e}`);
  }
}

async function executeSelected(shiftPressed: boolean = false) {
  if (_selectedIndex >= 0 && _selectedIndex < _items.length) {
    await executeItem(_selectedIndex, shiftPressed);
  }
}

async function startAlternativeExecution(index: number) {
  const item = _items[index];
  if (!item || item.item_type !== "skill") return;
  const data = item.data as { skill_id: string; skill_name: string } | null;
  if (!data?.skill_id) return;

  const skillId = data.skill_id;
  const itemId = item.id;
  const isRecordingThis = _isRecording && _recordingSkillId === skillId;
  if (!item.enabled && !isRecordingThis) return;

  if (isRecordingThis) {
    clearRecordingState();
    try {
      await invoke("execute_menu_item", {
        itemId,
        shiftPressed: true,
      });
    } catch (e) {
      error("Failed to stop speech recording for prompt: " + e);
    }
    await closeMenu();
    return;
  }

  _isRecording = true;
  _recordingSkillId = skillId;
  try {
    await invoke("execute_menu_item", {
      itemId,
      shiftPressed: true,
    });
  } catch (e) {
    error("Failed to start speech recording for prompt: " + e);
    clearRecordingState();
  }
  await refreshItems();
}

async function readDigitDelayMs(): Promise<number> {
  try {
    return (await getSettings()).menu_digit_select_delay_ms ?? DEFAULT_DIGIT_DELAY_MS;
  } catch (e) {
    warn("Failed to read menu_digit_select_delay_ms, using default: " + e);
    return DEFAULT_DIGIT_DELAY_MS;
  }
}

function runNumberedItem(num: number) {
  if (num >= 1 && num <= _allSkillItems.length) {
    const targetItem = _allSkillItems[num - 1];
    if (!targetItem.enabled && !(_isRecording && _recordingSkillId === (targetItem.data as { skill_id: string } | null)?.skill_id)) return;
    const targetIndex = _items.indexOf(targetItem);
    _selectedIndex = targetIndex;
    executeItem(targetIndex, digitIsAlternative);
  }
}

function handleNumberInput(digit: string, isAlternative: boolean) {
  digitIsAlternative = isAlternative;
  digitSelector.push(digit);
}

function getAllSkillItems(): MenuItem[] {
  return _allSkillItems;
}

async function refreshItems() {
  if (!_visible) return;
  try {
    await fetchRecordingState();
    const fetched = await invoke<MenuItem[]>("get_context_menu_items");
    _items = applyItemStates(fetched);
  } catch (e) {
    error("Failed to refresh context menu: " + e);
  }
}

async function init() {
  const win = getCurrentWebviewWindow();
  unlistens.push(await win.listen<{
    cursor_x: number;
    cursor_y: number;
    work_x: number;
    work_y: number;
    work_width: number;
    work_height: number;
    shell_placement: boolean;
  }>("show-context-menu", (event) => {
    const p = event.payload;
    openMenu({
      cursorX: p.cursor_x,
      cursorY: p.cursor_y,
      workX: p.work_x,
      workY: p.work_y,
      workWidth: p.work_width,
      workHeight: p.work_height,
      shellPlacement: p.shell_placement,
    });
  }));
  unlistens.push(await onContextChanged(() => refreshItems()));
  unlistens.push(await listen("execution-completed", () => refreshItems()));
  unlistens.push(await onHistoryChanged(() => refreshItems()));
  unlistens.push(
    await listen("speech-recording-stopped", () => {
      _isRecording = false;
      _recordingSkillId = null;
      _isTranscribing = true;
      refreshItems();
    }),
  );
  unlistens.push(
    await listen("speech-transcription-complete", () => {
      clearRecordingState();
      refreshItems();
    }),
  );
  unlistens.push(
    await listen("speech-transcription-error", () => {
      clearRecordingState();
      refreshItems();
    }),
  );
  unlistens.push(await listen("speech-transcription-retry", () => refreshItems()));

  const prefetched = await invoke<MenuItem[]>("get_context_menu_items");
  _items = prefetched;
}

function destroy() {
  for (const fn of unlistens) fn();
  unlistens = [];
}

async function fetchSkillModel(skillId: string): Promise<string | null> {
  await fetchSkillMetadata(skillId);
  return getSkillMeta(skillId)?.model ?? null;
}

async function openDialogForItem(index: number) {
  const item = _items[index];
  if (!item || item.item_type !== "skill") return;
  const data = item.data as { skill_id: string; skill_name: string } | null;
  if (!data?.skill_id) return;
  const skillId = data.skill_id;
  const skillName = data.skill_name ?? item.label;
  const skillModel = await fetchSkillModel(skillId);
  await closeMenu(false);
  await openConversationDialog(skillId, skillName, undefined, undefined, undefined, undefined, skillModel);
}

export {
  getItems,
  getSelectedIndex,
  setSelectedIndex,
  isVisible,
  isRecording,
  getRecordingSkillId,
  getWorkArea,
  isFirstPlacementPending,
  clearFirstPlacementPending,
  getOpenTrigger,
  openMenu,
  closeMenu,
  suppressClose,
  isSuppressed,
  resumeClose,
  isInBlurGrace,
  getCapture,
  captureScreenshot,
  toggleSelection,
  moveSelection,
  executeItem,
  executeSelected,
  startAlternativeExecution,
  handleNumberInput,
  getAllSkillItems,
  openDialogForItem,
  refreshItems,
  init,
  destroy,
};
