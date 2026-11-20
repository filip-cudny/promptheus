<script lang="ts">
  import { useCopyFeedback } from "$lib/utils/copyFeedback.svelte";
  import { rankByQuery } from "$lib/utils/fuzzyRank";
  import { useListSelection } from "$lib/components/shared/drivers/useListSelection.svelte";
  import type { RecentQuickAction } from "$lib/services/history";
  import RecentListFrame from "./RecentListFrame.svelte";
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

<RecentListFrame>
  {#snippet filter()}
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
  {/snippet}

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
</RecentListFrame>

<style>
  .count {
    flex: none;
    font-size: var(--font-size-sm);
    color: var(--text-faint);
  }

  .empty {
    padding: var(--space-5) var(--space-4);
    text-align: center;
    font-size: var(--font-size-md);
    color: var(--text-faint);
  }
</style>
