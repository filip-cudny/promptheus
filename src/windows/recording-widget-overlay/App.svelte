<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { attachConsole, error as logError } from "@tauri-apps/plugin-log";
  import type { ContextItem } from "$lib/types/context";
  import { isMac } from "$lib/utils/platform";
  import {
    DROPDOWN_PADDING,
    DROPDOWN_ROW_HEIGHT,
    DROPDOWN_SEPARATOR_HEIGHT,
    PADDING_X,
    PILL_WIDTH,
    PREVIEW_GAP,
    PREVIEW_MAX_HEIGHT,
    RADIUS,
    TARGET_ICON_GAP,
    TARGET_ICON_SIZE,
    TARGET_ICONS,
    TOOLTIP_HEIGHT,
    TOOLTIP_PADDING_X,
    dropdownSeparatorIndex,
    run,
    type PillModel,
  } from "../recording-widget/pill";

  interface OverlayDescriptor {
    seq: number;
    kind: "dropdown" | "preview" | "tooltip";
    up: boolean;
    index?: number;
    text?: string;
    centerX?: number;
  }

  type PreviewContent = { kind: "text"; text: string } | { kind: "image"; src: string };

  let model = $state<PillModel>({
    targets: [],
    selectedId: "",
    selectedLabel: "",
    targetLocked: false,
    items: [],
    itemRows: 0,
    contextVersion: 0,
    stopChatDestination: "",
  });
  let descriptor = $state.raw<OverlayDescriptor | null>(null);
  let previewContent = $state<PreviewContent | null>(null);
  let previewElement = $state<HTMLElement>();
  let imageElement = $state<HTMLImageElement>();
  let tooltipElement = $state<HTMLElement>();
  let previewOverflows = $state(false);
  let hoveredButton: HTMLButtonElement | null = null;
  let contextItems: ContextItem[] | null = null;
  let unlisteners: UnlistenFn[] = [];

  const separatorIndex = $derived(dropdownSeparatorIndex(model.targets));
  const dropdownHeight = $derived(
    model.targets.length * DROPDOWN_ROW_HEIGHT +
      2 * DROPDOWN_PADDING +
      (separatorIndex >= 0 ? DROPDOWN_SEPARATOR_HEIGHT : 0),
  );

  function setHoveredButton(button: HTMLButtonElement | null) {
    if (button === hoveredButton) return;
    hoveredButton?.classList.remove("hover");
    button?.classList.add("hover");
    hoveredButton = button;
  }

  function onPointer(point: { x: number; y: number } | null) {
    const element = point ? document.elementFromPoint(point.x, point.y) : null;
    setHoveredButton(element?.closest("button") ?? null);
  }

  async function loadImageSource(index: number | undefined): Promise<string | null> {
    try {
      contextItems ??= await invoke<ContextItem[]>("get_context_items");
    } catch (e) {
      logError(`get_context_items failed: ${e}`);
      return null;
    }
    const source = index === undefined ? undefined : contextItems[index];
    if (source?.item_type !== "image") {
      logError("recording widget preview: the context item is not an image");
      return null;
    }
    return `data:${source.media_type};base64,${source.data}`;
  }

  async function resolvePreviewContent(next: OverlayDescriptor): Promise<PreviewContent | null> {
    if (next.text != null) return { kind: "text", text: next.text };
    const src = await loadImageSource(next.index);
    return src ? { kind: "image", src } : null;
  }

  async function decodeImage(): Promise<boolean> {
    try {
      await imageElement?.decode();
      return true;
    } catch (e) {
      logError(`recording widget preview image decode failed: ${e}`);
      return false;
    }
  }

  function paintedFrame(): Promise<void> {
    return new Promise((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    });
  }

  async function acknowledge(seq: number) {
    try {
      await invoke("recording_widget_overlay_rendered", { seq });
    } catch (e) {
      logError(`recording_widget_overlay_rendered failed: ${e}`);
    }
  }

  async function onDescriptor(next: OverlayDescriptor) {
    descriptor = next;
    setHoveredButton(null);
    if (next.kind === "preview") {
      const content = await resolvePreviewContent(next);
      if (descriptor !== next || !content) return;
      previewContent = content;
    } else {
      previewContent = null;
    }
    await tick();
    if (descriptor !== next) return;
    if (next.kind === "preview" && previewContent?.kind === "image" && !(await decodeImage())) return;
    await paintedFrame();
    if (descriptor === next) await acknowledge(next.seq);
  }

  async function selectTarget(id: string) {
    await run("select_recording_widget_target", { id });
    await run("set_recording_widget_dropdown", { open: false, rows: 0, separator: false });
  }

  $effect(() => {
    previewOverflows =
      previewContent?.kind === "text" &&
      !!previewElement &&
      previewElement.scrollHeight > previewElement.clientHeight;
  });

  $effect(() => {
    if (!tooltipElement || descriptor?.kind !== "tooltip") return;
    const centerX = descriptor.centerX ?? 0;
    void descriptor.text;
    const left = centerX - tooltipElement.offsetWidth / 2;
    tooltipElement.style.left = `${Math.max(0, Math.min(left, PILL_WIDTH - tooltipElement.offsetWidth))}px`;
  });

  onMount(async () => {
    attachConsole();
    unlisteners = await Promise.all([
      listen<PillModel>("recording-widget-model", (event) => {
        model = event.payload;
        contextItems = null;
      }),
      listen<OverlayDescriptor>("recording-widget-overlay", (event) => onDescriptor(event.payload)),
      listen<{ x: number; y: number } | null>("recording-widget-overlay-pointer", (event) => onPointer(event.payload)),
    ]);
  });

  onDestroy(() => {
    unlisteners.forEach((unlisten) => unlisten());
  });
</script>

<div class="root" class:up={descriptor?.up} style:--radius="{RADIUS}px">
  {#if descriptor?.kind === "dropdown"}
    <div
      class="dropdown"
      class:mac={isMac}
      style:height="{dropdownHeight}px"
      style:padding="{DROPDOWN_PADDING}px 1px"
      style:--row-radius="{RADIUS - DROPDOWN_PADDING}px"
    >
      {#each model.targets as target, index (target.id)}
        <button
          type="button"
          class="option"
          class:selected={target.id === model.selectedId}
          tabindex="-1"
          style:height="{DROPDOWN_ROW_HEIGHT}px"
          style:padding="0 {PADDING_X}px"
          style:gap="{TARGET_ICON_GAP}px"
          onclick={() => selectTarget(target.id)}
        >
          {#if TARGET_ICONS[target.id]}
            <span class="option-icon" style:--icon-size="{TARGET_ICON_SIZE}px" aria-hidden="true">{@html TARGET_ICONS[target.id]}</span>
          {/if}
          {target.label}
        </button>
        {#if index === separatorIndex}
          <div class="separator" style:height="1px" style:margin="{(DROPDOWN_SEPARATOR_HEIGHT - 1) / 2}px {PADDING_X}px"></div>
        {/if}
      {/each}
    </div>
  {:else if descriptor?.kind === "preview" && previewContent}
    <div
      bind:this={previewElement}
      class="preview"
      class:image={previewContent.kind === "image"}
      class:faded={previewOverflows}
      style:max-height="{PREVIEW_MAX_HEIGHT}px"
      style:margin-top={descriptor.up ? undefined : `${PREVIEW_GAP}px`}
      style:margin-bottom={descriptor.up ? `${PREVIEW_GAP}px` : undefined}
    >
      {#if previewContent.kind === "image"}
        <img bind:this={imageElement} src={previewContent.src} alt="" />
      {:else}
        {previewContent.text}
      {/if}
    </div>
  {:else if descriptor?.kind === "tooltip"}
    <div
      bind:this={tooltipElement}
      class="tooltip"
      style:height="{TOOLTIP_HEIGHT}px"
      style:padding="0 {TOOLTIP_PADDING_X}px"
      style:margin-top={descriptor.up ? undefined : `${PREVIEW_GAP}px`}
      style:margin-bottom={descriptor.up ? `${PREVIEW_GAP}px` : undefined}
    >
      {descriptor.text}
    </div>
  {/if}
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
    justify-content: flex-start;
  }

  .root.up {
    justify-content: flex-end;
  }

  .dropdown {
    box-sizing: border-box;
    flex: none;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius);
    background: #1c1c1e;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
    color: #e5e5e7;
    font-size: 13px;
  }

  .dropdown.mac {
    background: rgba(28, 28, 30, 0.92);
  }

  .preview,
  .tooltip {
    box-sizing: border-box;
    flex: none;
    border: 1px solid var(--surface-floating-popover-border);
    border-radius: var(--surface-floating-popover-radius);
    background: var(--surface-floating-popover);
    color: #e5e5e7;
    font-size: 12px;
  }

  .preview {
    position: relative;
    overflow: hidden;
    padding: 8px;
    white-space: pre-wrap;
  }

  .preview.faded::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 36px;
    background: linear-gradient(transparent, var(--surface-floating-popover));
  }

  .tooltip {
    position: relative;
    align-self: flex-start;
    display: flex;
    align-items: center;
    width: max-content;
    white-space: nowrap;
  }

  .preview.image {
    height: 160px;
    padding: 0;
  }

  .preview img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .option {
    width: 100%;
    justify-content: flex-start;
    border-radius: 0;
    font: inherit;
  }

  .option:first-child {
    border-top-left-radius: var(--row-radius);
    border-top-right-radius: var(--row-radius);
  }

  .option:last-child {
    border-bottom-left-radius: var(--row-radius);
    border-bottom-right-radius: var(--row-radius);
  }

  .separator {
    flex: none;
    background: rgba(255, 255, 255, 0.12);
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

  .option.selected {
    font-weight: 700;
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
  button:global(.hover) {
    background: #3a3a3c;
  }
</style>
