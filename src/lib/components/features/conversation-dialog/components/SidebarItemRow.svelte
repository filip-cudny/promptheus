<script lang="ts">
  import type { Snippet } from "svelte";
  import { MessageSquare, MessagesSquare, Mic, Circle } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";

  type IconKind = "speech" | "multi-turn" | "single" | "draft";

  let {
    active,
    icon,
    timestamp = null,
    onClick,
    body,
    trailing,
  }: {
    active: boolean;
    icon: IconKind;
    timestamp?: string | null;
    onClick: () => void;
    body: Snippet;
    trailing?: Snippet;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions, a11y_no_noninteractive_element_interactions -->
<div class="tab-item" class:active onclick={onClick}>
  {#if icon === "draft"}
    <Circle size={8} fill="currentColor" class="draft-dot" />
  {:else if icon === "speech"}
    <Mic size={ICON_SIZE.sm} />
  {:else if icon === "multi-turn"}
    <MessagesSquare size={ICON_SIZE.sm} />
  {:else}
    <MessageSquare size={ICON_SIZE.sm} />
  {/if}
  <div class="tab-body">
    {@render body()}
    {#if timestamp}
      <span class="tab-meta">{timestamp}</span>
    {/if}
  </div>
  {#if trailing}
    {@render trailing()}
  {/if}
</div>

<style>
  .tab-item {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5);
    border-radius: var(--radius-lg);
    color: var(--text-muted);
    font-size: var(--font-size-base);
    cursor: pointer;
    flex-shrink: 0;
  }

  .tab-item:hover {
    background: var(--surface-overlay-faint);
    color: var(--text-secondary);
  }

  .tab-item.active {
    background: var(--surface-overlay);
    color: var(--text-primary);
    font-weight: var(--font-weight-semibold);
  }

  .tab-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .tab-meta {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-regular);
    color: var(--text-disabled);
  }

  :global(.draft-dot) {
    color: var(--warning);
    flex-shrink: 0;
    margin-top: var(--space-2);
  }
</style>
