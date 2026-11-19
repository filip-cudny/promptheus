import { invoke } from "@tauri-apps/api/core";
import { tick } from "svelte";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { debug as logDebug } from "@tauri-apps/plugin-log";
import { ctxMenuDiagMark, ctxMenuDiagSwitches } from "./ctxMenuDiag";
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

  function getSkillsAnchorOffset(): number {
    const menuEl = opts.getMenuEl();
    if (!menuEl) return 0;
    const anchor = menuEl.querySelector("[data-section='skills-anchor']");
    if (!anchor) return 0;
    return (anchor as HTMLElement).offsetTop;
  }

  async function triggerReposition() {
    const gen = ++resizeGeneration;
    await tick();
    if (gen !== resizeGeneration) return;
    const menuEl = opts.getMenuEl();
    if (!menuEl || !opts.isVisible()) return;

    let height = menuEl.scrollHeight + 2;
    const win = getCurrentWebviewWindow();
    const wa = opts.getWorkArea();
    let x = 0, y = 0;

    async function positionFromHeight(h: number) {
      if (!wa) return;
      [x, y] = await clampOrigin(wa, wa.cursorX, wa.cursorY - getSkillsAnchorOffset(), h);
      if (gen === resizeGeneration) placedOrigin = [x, y];
    }

    async function placeThroughShell() {
      const first = isFirstPlacementPending();
      const placement = invoke("place_context_menu", { x, y, first });
      if (first) clearFirstPlacementPending();
      await placement;
      if (gen !== resizeGeneration || !opts.isVisible()) return;

      const correctedHeight = menuEl!.scrollHeight + 2;
      if (correctedHeight !== height) {
        height = correctedHeight;
        await positionFromHeight(height);
        if (gen !== resizeGeneration || !opts.isVisible()) return;
        await win.setSize(new LogicalSize(MENU_WIDTH, height));
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

    const correctedHeight = menuEl.scrollHeight + 2;
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

    const height = menuEl.scrollHeight + 2;
    ctxMenuDiagMark("after-tick", {
      scrollHeight: menuEl.scrollHeight,
      innerHeight: window.innerHeight,
      placedOriginX: placedOrigin[0],
      placedOriginY: placedOrigin[1],
      bodyScrollTop: document.body.scrollTop,
    });
    const [x, y] = await clampOrigin(wa, placedOrigin[0], placedOrigin[1], height);
    ctxMenuDiagMark("clamp", { x, y, height });
    if (gen !== resizeGeneration || !opts.isVisible()) return;
    placedOrigin = [x, y];

    const win = getCurrentWebviewWindow();
    if (ctxMenuDiagSwitches().delayResize) {
      await new Promise((resolve) => setTimeout(resolve, 500));
      if (gen !== resizeGeneration || !opts.isVisible()) return;
      ctxMenuDiagMark("delay-done");
    }
    ctxMenuDiagMark("setSize-start");
    await win.setSize(new LogicalSize(MENU_WIDTH, height));
    ctxMenuDiagMark("setSize-done");
    if (gen !== resizeGeneration || !opts.isVisible()) return;
    if (wa.shellPlacement) {
      ctxMenuDiagMark("place-start");
      await invoke("place_context_menu", { x, y, first: false });
      ctxMenuDiagMark("place-done");
    } else {
      ctxMenuDiagMark("setPosition-start");
      await win.setPosition(new LogicalPosition(x, y));
      ctxMenuDiagMark("setPosition-done");
    }
    logDebug(`[ctx-menu] resized keeping top at (${x}, ${y}), size ${MENU_WIDTH}x${height}`);
  }

  return {
    get hoverEnabled() { return hoverEnabled; },
    enableHover() {
      if (!hoverEnabled) hoverEnabled = true;
    },
    triggerReposition,
    resizeKeepingTop,
  };
}

export type MenuPositioning = ReturnType<typeof useMenuPositioning>;
