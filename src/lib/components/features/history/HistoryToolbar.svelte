<script lang="ts">
  import { Search, X } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import type { HistorySearchStore } from "$lib/stores/historySearch.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import type {
    HistoryStatusFilter,
    HistoryTypeFilter,
    TimeRangePreset,
  } from "$lib/types/historySearch";
  import SelectDropdown from "$lib/components/shared/ui/SelectDropdown.svelte";
  import SkillFilterPicker from "./SkillFilterPicker.svelte";

  let {
    searchStore,
    searchInput = $bindable<HTMLInputElement | null>(null),
  }: {
    searchStore: HistorySearchStore;
    searchInput?: HTMLInputElement | null;
  } = $props();

  const TYPE_OPTIONS: ReadonlyArray<{ value: HistoryTypeFilter; label: string }> = [
    { value: "all", label: "All" },
    { value: "chat", label: "Chat" },
    { value: "quick_action", label: "Quick Action" },
    { value: "speech", label: "Speech" },
  ];

  const STATUS_OPTIONS: ReadonlyArray<{ value: HistoryStatusFilter; label: string }> = [
    { value: "all", label: "All statuses" },
    { value: "success", label: "Success only" },
    { value: "error", label: "Errors only" },
  ];

  const TIME_RANGE_OPTIONS: ReadonlyArray<{ value: TimeRangePreset; label: string }> = [
    { value: "all", label: "All time" },
    { value: "today", label: "Today" },
    { value: "7d", label: "Last 7 days" },
    { value: "30d", label: "Last 30 days" },
  ];

  function handleSearchKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && searchStore.query !== "") {
      e.preventDefault();
      searchStore.query = "";
    }
  }

  function clearSearch() {
    searchStore.query = "";
    searchInput?.focus();
  }
</script>

<div class="toolbar">
  <div class="search-wrap">
    <span class="search-icon">
      <Search size={ICON_SIZE.md} />
    </span>
    <input
      bind:this={searchInput}
      bind:value={searchStore.query}
      onkeydown={handleSearchKeydown}
      type="search"
      class="search-input"
      placeholder="Search history..."
      aria-label="Search history"
      autocomplete="off"
      spellcheck="false"
    />
    {#if searchStore.query !== ""}
      <span class="clear-btn-wrap">
        <ActionIconButton icon={X} onclick={clearSearch} label="Clear search" />
      </span>
    {/if}
  </div>

  <div class="filters-row">
    <div class="type-segment" role="group" aria-label="Filter by type">
      {#each TYPE_OPTIONS as option (option.value)}
        <button
          class="segment-btn"
          class:active={searchStore.typeFilter === option.value}
          aria-pressed={searchStore.typeFilter === option.value}
          onclick={() => (searchStore.typeFilter = option.value)}
        >
          {option.label}
        </button>
      {/each}
    </div>

    {#if searchStore.availableSkills.length > 0}
      <SkillFilterPicker {searchStore} />
    {/if}

    <SelectDropdown
      options={TIME_RANGE_OPTIONS}
      bind:value={searchStore.timeRange}
      ariaLabel="Filter by time range"
      activeWhenNot="all"
    />

    <div class="status-wrap">
      <SelectDropdown
        options={STATUS_OPTIONS}
        bind:value={searchStore.statusFilter}
        ariaLabel="Filter by status"
        activeWhenNot="all"
      />
    </div>

    {#if searchStore.activeFilterCount >= 2}
      <Button icon={X} onclick={() => searchStore.clear()}>
        Clear all ({searchStore.activeFilterCount})
      </Button>
    {/if}
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-5) var(--space-6);
    flex-shrink: 0;
  }

  .search-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 10px;
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    pointer-events: none;
  }

  .search-input {
    flex: 1;
    width: 100%;
    padding: var(--space-3) 32px var(--space-3) 30px;
    background: var(--surface-elevated);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-lg);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-base);
    outline: none;
  }

  .search-input::-webkit-search-decoration,
  .search-input::-webkit-search-cancel-button {
    appearance: none;
  }

  .search-input:focus {
    border-color: var(--accent-border);
    background: var(--surface-sunken);
  }

  .search-input::placeholder {
    color: var(--text-disabled);
  }

  .clear-btn-wrap {
    position: absolute;
    right: 6px;
    display: flex;
  }

  .filters-row {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
  }

  .type-segment {
    display: flex;
    gap: var(--space-1);
  }

  .segment-btn {
    padding: 3px var(--space-5);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-sm);
  }

  .segment-btn:hover {
    background: var(--surface-overlay);
    color: var(--text-primary);
  }

  .segment-btn.active {
    background: var(--accent-bg-soft);
    border-color: var(--accent-bg);
    color: var(--accent-text);
  }

  .status-wrap {
    margin-left: auto;
  }

  .segment-btn:focus-visible {
    outline: 2px solid var(--accent-ring);
    outline-offset: 1px;
  }
</style>
