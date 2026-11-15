<script lang="ts">
  import type { Snippet } from "svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";

  let {
    target,
    label,
    children,
  }: {
    target: string;
    label: string;
    children: Snippet;
  } = $props();

  const navigation = getSettingsNavigation();
</script>

<div class="cross-link">
  <span class="text">{@render children()}</span>
  <button type="button" class="link" onclick={() => void navigation.navigateTo(target)}>
    {label} ›
  </button>
</div>

<style>
  .cross-link {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    max-width: 760px;
    padding: var(--space-4) var(--space-5);
    border: 1px dashed var(--border-default);
    border-radius: var(--radius-md);
    font-size: var(--font-size-md);
    color: var(--text-secondary);
  }

  .text :global(kbd) {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    padding: var(--space-0) var(--space-2);
    background: var(--surface-sunken);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
  }

  .link {
    background: none;
    border: none;
    padding: var(--space-0);
    font: inherit;
    color: var(--accent);
    cursor: pointer;
    white-space: nowrap;
  }

  .link:hover {
    text-decoration: underline;
  }
</style>
