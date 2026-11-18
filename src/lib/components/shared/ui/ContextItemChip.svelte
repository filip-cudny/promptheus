<script lang="ts">
  import type { ComponentType, SvelteComponent } from "svelte";
  import { Type, X } from "lucide-svelte";
  import type { IconProps } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";

  let {
    kind,
    number,
    text = "",
    src,
    title,
    icon: Icon = Type,
    maxWidth,
    onclick,
    onremove,
    removeLabel = "Remove",
  }: {
    kind: "image" | "text";
    number?: number | null;
    text?: string;
    src?: string;
    title?: string;
    icon?: ComponentType<SvelteComponent<IconProps>>;
    maxWidth?: string;
    onclick?: () => void;
    onremove?: () => void;
    removeLabel?: string;
  } = $props();

  const numberLabel = $derived(number == null ? "" : `#${number}`);

  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    onclick?.();
  }

  function handleRemove(e: MouseEvent) {
    e.stopPropagation();
    onremove?.();
  }
</script>

{#snippet body()}
  {#if kind === "image"}
    <img class="chip-thumbnail" {src} alt="" />
  {:else}
    <span class="chip-icon"><Icon size={ICON_SIZE.sm} /></span>
  {/if}
  {#if numberLabel}
    <span class="chip-number">{numberLabel}</span>
  {/if}
  {#if kind === "text"}
    <span class="chip-text">{text}</span>
  {/if}
{/snippet}

<span class="context-item-chip" class:text-chip={kind === "text"} style:max-width={kind === "text" ? maxWidth : undefined}>
  {#if onclick}
    <button type="button" class="chip-body clickable" {title} onclick={handleClick}>
      {@render body()}
    </button>
  {:else}
    <span class="chip-body" {title}>
      {@render body()}
    </span>
  {/if}
  {#if onremove}
    <button type="button" class="chip-remove" aria-label={removeLabel} title={removeLabel} onclick={handleRemove}>
      <X size={ICON_SIZE.sm} />
    </button>
  {/if}
</span>

<style>
  .context-item-chip {
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
    min-width: 0;
    height: 20px;
    padding: 0 7px 0 2px;
    gap: 5px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: rgba(0, 0, 0, 0.25);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--text-primary);
    white-space: nowrap;
  }

  .context-item-chip.text-chip {
    flex: 0 1 auto;
  }

  .context-item-chip:hover {
    border-color: var(--accent-border);
  }

  .chip-body {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
  }

  .chip-body.clickable {
    cursor: pointer;
  }

  .chip-thumbnail {
    width: 16px;
    height: 16px;
    flex: none;
    object-fit: cover;
    border-radius: var(--radius-md);
  }

  .chip-icon {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    flex: none;
    color: var(--text-muted);
  }

  .chip-number {
    flex: none;
  }

  .chip-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip-remove {
    display: grid;
    place-items: center;
    flex: none;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .chip-remove:hover {
    color: var(--text-primary);
  }
</style>
