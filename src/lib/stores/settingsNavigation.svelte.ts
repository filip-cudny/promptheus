import { tick } from "svelte";
import { warn } from "@tauri-apps/plugin-log";
import type { ModelType, SurfaceKind } from "$lib/types";
import type { SettingsSection } from "$lib/components/features/settings/SettingsSidebar.svelte";
import { getSettingsCatalogStore } from "$lib/stores/settingsCatalog.svelte";
import { isSectionKey, parseSectionLink } from "$lib/utils/settingLinks";
import { catalogPath, listItemOf, type ListItemRef } from "$lib/utils/settingsSearch";
import {
  isAssignmentKey,
  isEnvKey,
  parseAssignmentLink,
  parseEnvLink,
} from "$lib/utils/settingsCrossLinks";

const HIGHLIGHT_MS = 2000;
const ELEMENT_WAIT_FRAMES = 20;
const NEW_MODEL_TYPES = new Map<string, ModelType>([
  ["models:new", "text"],
  ["models:new-stt", "stt"],
]);
const PERSISTENT_STYLE_ID = "settings-persistent-targets";
const PERSISTENT_TARGET_STYLE =
  "{ border-radius: var(--radius-md); outline: 2px solid var(--accent); outline-offset: 4px; background: var(--accent-bg-soft); }";

let activeSection = $state<SettingsSection>("models");
let selectedItem = $state.raw<ListItemRef | null>(null);
let createModelType = $state<ModelType | null>(null);
let highlighted = $state<string | null>(null);
let highlightedSection = $state<SettingsSection | null>(null);
let highlightTimer: ReturnType<typeof setTimeout> | undefined;
let sectionHighlightTimer: ReturnType<typeof setTimeout> | undefined;
let highlightedAssignment = $state<SurfaceKind | null>(null);
let highlightedEnv = $state<string | null>(null);
let assignmentTimer: ReturnType<typeof setTimeout> | undefined;
let envTimer: ReturnType<typeof setTimeout> | undefined;
let persistentTargets = $state.raw<string[]>([]);

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

async function waitForElement(id: string): Promise<HTMLElement | null> {
  await tick();
  for (let i = 0; i < ELEMENT_WAIT_FRAMES; i++) {
    const el = document.getElementById(id);
    if (el) return el;
    await nextFrame();
  }
  return null;
}

async function navigateToSection(key: string) {
  const section = parseSectionLink(key);
  if (!section) {
    warn(`settings navigation: unknown section ${key.slice("section:".length)}`);
    return;
  }
  activeSection = section;
  await tick();
  scrollContentToTop();
  highlightedSection = section;
  clearTimeout(sectionHighlightTimer);
  sectionHighlightTimer = setTimeout(() => (highlightedSection = null), HIGHLIGHT_MS);
}

function scrollContentToTop() {
  const content = document.querySelector<HTMLElement>("[data-settings-content]");
  if (!content) return;
  for (const el of [content, ...content.querySelectorAll<HTMLElement>("*")]) {
    if (el.scrollTop > 0) el.scrollTo({ top: 0 });
  }
}

async function navigateToAssignment(key: string) {
  const surface = parseAssignmentLink(key);
  if (!surface) {
    warn(`settings navigation: unknown assignment ${key}`);
    return;
  }
  activeSection = "models";
  highlightedAssignment = null;
  await tick();
  highlightedAssignment = surface;
  clearTimeout(assignmentTimer);
  assignmentTimer = setTimeout(() => (highlightedAssignment = null), HIGHLIGHT_MS);
  const el = await waitForElement(`assignment-${surface}`);
  if (!el) {
    warn(`settings navigation: row assignment-${surface} did not render for ${key}`);
    return;
  }
  el.scrollIntoView({ block: "center" });
}

async function navigateToEnv(key: string) {
  const name = parseEnvLink(key);
  if (!name) {
    warn(`settings navigation: no variable name in ${key}`);
    return;
  }
  activeSection = "environment";
  const el = await waitForElement(`env-var-${name}`);
  if (!el) {
    warn(`settings navigation: row env-var-${name} did not render for ${key}`);
    return;
  }
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  highlightedEnv = name;
  clearTimeout(envTimer);
  envTimer = setTimeout(() => (highlightedEnv = null), HIGHLIGHT_MS);
}

function renderPersistentTargetStyle() {
  document.getElementById(PERSISTENT_STYLE_ID)?.remove();
  if (persistentTargets.length === 0) return;
  const selectors = persistentTargets.map((id) => `[data-setting-target="${CSS.escape(id)}"]`);
  const style = document.createElement("style");
  style.id = PERSISTENT_STYLE_ID;
  style.textContent = `${selectors.join(", ")} ${PERSISTENT_TARGET_STYLE}`;
  document.head.append(style);
}

function setPersistentTargets(ids: string[]) {
  persistentTargets = [...ids];
  renderPersistentTargetStyle();
}

function clearPersistentTargets() {
  setPersistentTargets([]);
}

function isPersistentTarget(id: string): boolean {
  return persistentTargets.includes(id);
}

async function navigateTo(key: string) {
  const newModelType = NEW_MODEL_TYPES.get(key);
  if (newModelType) {
    activeSection = "models";
    createModelType = newModelType;
    return;
  }
  if (isSectionKey(key)) {
    await navigateToSection(key);
    return;
  }
  if (isAssignmentKey(key)) {
    await navigateToAssignment(key);
    return;
  }
  if (isEnvKey(key)) {
    await navigateToEnv(key);
    return;
  }
  const path = catalogPath(key);
  const entry = getSettingsCatalogStore().entry(path);
  const skill = listItemOf(key);
  if (!entry && skill?.list === "skills") {
    activeSection = "skills";
    selectedItem = { ...skill };
    await tick();
    selectedItem = null;
    return;
  }
  if (!entry) {
    warn(`settings navigation: no catalog entry for ${key}`);
    return;
  }
  activeSection = entry.section;
  const item = listItemOf(key);
  selectedItem = item ? { ...item } : null;
  const el = await waitForElement(`setting-${path}`);
  selectedItem = null;
  if (!el) {
    warn(`settings navigation: field setting-${path} did not render for ${key}`);
    return;
  }
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  highlighted = key;
  clearTimeout(highlightTimer);
  highlightTimer = setTimeout(() => (highlighted = null), HIGHLIGHT_MS);
}

export function getSettingsNavigation() {
  return {
    get activeSection() {
      return activeSection;
    },
    set activeSection(section: SettingsSection) {
      activeSection = section;
    },
    get selectedItem() {
      return selectedItem;
    },
    get createModelType() {
      return createModelType;
    },
    clearCreateModelType() {
      createModelType = null;
    },
    get highlighted() {
      return highlighted;
    },
    get highlightedSection() {
      return highlightedSection;
    },
    get highlightedAssignment() {
      return highlightedAssignment;
    },
    get highlightedEnv() {
      return highlightedEnv;
    },
    get persistentTargets() {
      return persistentTargets;
    },
    setPersistentTargets,
    clearPersistentTargets,
    isPersistentTarget,
    navigateTo,
  };
}
