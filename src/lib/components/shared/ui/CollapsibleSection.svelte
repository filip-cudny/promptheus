<script lang="ts">
  import type { Snippet } from "svelte";
  import { slide } from "svelte/transition";
  import { ChevronRight, ChevronDown } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";

  let {
    title,
    collapsed = $bindable(false),
    headerClass,
    summary,
    actions,
    children,
  }: {
    title: string;
    collapsed: boolean;
    headerClass?: string;
    summary?: Snippet;
    actions?: Snippet;
    children: Snippet;
  } = $props();
</script>

<div class="collapsible-section">
  <button class="collapsible-header {headerClass ?? ''}" onclick={() => (collapsed = !collapsed)}>
    <span class="collapse-arrow">
      {#if collapsed}
        <ChevronRight size={ICON_SIZE.md} />
      {:else}
        <ChevronDown size={ICON_SIZE.md} />
      {/if}
    </span>
    <span class="collapsible-title">{title}</span>
    {#if summary}
      <span class="collapsible-summary">{@render summary()}</span>
    {/if}
    {#if actions}
      <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <span class="collapsible-actions" onclick={(e) => e.stopPropagation()}>
        {@render actions()}
      </span>
    {/if}
  </button>
  {#if !collapsed}
    <div class="collapsible-content" transition:slide={{ duration: 150 }}>
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .collapsible-section {
    border: 1px solid var(--border-default);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .collapsible-header {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    width: 100%;
    padding: 5px var(--space-6);
    background: var(--surface-overlay-faint);
    border: none;
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
    text-align: left;
  }

  .collapsible-header:hover {
    background: var(--surface-overlay);
  }

  .collapse-arrow {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .collapsible-title {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .collapsible-summary {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .collapsible-actions {
    display: flex;
    gap: var(--space-2);
    flex-shrink: 0;
    margin-left: auto;
  }

  .collapsible-content {
    padding: var(--space-5) var(--space-7);
  }
</style>
