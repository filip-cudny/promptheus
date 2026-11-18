<script lang="ts">
  import { untrack } from "svelte";
  import { slide } from "svelte/transition";
  import type { ToolCall } from "$lib/types/ai";
  import { ChevronRight, ChevronDown, Wrench } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import ToolCallItem from "./ToolCallItem.svelte";
  import ProcessingIndicator from "../components/ProcessingIndicator.svelte";

  let {
    toolCalls,
    isStreaming = false,
  }: {
    toolCalls: ToolCall[];
    isStreaming?: boolean;
  } = $props();

  let allCompleted = $derived(
    toolCalls.length > 0 &&
    toolCalls.every((tc) => tc.status === "completed" || tc.status === "failed" || tc.status === "cancelled")
  );

  let expanded = $state(untrack(() => !allCompleted));
  let wasActive = $state(false);

  let anyActive = $derived(
    toolCalls.some((tc) => tc.status === "in_progress" || tc.status === "pending")
  );

  let failedCount = $derived(toolCalls.filter((tc) => tc.status === "failed").length);

  let summaryText = $derived.by(() => {
    const label = toolCalls.length === 1 ? "Used 1 tool" : `Used ${toolCalls.length} tools`;
    return failedCount > 0 ? `${label} (${failedCount} failed)` : label;
  });

  $effect(() => {
    if (anyActive) {
      wasActive = true;
    }
    if (allCompleted && wasActive && !isStreaming) {
      expanded = false;
    }
  });
</script>

{#if anyActive || (isStreaming && wasActive)}
  <section class="tool-group">
    <div class="tool-group-label tool-group-label-active">
      <Wrench size={ICON_SIZE.sm} />
      <ProcessingIndicator label="Running tools" inline />
    </div>
    <div class="tool-group-items">
      {#each toolCalls as toolCall (toolCall.tool_call_id)}
        <ToolCallItem {toolCall} />
      {/each}
    </div>
  </section>
{:else if toolCalls.length > 0}
  <section class="tool-group">
    <button
      class="tool-group-toggle"
      onclick={() => (expanded = !expanded)}
      aria-expanded={expanded}
    >
      {#if expanded}
        <ChevronDown size={ICON_SIZE.sm} />
      {:else}
        <ChevronRight size={ICON_SIZE.sm} />
      {/if}
      <Wrench size={ICON_SIZE.sm} />
      <span class="tool-group-summary">{summaryText}</span>
    </button>
    {#if expanded}
      <div class="tool-group-items" transition:slide={{ duration: 150 }}>
        {#each toolCalls as toolCall (toolCall.tool_call_id)}
          <ToolCallItem {toolCall} />
        {/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  .tool-group {
    margin: var(--space-4) var(--space-0);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .tool-group-label {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-0) var(--space-2);
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
  }

  .tool-group-label-active :global(.processing-label) {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
  }

  .tool-group-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-2) var(--space-0);
    border: none;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
    text-align: left;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }

  .tool-group-toggle:hover {
    color: var(--text-secondary);
  }

  .tool-group-summary {
    font-weight: var(--font-weight-medium);
  }

  .tool-group-items {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
</style>
