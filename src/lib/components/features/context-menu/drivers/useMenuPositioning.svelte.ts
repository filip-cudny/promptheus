import { invoke } from "@tauri-apps/api/core";
import { tick } from "svelte";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { debug as logDebug, warn as logWarn } from "@tauri-apps/plugin-log";
import {
  menuWindowHeight,
  closedMenuHeight,
  closedOffset,
  offsetAfterToggle,
  menuInputRect,
} from "$lib/utils/menuLayout";
import { isFirstPlacementPending, clearFirstPlacementPending } from "$lib/stores/contextMenu.svelte";

interface WorkArea {
  cursorX: number;
  cursorY: number;
  workX: number;
  workY: number;
  workWidth: number;
  workHeight: number;
  shellPlacement: boolean;
}

const MENU_WIDTH = 320;

type Opts = {
  getMenuEl: () => HTMLDivElement | undefined;
  isVisible: () => boolean;
  isListOpen: () => boolean;
  getWorkArea: () => WorkArea | null;
};

function clampOrigin(wa: WorkArea, anchorX: number, anchorY: number, height: number) {
  return invoke<[number, number]>("clamp_menu_origin", {
    anchorX,
    anchorY,
    width: MENU_WIDTH,
    height,
    workX: wa.workX,
    workY: wa.workY,
    workWidth: wa.workWidth,
    workHeight: wa.workHeight,
  });
}

export function useMenuPositioning(opts: Opts) {
  let hoverEnabled = $state(false);
  let resizeGeneration = 0;
  let placedOrigin: [number, number] | null = null;
  let menuOffset = $state(0);
  let placedHeight = 0;

  function getSkillsAnchorOffset(): number {
    const menuEl = opts.getMenuEl();
    if (!menuEl) return 0;
    const anchor = menuEl.querySelector("[data-section='skills-anchor']");
    if (!anchor) return 0;
    return Math.round(anchor.getBoundingClientRect().top - menuEl.getBoundingClientRect().top);
  }

  function isShellPlacement(): boolean {
    return opts.getWorkArea()?.shellPlacement === true;
  }

  function listBlockHeight(menuEl: HTMLElement): number {
    const probe = menuEl.querySelector("[data-measure='recent-list-block']");
    return probe ? Math.ceil(probe.getBoundingClientRect().height) : 0;
  }

  function windowHeight(menuEl: HTMLElement): number {
    const menuHeight = menuEl.scrollHeight + 2;
    if (!isShellPlacement()) return menuHeight;
    const block = listBlockHeight(menuEl);
    return menuWindowHeight(closedMenuHeight(menuHeight, opts.isListOpen(), block), block);
  }

  async function sendInputRegion() {
    const menuEl = opts.getMenuEl();
    if (!menuEl) return;
    const shellRect = menuEl.getBoundingClientRect();
    const panels = [...document.querySelectorAll(".floating-panel")].map((el) => {
      const rect = el.getBoundingClientRect();
      return { top: rect.top, bottom: rect.bottom };
    });
    const rect = menuInputRect(shellRect.top, shellRect.height, window.innerWidth, panels);
    try {
      await invoke("set_context_menu_input_region", rect);
    } catch (err) {
      logWarn(`[ctx-menu] setting the input region failed: ${err}`);
    }
  }

  async function triggerReposition() {
    const gen = ++resizeGeneration;
    await tick();
    if (gen !== resizeGeneration) return;
    const menuEl = opts.getMenuEl();
    if (!menuEl || !opts.isVisible()) return;

    let height = windowHeight(menuEl);
    const win = getCurrentWebviewWindow();
    const wa = opts.getWorkArea();
    let x = 0, y = 0;

    async function positionFromHeight(h: number) {
      if (!wa) return;
      const anchorY = wa.cursorY - getSkillsAnchorOffset();
      [x, y] = await clampOrigin(wa, wa.cursorX, anchorY, h);
      if (gen !== resizeGeneration) return;
      placedOrigin = [x, y];
      if (wa.shellPlacement === true) {
        menuOffset = opts.isListOpen() ? 0 : closedOffset(anchorY, y, listBlockHeight(menuEl!));
      }
    }

    async function placeThroughShell() {
      placedHeight = height;
      await tick();
      await sendInputRegion();
      if (gen !== resizeGeneration || !opts.isVisible()) return;
      const first = isFirstPlacementPending();
      const placement = invoke("place_context_menu", { x, y, first });
      if (first) clearFirstPlacementPending();
      await placement;
      if (gen !== resizeGeneration || !opts.isVisible()) return;

      const correctedHeight = windowHeight(menuEl!);
      if (correctedHeight !== height) {
        height = correctedHeight;
        await positionFromHeight(height);
        if (gen !== resizeGeneration || !opts.isVisible()) return;
        await win.setSize(new LogicalSize(MENU_WIDTH, height));
        if (gen !== resizeGeneration || !opts.isVisible()) return;
        placedHeight = height;
        await tick();
        await sendInputRegion();
        if (gen !== resizeGeneration || !opts.isVisible()) return;
        await invoke("place_context_menu", { x, y, first: false });
      }
      logDebug(`[ctx-menu] placed through shell at (${x}, ${y}), size ${MENU_WIDTH}x${height}`);
    }

    await positionFromHeight(height);
    if (gen !== resizeGeneration || !opts.isVisible()) return;
    hoverEnabled = false;
    await win.setSize(new LogicalSize(MENU_WIDTH, height));
    if (gen !== resizeGeneration || !opts.isVisible()) return;

    if (wa?.shellPlacement === true) {
      await placeThroughShell();
      return;
    }

    if (wa) {
      await win.setPosition(new LogicalPosition(x, y));
      if (gen !== resizeGeneration || !opts.isVisible()) return;
    }
    await invoke("show_context_menu_panel");
    if (gen !== resizeGeneration || !opts.isVisible()) return;

    const correctedHeight = windowHeight(menuEl);
    if (correctedHeight !== height) {
      height = correctedHeight;
      await positionFromHeight(height);
      if (gen !== resizeGeneration || !opts.isVisible()) return;
      await win.setSize(new LogicalSize(MENU_WIDTH, height));
      if (gen !== resizeGeneration || !opts.isVisible()) return;
      if (wa) {
        await win.setPosition(new LogicalPosition(x, y));
        if (gen !== resizeGeneration || !opts.isVisible()) return;
      }
    }

    await invoke("focus_context_menu");
    logDebug(`[ctx-menu] opened at (${x}, ${y}), size ${MENU_WIDTH}x${height}`);
  }

  async function resizeKeepingTop() {
    const gen = ++resizeGeneration;
    await tick();
    if (gen !== resizeGeneration) return;
    const menuEl = opts.getMenuEl();
    const wa = opts.getWorkArea();
    if (!menuEl || !opts.isVisible() || !wa || !placedOrigin) return;

    const height = windowHeight(menuEl);
    const [x, y] = await clampOrigin(wa, placedOrigin[0], placedOrigin[1], height);
    if (gen !== resizeGeneration || !opts.isVisible()) return;
    placedOrigin = [x, y];

    const win = getCurrentWebviewWindow();
    await win.setSize(new LogicalSize(MENU_WIDTH, height));
    if (gen !== resizeGeneration || !opts.isVisible()) return;
    if (wa.shellPlacement) {
      placedHeight = height;
      await tick();
      await sendInputRegion();
      if (gen !== resizeGeneration || !opts.isVisible()) return;
      await invoke("place_context_menu", { x, y, first: false });
    } else {
      await win.setPosition(new LogicalPosition(x, y));
    }
    logDebug(`[ctx-menu] resized keeping top at (${x}, ${y}), size ${MENU_WIDTH}x${height}`);
  }

  async function refresh() {
    if (!isShellPlacement()) {
      if (opts.isListOpen()) await resizeKeepingTop();
      else await triggerReposition();
      return;
    }
    await tick();
    const menuEl = opts.getMenuEl();
    if (!menuEl || !opts.isVisible()) return;
    if (windowHeight(menuEl) === placedHeight) {
      await sendInputRegion();
      return;
    }
    if (opts.isListOpen()) await resizeKeepingTop();
    else await triggerReposition();
  }

  async function toggleList(open: boolean) {
    if (!isShellPlacement()) {
      await resizeKeepingTop();
      return;
    }
    menuOffset = offsetAfterToggle(menuOffset, open);
    await tick();
    await sendInputRegion();
    const menuHeight = (opts.getMenuEl()?.scrollHeight ?? 0) + 2;
    logDebug(`[ctx-menu] list toggled open=${open}, menu ${menuHeight} px, window ${placedHeight} px, offset ${menuOffset}`);
  }

  async function updateInputRegion() {
    if (!isShellPlacement()) return;
    await tick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (!opts.isVisible()) return;
    await sendInputRegion();
  }

  return {
    get hoverEnabled() { return hoverEnabled; },
    enableHover() {
      if (!hoverEnabled) hoverEnabled = true;
    },
    get menuOffset() { return menuOffset; },
    triggerReposition,
    refresh,
    toggleList,
    updateInputRegion,
  };
}

export type MenuPositioning = ReturnType<typeof useMenuPositioning>;
