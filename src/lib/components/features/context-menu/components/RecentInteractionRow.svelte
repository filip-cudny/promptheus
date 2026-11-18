<script module lang="ts">
  import type { RecentQuickAction } from "$lib/services/history";

  function collapseWhitespace(text: string): string {
    return text.replace(/\s+/g, " ").trim();
  }

  export function firstLine(row: RecentQuickAction): string {
    const main = row.status === "ok" ? (row.output_preview ?? row.input_preview) : row.input_preview;
    return collapseWhitespace(main);
  }

  export function secondLine(row: RecentQuickAction): string {
    if (row.entry_type === "speech") return "Transcript";
    return row.skill_name ?? row.skill_id ?? "";
  }

  export function canCopyOutput(row: RecentQuickAction): boolean {
    return row.status === "ok";
  }
</script>

<script lang="ts">
  import { Mic, Pencil, Check, SquareArrowOutUpRight, RotateCcw, LoaderCircle } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import { formatTimestamp } from "$lib/utils/format";
  import { highlightFor } from "$lib/utils/highlightMatches";
  import type { FuzzyMatch } from "$lib/utils/fuzzyRank";

  let {
    row,
    matches,
    selected,
    copied,
    element = $bindable(),
    onhover,
    onCopyOutput,
    onCopyPart,
    onOpenInChat,
    onRetry,
  }: {
    row: RecentQuickAction;
    matches: (FuzzyMatch | null)[];
    selected: boolean;
    copied: boolean;
    element?: HTMLElement | null;
    onhover: () => void;
    onCopyOutput: () => void;
    onCopyPart: (part: "input" | "output") => void;
    onOpenInChat: () => void;
    onRetry: () => void;
  } = $props();

  let isSpeech = $derived(row.entry_type === "speech");
  let dimmed = $derived(row.status !== "ok");
  let first = $derived(firstLine(row));
  let second = $derived(secondLine(row));
  let firstHtml = $derived(
    highlightFor(first, [{ field: "input_preview", indices: matches[0]?.indices ?? [] }], ["input_preview"]),
  );
  let secondHtml = $derived(
    highlightFor(second, [{ field: "skill_name", indices: matches[1]?.indices ?? [] }], ["skill_name"]),
  );
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="row"
  class:selected
  class:dimmed
  role="option"
  tabindex="-1"
  aria-selected={selected}
  bind:this={element}
  onmousemove={onhover}
  onmousedown={(e) => e.preventDefault()}
  onclick={onCopyOutput}
>
  <span class="type-icon">
    {#if copied}
      <Check size={ICON_SIZE.md} />
    {:else if isSpeech}
      <Mic size={ICON_SIZE.md} />
    {:else}
      <Pencil size={ICON_SIZE.md} />
    {/if}
  </span>

  <span class="lines">
    <span class="line1">
      <span class="main">{@html firstHtml}</span>
      <span class="when">{formatTimestamp(row.timestamp)}</span>
    </span>
    <span class="sub">{@html secondHtml}</span>
  </span>

  <span class="actions">
    {#if !isSpeech}
      <button
        type="button"
        class="mini"
        title="Copy the input"
        onclick={(e) => { e.stopPropagation(); onCopyPart("input"); }}
      >In</button>
      <button
        type="button"
        class="mini"
        title="Copy the output"
        disabled={!canCopyOutput(row)}
        onclick={(e) => { e.stopPropagation(); onCopyPart("output"); }}
      >Out</button>
    {/if}
    {#if row.status === "retrying"}
      <span class="spinning" title="Retrying transcription…"><LoaderCircle size={ICON_SIZE.md} /></span>
    {:else if isSpeech && row.status === "failed"}
      {#if row.has_audio}
        <ActionIconButton icon={RotateCcw} label="Retry transcription" onclick={onRetry} />
      {/if}
    {:else}
      <ActionIconButton icon={SquareArrowOutUpRight} label="Open in chat" onclick={onOpenInChat} />
    {/if}
  </span>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-1) var(--space-4);
    font-size: var(--font-size-md);
    cursor: pointer;
  }

  .row:not(:first-child) {
    border-top: 1px solid var(--surface-overlay-faint);
  }

  .row.selected {
    background: var(--surface-overlay);
  }

  .type-icon {
    display: flex;
    flex: none;
    width: 14px;
    justify-content: center;
    color: var(--text-muted);
  }

  .lines {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .line1 {
    display: flex;
    gap: var(--space-4);
    min-width: 0;
  }

  .main {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 15px;
    color: var(--text-secondary);
  }

  .dimmed .main {
    color: var(--text-disabled);
  }

  .when {
    flex: none;
    font-size: var(--font-size-xs);
    line-height: 15px;
    color: var(--text-faint);
  }

  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-xs);
    line-height: 12px;
    color: var(--text-faint);
  }

  .main :global(mark),
  .sub :global(mark) {
    background: transparent;
    color: var(--info);
    font-weight: 600;
    padding: var(--space-0);
  }

  .actions {
    display: none;
    flex: none;
    align-items: center;
    gap: var(--space-1);
  }

  .row.selected .actions {
    display: flex;
  }

  .row.selected .when {
    display: none;
  }

  .mini {
    padding: var(--space-0) var(--space-3);
    border: 1px solid var(--border-hard-2);
    border-radius: var(--radius-2xl);
    background: var(--surface-elevated);
    color: var(--text-secondary);
    font-family: inherit;
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .mini:hover:not(:disabled) {
    border-color: var(--border-strong);
  }

  .mini:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .spinning {
    display: flex;
    color: var(--text-muted);
    animation: spin 1s linear infinite;
  }
</style>
