<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { ChevronRight, MessagesSquare, Plus, X } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { highlightFor } from "$lib/utils/highlightMatches";
  import { useListSelection } from "$lib/components/shared/drivers/useListSelection.svelte";
  import type { SearchResult } from "$lib/types/historySearch";
  import { useHistorySearch } from "$lib/stores/useHistorySearch.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import CommandPalette from "$lib/components/shared/ui/CommandPalette.svelte";
  import KbdHint from "$lib/components/shared/ui/KbdHint.svelte";
  import { displayName, snippetFor, formatTimestamp } from "$lib/utils/historySearchSnippet";

  let { open, onClose, onNewChat, onOpenConversation }: {
    open: boolean;
    onClose: () => void;
    onNewChat: () => void;
    onOpenConversation: (entryId: string) => void;
  } = $props();

  const historySearch = useHistorySearch();

  let query = $state("");
  let inputEl = $state<HTMLInputElement | undefined>();

  type Item =
    | { kind: "new-chat" }
    | { kind: "recent"; result: SearchResult };

  let items = $derived<Item[]>([
    { kind: "new-chat" },
    ...historySearch.results.map<Item>((r) => ({ kind: "recent", result: r })),
  ]);

  const selection = useListSelection({
    count: () => items.length,
    active: () => open,
    onEscape: () => onClose(),
    onEnter: (i) => {
      const item = items[i];
      if (item) activate(item);
    },
    stopPropagation: true,
  });

  $effect(() => {
    if (open) {
      query = "";
      selection.index = 0;
      historySearch.clear();
      historySearch.search("");
      tick().then(() => inputEl?.focus());
    } else {
      historySearch.cancel();
    }
  });

  $effect(() => {
    if (!open) return;
    historySearch.search(query);
  });

  function activate(item: Item) {
    if (item.kind === "new-chat") {
      onNewChat();
    } else {
      onOpenConversation(item.result.entry.id);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    selection.handleKeydown(e);
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", handleKeydown);
    historySearch.cancel();
  });

</script>

<CommandPalette
  {open}
  {onClose}
  bind:query
  bind:inputRef={inputEl}
  placeholder="Search or start a chat"
  variant="overlay"
>
  {#snippet headerExtras()}
    <span class="palette-close">
      <ActionIconButton icon={X} label="Close" onclick={onClose} />
    </span>
  {/snippet}
  {#snippet body()}
    <div class="palette-section-header">
      <span class="palette-section-icon"><ChevronRight size={ICON_SIZE.sm} /></span>
      <span>Quick actions</span>
    </div>
    {#each items as item, i (item.kind === "new-chat" ? "new-chat" : item.result.entry.id)}
      {#if item.kind === "new-chat"}
        <button
          bind:this={selection.itemEls[i]}
          type="button"
          role="option"
          aria-selected={i === selection.index}
          class="palette-item"
          class:highlight={i === selection.index}
          onmouseenter={() => (selection.index = i)}
          onclick={() => activate(item)}
        >
          <span class="palette-item-icon"><Plus size={ICON_SIZE.md} /></span>
          <span class="palette-item-name">New chat</span>
          <span class="palette-item-hint">Enter</span>
        </button>
      {:else}
        {#if i === 1}
          <div class="palette-section-header">
            <span class="palette-section-icon"><ChevronRight size={ICON_SIZE.sm} /></span>
            <span>Recents</span>
          </div>
        {/if}
        {@const snippet = snippetFor(item.result)}
        <button
          bind:this={selection.itemEls[i]}
          type="button"
          role="option"
          aria-selected={i === selection.index}
          class="palette-item"
          class:highlight={i === selection.index}
          onmouseenter={() => (selection.index = i)}
          onclick={() => activate(item)}
        >
          <span class="palette-item-icon"><MessagesSquare size={ICON_SIZE.md} /></span>
          <span class="palette-item-main">
            <span class="palette-item-name">
              {@html highlightFor(displayName(item.result), item.result.matches, ["title", "skill_name"])}
            </span>
            {#if snippet}
              <span class="palette-item-snippet">{@html highlightFor(snippet.text, snippet.matches, [snippet.field])}</span>
            {/if}
          </span>
          <span class="palette-item-hint">{formatTimestamp(item.result)}</span>
        </button>
      {/if}
    {/each}
    {#if items.length === 1 && !historySearch.isLoading}
      <div class="palette-empty">
        {query.trim() ? "No matching conversations" : "No recent conversations"}
      </div>
    {/if}
  {/snippet}
  {#snippet footer()}
    <span><KbdHint keys={["↑↓"]} /> / <KbdHint keys={["⌃JK"]} /> Navigate</span>
    <span><KbdHint keys={["↵"]} /> Open</span>
    <span><KbdHint keys={["esc"]} /> Close</span>
  {/snippet}
</CommandPalette>

<style>
  .palette-close {
    display: flex;
    padding: var(--space-0) var(--space-6);
  }

  .palette-section-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-7) var(--space-2);
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
    text-transform: none;
  }

  .palette-section-icon {
    display: inline-flex;
    align-items: center;
    color: var(--text-disabled);
  }

  :global(.palette-item-main) {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }


  :global(.palette-item-main .palette-item-name) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.palette-item-snippet) {
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.palette-item-hint) {
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
    flex-shrink: 0;
  }

  :global(.palette-item-name mark),
  :global(.palette-item-snippet mark) {
    background: rgba(255, 220, 100, 0.25);
    color: inherit;
    padding: var(--space-0);
    border-radius: 2px;
  }
</style>
