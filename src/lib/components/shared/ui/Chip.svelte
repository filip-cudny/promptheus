<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title = "",
    disabled = false,
    onclick,
    children,
  }: {
    title?: string;
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

{#if onclick}
  <button class="chip" class:clickable={!disabled} class:chip-disabled={disabled} {title} {disabled} {onclick}>
    {@render children()}
  </button>
{:else}
  <span class="chip" class:chip-disabled={disabled} {title}>
    {@render children()}
  </span>
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-4);
    background: var(--surface-elevated);
    border: 1px solid var(--border-hard-2);
    border-radius: var(--radius-chip);
    font-size: var(--font-size-md);
    color: var(--text-secondary);
    white-space: nowrap;
    font-family: inherit;
  }

  .clickable {
    cursor: pointer;
  }

  .clickable:hover {
    background: var(--surface-elevated);
  }

  .chip-disabled {
    background: var(--surface-elevated);
    border-color: var(--border-hard);
    color: var(--text-muted);
    cursor: default;
  }
</style>
