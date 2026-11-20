<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { prefersReducedMotion } from "svelte/motion";
  import { fade } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { attachConsole, error as logError } from "@tauri-apps/plugin-log";
  import {
    ArrowUp,
    Check,
    ChevronsUpDown,
    ClipboardPaste,
    Image as ImageIcon,
    MessageSquareShare,
    Pause,
    Play,
    Type,
    X,
  } from "lucide-svelte";
  import { ScanSquare, TextSelect } from "lucide-static";
  import { isMac } from "$lib/utils/platform";
  import claudeIcon from "$lib/icons/providers/claude.svg?raw";
  import { PADDING_X, PILL_WIDTH, RADIUS, dropdownSeparatorIndex, run, type PillItem, type PillModel } from "./pill";

  type WidgetState = "recording" | "paused" | "processing" | "retrying" | "done";

  interface WidgetPayload {
    state: WidgetState;
    level: number;
    elapsedMs: number;
    monochromatic: boolean;
    attempt?: number;
    maxAttempts?: number;
    delayMs?: number;
  }

  type PopupKind = keyof typeof POPUP_COMMANDS;

  const PILL_HEIGHT = 72;
  const MAX_ITEM_ROWS = 3;
  const ITEM_ROW_HEIGHT = 24;
  const ITEM_ROW_GAP = 4;
  const ITEMS_ROW_PADDING = 10;
  const PREVIEW_DELAY_MS = 300;
  const TOOLTIP_DELAY_MS = 700;
  const TOOLTIP_CHAIN_MS = 500;
  const ACTION_ICON_SIZE = 16;
  const POPUP_COMMANDS = { preview: "set_recording_widget_preview", tooltip: "set_recording_widget_tooltip" } as const;
  const ROW_HEIGHT = PILL_HEIGHT / 2;
  const CHIP_GAP = 4;
  const IMAGE_CHIP_GAP = 5;
  const IMAGE_COLUMNS = 3;
  const DEFAULT_TARGET_LABEL = "Dictation";
  const BAR_WIDTH = 2;
  const BAR_GAP = 3;
  const TIME_WIDTH = 34;
  const ROW_GAP = 10;
  const BUTTON_SIZE = 28;
  const ACTION_GAP = 4;
  const ROW1_BUTTONS = 3;
  const METER_WIDTH =
    PILL_WIDTH - 2 * PADDING_X - TIME_WIDTH - 2 * ROW_GAP - ROW1_BUTTONS * BUTTON_SIZE - (ROW1_BUTTONS - 1) * ACTION_GAP;
  const BAR_COUNT = Math.floor((METER_WIDTH + BAR_GAP) / (BAR_WIDTH + BAR_GAP));
  const BAR_PATTERN = [0.5, 0.8, 0.65, 1, 0.7, 0.9, 0.55, 0.95, 0.75, 0.85, 0.6, 0.45, 0.8, 0.95, 0.6, 0.7, 1, 0.55];
  const BAR_FACTORS = Array.from({ length: BAR_COUNT }, (_, i) => BAR_PATTERN[i % BAR_PATTERN.length]);
  const MIN_BAR = 0.12;
  const DONE_COLOR = "#62A878";
  const WAVE_BAR_WIDTH = 2;
  const WAVE_BAR_COUNT = Math.floor((PILL_WIDTH - 2 * PADDING_X + BAR_GAP) / (WAVE_BAR_WIDTH + BAR_GAP));
  const WAVE_PERIOD_MS = 1200;
  const RETRY_WAVE_BAR_COUNT = Math.floor(
    (PILL_WIDTH - 2 * PADDING_X - BUTTON_SIZE - ROW_GAP + BAR_GAP) / (WAVE_BAR_WIDTH + BAR_GAP),
  );
  const waveDelays = (count: number) => Array.from({ length: count }, (_, i) => (i / count - 1) * WAVE_PERIOD_MS);
  const WAVE_BARS = waveDelays(WAVE_BAR_COUNT);
  const RETRY_WAVE_BARS = waveDelays(RETRY_WAVE_BAR_COUNT);
  const SWAP_MS = 150;

  let widgetState = $state<WidgetState>("recording");
  let level = $state(0);
  let elapsedMs = $state(0);
  let monochromatic = $state(true);
  let retryAttempt = $state(0);
  let retryMaxAttempts = $state(0);
  let retryRemainingMs = $state(0);
  let retryEndAt = 0;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let model = $state<PillModel>({
    targets: [],
    selectedId: "",
    selectedLabel: DEFAULT_TARGET_LABEL,
    targetLocked: false,
    items: [],
    itemRows: 0,
    contextVersion: 0,
    stopChatDestination: "",
  });
  let dropdownOpen = $state(false);
  let hoveredIndex: number | null = null;
  let hoveredButton: HTMLButtonElement | null = null;
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let previewToken = 0;
  let previewRequested = false;
  let backendOverlay: PopupKind | null = null;
  let tooltipTarget: HTMLElement | null = null;
  let tooltipTimer: ReturnType<typeof setTimeout> | undefined;
  let tooltipToken = 0;
  let tooltipRequested = false;
  let tooltipHiddenAt = -Infinity;
  let previewQueue: Promise<void> = Promise.resolve();
  let unlisteners: UnlistenFn[] = [];

  const clock = $derived.by(() => {
    const totalSeconds = Math.floor(elapsedMs / 1000);
    const seconds = String(totalSeconds % 60).padStart(2, "0");
    return `${Math.floor(totalSeconds / 60)}:${seconds}`;
  });

  const interactive = $derived(!model.targetLocked && widgetState !== "done");
  const hasItems = $derived(model.items.length > 0);
  const imageLines = $derived.by(() => {
    const images = model.items.filter((item) => item.kind === "image");
    return Array.from({ length: Math.ceil(images.length / IMAGE_COLUMNS) }, (_, line) =>
      images.slice(line * IMAGE_COLUMNS, (line + 1) * IMAGE_COLUMNS),
    );
  });
  const textItems = $derived(model.items.filter((item) => item.kind === "text"));
  const pillHeight = $derived(PILL_HEIGHT + itemsHeight(model.itemRows));
  const separatorIndex = $derived(dropdownSeparatorIndex(model.targets));
  const hasSeparator = $derived(separatorIndex >= 0);
  const stopChatTooltip = $derived(
    model.stopChatDestination ? `Stop and send to ${model.stopChatDestination}` : "Stop and open chat",
  );

  function chipText(preview: string): string {
    return preview.replace(/\s+/g, " ").trim();
  }

  function itemsHeight(rows: number): number {
    if (rows <= 0) return 0;
    const clamped = Math.min(rows, MAX_ITEM_ROWS);
    return clamped * ITEM_ROW_HEIGHT + (clamped - 1) * ITEM_ROW_GAP + ITEMS_ROW_PADDING;
  }

  function tickRetryCountdown() {
    retryRemainingMs = Math.max(0, retryEndAt - performance.now());
    if (retryRemainingMs === 0) return;
    const untilTextChange = retryRemainingMs - (Math.ceil(retryRemainingMs / 1000) - 1) * 1000;
    retryTimer = setTimeout(tickRetryCountdown, untilTextChange);
  }

  function startRetryCountdown(delayMs: number) {
    retryEndAt = performance.now() + delayMs;
    tickRetryCountdown();
  }

  function swap(node: Element) {
    const instant = prefersReducedMotion.current || widgetState === "recording" || widgetState === "paused";
    return fade(node, { duration: instant ? 0 : SWAP_MS, easing: cubicOut });
  }

  function barScale(factor: number): number {
    return Math.max(MIN_BAR, Math.min(1, level * factor));
  }

  function runAction(command: string) {
    hideTooltip();
    return run(command);
  }

  let selecting = $state(false);

  async function toggleSelecting() {
    if (selecting) {
      await run("cancel_recording_widget_selection");
      return;
    }
    selecting = true;
    await runAction("capture_selection_into_recording_widget");
    selecting = false;
  }

  function enqueuePreviewCall(task: () => Promise<void>) {
    previewQueue = previewQueue.then(task);
  }

  async function openPopup(kind: PopupKind, args: Record<string, unknown>): Promise<boolean> {
    try {
      await invoke(POPUP_COMMANDS[kind], { open: true, ...args });
      backendOverlay = kind;
      return true;
    } catch (e) {
      logError(`${POPUP_COMMANDS[kind]} failed: ${e}`);
      return false;
    }
  }

  function closePopup(kind: PopupKind) {
    enqueuePreviewCall(async () => {
      if (backendOverlay !== kind) return;
      await run(POPUP_COMMANDS[kind], { open: false });
      backendOverlay = null;
    });
  }

  function resetPreviewState() {
    clearTimeout(hoverTimer);
    hoveredIndex = null;
    previewToken++;
    previewRequested = false;
    clearTimeout(tooltipTimer);
    tooltipTarget = null;
    tooltipToken++;
    tooltipRequested = false;
    backendOverlay = null;
  }

  function hidePreview() {
    clearTimeout(hoverTimer);
    hoveredIndex = null;
    previewToken++;
    if (!previewRequested) return;
    previewRequested = false;
    closePopup("preview");
  }

  function hideTooltip() {
    clearTimeout(tooltipTimer);
    tooltipToken++;
    if (!tooltipRequested) return;
    tooltipRequested = false;
    tooltipHiddenAt = performance.now();
    closePopup("tooltip");
  }

  function showTooltip(target: HTMLElement) {
    if (dropdownOpen) return;
    const token = ++tooltipToken;
    const text = target.dataset.tooltip ?? "";
    tooltipRequested = true;
    enqueuePreviewCall(async () => {
      if (token !== tooltipToken) return;
      if (!(await openPopup("tooltip", { text }))) tooltipRequested = false;
    });
  }

  function followTooltipTarget(target: HTMLElement | null) {
    if (target === tooltipTarget) return;
    tooltipTarget = target;
    hideTooltip();
    if (!target) return;
    const chained = performance.now() - tooltipHiddenAt <= TOOLTIP_CHAIN_MS;
    tooltipTimer = setTimeout(() => showTooltip(target), chained ? 0 : TOOLTIP_DELAY_MS);
  }

  function showPreview(index: number) {
    const item = model.items.find((candidate) => candidate.index === index);
    if (!item || dropdownOpen) return;
    const token = ++previewToken;
    previewRequested = true;
    enqueuePreviewCall(async () => {
      if (token !== previewToken) return;
      const text = item.kind === "text" ? item.preview : null;
      if (!(await openPopup("preview", { index: item.index, text }))) previewRequested = false;
    });
  }

  function onPointer(point: { x: number; y: number } | null) {
    const element = point ? document.elementFromPoint(point.x, point.y) : null;
    const button = element?.closest("button") ?? null;
    if (button !== hoveredButton) {
      hoveredButton?.classList.remove("hover");
      button?.classList.add("hover");
      hoveredButton = button;
    }
    followTooltipTarget(dropdownOpen ? null : (element?.closest<HTMLElement>("[data-tooltip]") ?? null));
    const chip = element?.closest<HTMLElement>("[data-item-index]");
    if (!chip) {
      hidePreview();
      return;
    }
    const index = Number(chip.dataset.itemIndex);
    if (index === hoveredIndex) return;
    clearTimeout(hoverTimer);
    hoveredIndex = index;
    hoverTimer = setTimeout(() => showPreview(index), PREVIEW_DELAY_MS);
  }

  async function openDropdown() {
    if (model.targets.length === 0) return;
    hidePreview();
    hideTooltip();
    try {
      await invoke("set_recording_widget_dropdown", {
        open: true,
        rows: model.targets.length,
        separator: hasSeparator,
      });
      dropdownOpen = true;
    } catch (e) {
      logError(`set_recording_widget_dropdown failed: ${e}`);
    }
  }

  async function closeDropdown() {
    dropdownOpen = false;
    await run("set_recording_widget_dropdown", { open: false, rows: 0, separator: false });
  }

  async function selectTarget(id: string) {
    await run("select_recording_widget_target", { id });
    await closeDropdown();
  }

  $effect(() => {
    if (!interactive && dropdownOpen) closeDropdown();
  });

  $effect(() => {
    if (interactive) return;
    tooltipTarget = null;
    hideTooltip();
  });

  onMount(async () => {
    attachConsole();
    unlisteners = await Promise.all([
      listen<WidgetPayload>("recording-widget-state", (event) => {
        clearTimeout(retryTimer);
        widgetState = event.payload.state;
        elapsedMs = event.payload.elapsedMs;
        monochromatic = event.payload.monochromatic;
        if (widgetState === "recording") level = event.payload.level;
        if (widgetState === "retrying") {
          retryAttempt = event.payload.attempt ?? 0;
          retryMaxAttempts = event.payload.maxAttempts ?? 0;
          startRetryCountdown(event.payload.delayMs ?? 0);
        }
      }),
      listen<PillModel>("recording-widget-model", (event) => {
        model = event.payload;
      }),
      listen<{ x: number; y: number } | null>("recording-widget-pointer", (event) => onPointer(event.payload)),
      listen("recording-widget-dropdown-closed", () => {
        dropdownOpen = false;
        resetPreviewState();
      }),
    ]);
  });

  onDestroy(() => {
    clearTimeout(retryTimer);
    unlisteners.forEach((unlisten) => unlisten());
  });
</script>

{#snippet removeButton(item: PillItem)}
  <button
    type="button"
    class="chip-remove"
    tabindex="-1"
    aria-label="Remove {item.label}"
    disabled={model.targetLocked}
    onclick={() => run("remove_recording_widget_item", { index: item.index, contextVersion: model.contextVersion })}
  >
    <X size={12} />
  </button>
{/snippet}

<div class="root" style:--radius="{RADIUS}px">
  <div class="pill" class:mac={isMac} style:height="{pillHeight}px" style:padding="0 {PADDING_X}px" data-tauri-drag-region>
    {#if hasItems}
      <div class="items" style:height="{itemsHeight(model.itemRows)}px" style:gap="{ITEM_ROW_GAP}px" data-tauri-drag-region>
        {#each imageLines as line}
          <div class="image-line" style:gap="{ITEM_ROW_GAP}px" data-tauri-drag-region>
            {#each line as item (item.index)}
              <span class="chip image" style:gap="{IMAGE_CHIP_GAP}px" data-item-index={item.index}>
                {#if item.thumbnail}
                  <span class="thumb"><img src={item.thumbnail} alt="" /></span>
                {:else}
                  <ImageIcon size={12} />
                {/if}
                <span class="chip-number">{item.label}</span>
                {@render removeButton(item)}
              </span>
            {/each}
          </div>
        {/each}
        {#each textItems as item (item.index)}
          <span class="chip text" style:gap="{CHIP_GAP}px" data-item-index={item.index}>
            <Type size={12} />
            <span class="chip-number">{item.label}</span>
            <span class="chip-text">{chipText(item.preview)}</span>
            {@render removeButton(item)}
          </span>
        {/each}
      </div>
    {/if}
    <div class="row stack" style:height="{ROW_HEIGHT}px" data-tauri-drag-region>
      {#if widgetState === "recording" || widgetState === "paused"}
        <div class="layer" out:swap data-tauri-drag-region>
          <div class="bars" class:paused={widgetState === "paused"} style:gap="{BAR_GAP}px" data-tauri-drag-region>
            {#each BAR_FACTORS as factor}
              <span class="bar" style:width="{BAR_WIDTH}px" style:transform="scaleY({barScale(factor)})" data-tauri-drag-region></span>
            {/each}
          </div>
          <span class="time" style:min-width="{TIME_WIDTH}px" data-tauri-drag-region>{clock}</span>
          <div class="actions">
            {#if widgetState === "recording"}
              <button type="button" tabindex="-1" aria-label="Pause" onclick={() => run("pause_speech_recording")}>
                <Pause size={16} />
              </button>
            {:else}
              <button type="button" tabindex="-1" aria-label="Resume" onclick={() => run("resume_speech_recording")}>
                <Play size={16} />
              </button>
            {/if}
            <button type="button" class="cancel" tabindex="-1" aria-label="Cancel" onclick={() => run("cancel_speech_recording")}>
              <X size={16} />
            </button>
            <button type="button" class="primary" tabindex="-1" aria-label="Finish" onclick={() => run("toggle_speech_recording")}>
              <ArrowUp size={16} strokeWidth={3} />
            </button>
          </div>
        </div>
      {:else if widgetState === "processing"}
        <div class="layer" transition:swap data-tauri-drag-region>
          <div class="wave" style:--wave-period="{WAVE_PERIOD_MS}ms" style:gap="{BAR_GAP}px" data-tauri-drag-region>
            {#each WAVE_BARS as delayMs}
              <span class="wave-bar" style:width="{WAVE_BAR_WIDTH}px" style:animation-delay="{delayMs}ms" data-tauri-drag-region></span>
            {/each}
          </div>
        </div>
      {:else if widgetState === "retrying"}
        <div class="layer" transition:swap data-tauri-drag-region>
          {#if retryRemainingMs > 0}
            <span class="time" data-tauri-drag-region>
              Retry {retryAttempt}/{retryMaxAttempts} in {Math.ceil(retryRemainingMs / 1000)} s
            </span>
          {:else}
            <div class="wave fixed" style:--wave-period="{WAVE_PERIOD_MS}ms" style:gap="{BAR_GAP}px" data-tauri-drag-region>
              {#each RETRY_WAVE_BARS as delayMs}
                <span class="wave-bar" style:width="{WAVE_BAR_WIDTH}px" style:animation-delay="{delayMs}ms" data-tauri-drag-region></span>
              {/each}
            </div>
          {/if}
          <div class="actions">
            <button type="button" class="cancel" tabindex="-1" aria-label="Cancel" onclick={() => run("cancel_speech_recording")}>
              <X size={16} />
            </button>
          </div>
        </div>
      {:else}
        <div class="layer" out:swap data-tauri-drag-region>
          <span class="status" data-tauri-drag-region>
            <Check size={16} color={monochromatic ? "currentColor" : DONE_COLOR} />
            Copied
          </span>
        </div>
      {/if}
    </div>
    <div class="row" class:locked={!interactive} style:height="{ROW_HEIGHT}px" data-tauri-drag-region>
      <button
        type="button"
        class="target"
        tabindex="-1"
        onclick={() => (selecting ? run("cancel_recording_widget_selection") : dropdownOpen ? closeDropdown() : openDropdown())}
      >
        <span class="label">{selecting && interactive ? "Select text…" : model.selectedLabel}</span>
        {#if !selecting}
          <ChevronsUpDown size={14} />
        {/if}
      </button>
      <div class="actions">
        {#if isMac}
          <button
            type="button"
            tabindex="-1"
            aria-label="Screenshot"
            data-tooltip="Add screen area to context"
            onclick={() => runAction("capture_screenshot_into_recording_widget")}
          >
            <span class="option-icon" style:--icon-size="{ACTION_ICON_SIZE}px" aria-hidden="true">{@html ScanSquare}</span>
          </button>
          <button
            type="button"
            class:selecting
            tabindex="-1"
            aria-label="Select text"
            data-tooltip="Add selected text to context"
            onclick={toggleSelecting}
          >
            <span class="option-icon" style:--icon-size="{ACTION_ICON_SIZE}px" aria-hidden="true">{@html TextSelect}</span>
          </button>
        {/if}
        <button
          type="button"
          tabindex="-1"
          aria-label="Paste"
          data-tooltip="Add clipboard to context"
          onclick={() => runAction("paste_into_recording_widget")}
        >
          <ClipboardPaste size={16} />
        </button>
        <button
          type="button"
          class="secondary"
          tabindex="-1"
          aria-label={stopChatTooltip}
          data-tooltip={stopChatTooltip}
          onclick={() => runAction("stop_chat_speech_recording")}
        >
          {#if model.stopChatDestination}
            <span class="option-icon" style:--icon-size="{ACTION_ICON_SIZE}px" aria-hidden="true">{@html claudeIcon}</span>
          {:else}
            <MessageSquareShare size={16} />
          {/if}
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
    width: 100%;
    height: 100%;
    font-family: var(--font-sans);
    user-select: none;
  }

  .root {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .pill {
    box-sizing: border-box;
    flex: none;
    border-radius: var(--radius);
    background: #1c1c1e;
    color: #e5e5e7;
    font-size: 13px;
  }

  .pill.mac {
    background: rgba(28, 28, 30, 0.92);
  }

  .pill {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }

  .items {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
    box-sizing: border-box;
    padding: 5px 0;
    overflow-y: auto;
    scrollbar-width: none;
  }

  .items::-webkit-scrollbar {
    display: none;
  }

  .image-line {
    display: flex;
  }

  .chip {
    display: flex;
    align-items: center;
    height: 24px;
    min-width: 0;
    padding: 0 6px;
    border-radius: var(--radius-chip);
    background: rgba(255, 255, 255, 0.12);
    font-size: 11px;
    white-space: nowrap;
  }

  .chip.image {
    padding: 0 4px 0 2px;
  }

  .thumb {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    overflow: hidden;
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.18);
  }

  .thumb img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .chip-number {
    flex: none;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #929294;
    font-size: 11.5px;
  }

  .chip-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #e5e5e7;
  }

  .chip-remove {
    flex: none;
    width: 16px;
    height: 16px;
  }

  .chip-remove:disabled {
    opacity: 0.35;
    pointer-events: none;
  }

  .target {
    flex: 0 1 auto;
    min-width: 0;
    width: auto;
    height: 28px;
    gap: 4px;
    margin-left: -8px;
    padding: 0 8px;
    border-radius: 14px;
    justify-content: flex-start;
    color: #929294;
  }

  .label {
    color: #929294;
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }

  .option-icon {
    display: flex;
    flex: none;
    width: var(--icon-size);
    height: var(--icon-size);
  }

  .option-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .bars {
    display: flex;
    align-items: center;
    height: 28px;
  }

  .bar {
    height: 100%;
    border-radius: 2px;
    background: #e5e5e7;
    transition: transform 120ms ease-out;
  }

  .bars.paused .bar {
    background: #929294;
  }

  .time {
    font-variant-numeric: tabular-nums;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }

  button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: #e5e5e7;
    cursor: pointer;
  }

  button:hover,
  button.selecting,
  button:global(.hover) {
    background: #3a3a3c;
  }

  button.cancel:hover,
  button.cancel:global(.hover) {
    color: #d97373;
  }

  button.primary {
    background: #e5e5e7;
    color: #1c1c1e;
  }

  button.primary:hover,
  button.primary:global(.hover) {
    background: #cfcfd3;
  }

  button.secondary {
    background: #3a3a3c;
  }

  button.secondary:hover,
  button.secondary:global(.hover) {
    background: #48484a;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    animation: status-enter 180ms cubic-bezier(0.33, 1, 0.68, 1);
  }

  @keyframes status-enter {
    from {
      opacity: 0;
      transform: scale(0.85);
    }
  }

  .row.locked .target,
  .row.locked .actions {
    pointer-events: none;
  }

  .row.locked .target :global(svg),
  .row.locked .actions {
    opacity: 0;
    transition: opacity 150ms cubic-bezier(0.33, 1, 0.68, 1);
  }

  .row.stack {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }

  .layer {
    grid-area: 1 / 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }

  @media (prefers-reduced-motion: reduce) {
    .status {
      animation: none;
    }

    .row.locked .target :global(svg),
    .row.locked .actions {
      transition: none;
    }
  }

  .wave {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 24px;
  }

  .wave.fixed {
    flex: none;
  }

  .wave-bar {
    height: 12px;
    border-radius: 2px;
    background: #929294;
    transform: scaleY(0.34);
    animation: wave var(--wave-period) ease-in-out infinite;
  }

  @keyframes wave {
    50% {
      transform: scaleY(1);
    }
  }
</style>
