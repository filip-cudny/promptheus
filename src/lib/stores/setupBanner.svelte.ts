import { error as logError } from "@tauri-apps/plugin-log";
import type { ModelType, SurfaceKind } from "$lib/types";
import { onSettingsChanged } from "$lib/services/events";
import { getEnvVars, testModel } from "$lib/services/settings";
import {
  getSettingsSyncStatus,
  onSettingsSyncStatusChanged,
  type SyncState,
} from "$lib/services/settingsSync";
import { assignSetupModel, finishSetup, getSetupStatus } from "$lib/services/setup";
import { setUiState } from "$lib/services/uiState";
import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
import { getSettingsStore } from "$lib/stores/settings.svelte";
import { actionShortcuts, formatShortcut } from "$lib/utils/keymapShortcuts";
import { isMac } from "$lib/utils/platform";
import {
  SETUP_STEP_COUNT,
  clampStep,
  createdModelPassed,
  setupStep,
  testNote,
  testableModel,
  type SetupStep,
} from "$lib/utils/setupSteps";

const STEP_KEY = "settings-dialog.setup.step";

const ASSIGNMENT_MESSAGES: Partial<Record<SurfaceKind, string>> = {
  chat: "Assigned to Chat (Quick actions and Titles follow it); change in Models → Assignments.",
  speech_to_text: "Assigned to Speech; change in Models → Assignments.",
};

const settingsStore = getSettingsStore();
const navigation = getSettingsNavigation();

let active = $state(false);
let step = $state(1);
let textPassed = $state(false);
let speechPassed = $state(false);
let testing = $state(false);
let note = $state<string | null>(null);
let assignmentMessage = $state<string | null>(null);
let syncState = $state<SyncState | null>(null);
let envSet = $state.raw<Set<string> | null>(null);
let initialApplied = false;
let loading: Promise<void> | null = null;
let unlisteners: Array<() => void> = [];

const current = $derived.by<SetupStep | null>(() => {
  const settings = settingsStore.settings;
  if (!settings || !envSet) return null;
  const owned = envSet;
  const os = isMac ? "macos" : "linux";
  const shortcut = actionShortcuts(settings.keymaps, "open_context_menu", os)[0];
  return setupStep(step, {
    surfaces: settings.surfaces,
    models: settings.models,
    isEnvSet: (name) => owned.has(name),
    syncState,
    textPassed,
    speechPassed,
    contextMenuShortcut: shortcut ? formatShortcut(shortcut) : null,
  });
});

const testTarget = $derived.by(() => {
  const settings = settingsStore.settings;
  return settings ? testableModel(step, settings.surfaces, settings.models) : null;
});

async function refreshEnv() {
  try {
    const vars = await getEnvVars();
    envSet = new Set(vars.filter((v) => v.set).map((v) => v.name));
  } catch (e) {
    logError("Setup banner: failed to read the environment variables: " + e);
    envSet ??= new Set();
  }
}

async function refreshSync() {
  try {
    syncState = (await getSettingsSyncStatus()).state;
  } catch (e) {
    logError("Setup banner: failed to read the sync status: " + e);
  }
}

async function load() {
  try {
    const status = await getSetupStatus();
    step = clampStep(status.step);
    active = status.active;
  } catch (e) {
    logError("Setup banner: failed to read the setup status: " + e);
    return;
  }
  if (!active) return;
  unlisteners.push(
    await onSettingsChanged(refreshEnv),
    await onSettingsSyncStatusChanged((status) => (syncState = status.state)),
  );
  await Promise.all([refreshEnv(), refreshSync()]);
}

function init() {
  loading ??= load();
  return loading;
}

function destroy() {
  for (const unlisten of unlisteners) unlisten();
  unlisteners = [];
}

function applyStep() {
  if (!current) return;
  navigation.setPersistentTargets(current.highlight);
  void navigation.navigateTo(current.navigate);
}

function applyInitial() {
  if (initialApplied || !active || !current) return;
  initialApplied = true;
  applyStep();
}

async function saveStep() {
  try {
    await setUiState(STEP_KEY, step);
  } catch (e) {
    logError("Setup banner: failed to save the step: " + e);
  }
}

async function goTo(next: number) {
  step = clampStep(next);
  note = null;
  assignmentMessage = null;
  applyStep();
  await saveStep();
}

function back() {
  return goTo(step - 1);
}

function skip() {
  return goTo(step + 1);
}

function next() {
  if (!current?.canNext) return Promise.resolve();
  return goTo(step + 1);
}

async function finish() {
  active = false;
  navigation.clearPersistentTargets();
  try {
    await finishSetup();
  } catch (e) {
    logError("Setup banner: failed to finish the setup: " + e);
  }
}

async function runTest() {
  const model = testTarget;
  if (!model || testing) return;
  const type = model.type;
  testing = true;
  note = null;
  try {
    const result = await testModel(model);
    note = testNote(result);
    if (result.ok) markPassed(type);
  } catch (e) {
    note = String(e);
    logError("Setup banner: model test failed: " + e);
  } finally {
    testing = false;
  }
}

function markPassed(type: ModelType) {
  if (type === "text") textPassed = true;
  else speechPassed = true;
}

async function modelCreated(id: string, testPassed: boolean) {
  if (!active) return;
  let surface: SurfaceKind | null;
  try {
    surface = await assignSetupModel(id);
  } catch (e) {
    logError("Setup banner: failed to assign the created model: " + e);
    return;
  }
  await settingsStore.refresh();
  assignmentMessage = surface ? (ASSIGNMENT_MESSAGES[surface] ?? null) : null;
  const surfaces = settingsStore.settings?.surfaces;
  const type = settingsStore.getModel(id)?.type;
  if (surfaces && type && createdModelPassed(type, id, testPassed, surfaces)) markPassed(type);
}

export function getSetupBanner() {
  return {
    get active() { return active; },
    get step() { return step; },
    get stepCount() { return SETUP_STEP_COUNT; },
    get current() { return current; },
    get testing() { return testing; },
    get canTest() { return testTarget !== null && !testing; },
    get note() { return note; },
    get assignmentMessage() { return assignmentMessage; },
    init,
    destroy,
    applyInitial,
    back,
    skip,
    next,
    finish,
    runTest,
    modelCreated,
  };
}
