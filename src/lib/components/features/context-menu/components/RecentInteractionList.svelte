<script lang="ts">
  import { Search } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { useCopyFeedback } from "$lib/utils/copyFeedback.svelte";
  import { rankByQuery } from "$lib/utils/fuzzyRank";
  import { useListSelection } from "$lib/components/shared/drivers/useListSelection.svelte";
  import type { RecentQuickAction } from "$lib/services/history";
  import RecentInteractionRow, {
    firstLine,
    secondLine,
    canCopyOutput,
  } from "./RecentInteractionRow.svelte";

  let {
    rows,
    onCopyPart,
    onOpenInChat,
    onRetry,
    onClose,
  }: {
    rows: RecentQuickAction[] | null;
    onCopyPart: (entryId: string, part: "input" | "output") => Promise<boolean>;
    onOpenInChat: (row: RecentQuickAction) => void;
    onRetry: (entryId: string) => void;
    onClose: () => void;
  } = $props();

  let query = $state("");
  let inputEl = $state<HTMLInputElement>();
  const copyFeedback = useCopyFeedback<string>();

  let ranked = $derived(
    rankByQuery(rows ?? [], query, (r) => [firstLine(r), secondLine(r)]),
  );
  let filtering = $derived(query.trim() !== "");

  const selection = useListSelection({
    count: () => ranked.length,
    active: () => true,
    onEscape: handleEscape,
    onEnter: (index) => {
      const entry = ranked[index];
      if (entry) void copy(entry.item, "output");
    },
  });

  $effect(() => {
    const el = inputEl;
    el?.focus();
    return () => el?.blur();
  });

  async function copy(row: RecentQuickAction, part: "input" | "output") {
    if (part === "output" && !canCopyOutput(row)) return;
    if (await onCopyPart(row.id, part)) copyFeedback.flash(row.id);
  }

  function handleEscape() {
    if (query) {
      query = "";
      selection.index = 0;
      return;
    }
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Shift") return;
    e.stopPropagation();
    selection.handleKeydown(e);
  }
</script>

<div class="recent-list">
  <div class="filter">
    <Search size={ICON_SIZE.md} />
    <input
      bind:this={inputEl}
      bind:value={query}
      oninput={() => (selection.index = 0)}
      onkeydown={handleKeydown}
      placeholder="Filter"
      autocomplete="off"
      spellcheck="false"
    />
    {#if filtering}
      <span class="count">{ranked.length}</span>
    {/if}
  </div>

  <div class="box" role="listbox">
    {#if rows !== null && rows.length === 0}
      <div class="empty">No entries</div>
    {:else if rows !== null && ranked.length === 0}
      <div class="empty">No matches</div>
    {:else}
      {#each ranked as { item, matches }, i (item.id)}
        <RecentInteractionRow
          row={item}
          {matches}
          selected={i === selection.index}
          copied={copyFeedback.confirmed === item.id}
          bind:element={selection.itemEls[i]}
          onhover={() => (selection.index = i)}
          onCopyOutput={() => copy(item, "output")}
          onCopyPart={(part) => copy(item, part)}
          onOpenInChat={() => onOpenInChat(item)}
          onRetry={() => onRetry(item.id)}
        />
      {/each}
    {/if}
  </div>
</div>

<style>
  .recent-list {
    margin: var(--space-1) var(--space-6) var(--space-3);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .filter {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border-default);
    color: var(--text-muted);
  }

  .filter input {
    flex: 1;
    min-width: 0;
    padding: var(--space-0);
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
  }

  .filter input::placeholder {
    color: var(--text-faint);
  }

  .count {
    flex: none;
    font-size: var(--font-size-sm);
    color: var(--text-faint);
  }

  .box {
    height: 160px;
    overflow-y: auto;
  }

  .empty {
    padding: var(--space-5) var(--space-4);
    text-align: center;
    font-size: var(--font-size-md);
    color: var(--text-faint);
  }
</style>
