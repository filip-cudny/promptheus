<script lang="ts">
  import { ChevronDown, X, Search } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import FloatingPanel from "$lib/components/shared/ui/FloatingPanel.svelte";
  import type { HistorySearchStore } from "$lib/stores/historySearch.svelte";

  let { searchStore }: { searchStore: HistorySearchStore } = $props();

  let triggerEl = $state<HTMLButtonElement | undefined>();
  let open = $state(false);
  let localFilter = $state("");

  let selectedCount = $derived(searchStore.skillFilter.size);
  let selectedFirstName = $derived.by(() => {
    if (selectedCount !== 1) return null;
    const [id] = Array.from(searchStore.skillFilter);
    return searchStore.availableSkills.find((s) => s.skill_id === id)?.skill_name ?? id;
  });

  let visibleSkills = $derived.by(() => {
    const q = localFilter.trim().toLowerCase();
    if (!q) return searchStore.availableSkills;
    return searchStore.availableSkills.filter((s) =>
      s.skill_name.toLowerCase().includes(q),
    );
  });

  let showLocalSearch = $derived(searchStore.availableSkills.length > 10);

  function toggleOpen() {
    open = !open;
    if (open) localFilter = "";
  }

  function handleClose() {
    open = false;
  }

  function handleClearAllSkills(e: MouseEvent) {
    e.stopPropagation();
    searchStore.clearSkills();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      e.preventDefault();
      open = false;
      triggerEl?.focus();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="trigger-group" class:active={selectedCount > 0}>
  <button
    bind:this={triggerEl}
    type="button"
    class="trigger"
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    {#if selectedCount === 0}
      <span class="label">Skills</span>
      <ChevronDown size={ICON_SIZE.sm} />
    {:else if selectedCount === 1}
      <span class="label">Skill: {selectedFirstName}</span>
    {:else}
      <span class="label">Skills ({selectedCount})</span>
    {/if}
  </button>
  {#if selectedCount > 0}
    <ActionIconButton icon={X} onclick={handleClearAllSkills} label="Clear skill filter" />
  {/if}
</div>

<FloatingPanel
  visible={open}
  anchorEl={triggerEl}
  onclose={handleClose}
  fitContent
>
  <div class="popover">
    {#if showLocalSearch}
      <div class="local-search">
        <span class="local-search-icon"><Search size={ICON_SIZE.sm} /></span>
        <input
          type="search"
          class="local-search-input"
          bind:value={localFilter}
          placeholder="Filter skills..."
          autocomplete="off"
          spellcheck="false"
        />
      </div>
    {/if}

    <ul
      class="skill-list"
      role="listbox"
      aria-multiselectable="true"
      aria-label="Skills"
    >
      {#each visibleSkills as skill (skill.skill_id)}
        {@const selected = searchStore.skillFilter.has(skill.skill_id)}
        <li
          class="skill-item"
          class:selected
          role="option"
          aria-selected={selected}
          tabindex="0"
          onclick={() => searchStore.toggleSkill(skill.skill_id)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              searchStore.toggleSkill(skill.skill_id);
            }
          }}
        >
          <span class="checkbox" aria-hidden="true">
            {#if selected}<span class="check">✓</span>{/if}
          </span>
          <span class="skill-name">{skill.skill_name}</span>
          <span class="skill-count">({skill.count})</span>
        </li>
      {:else}
        <li class="empty">No skills match</li>
      {/each}
    </ul>

    <div class="footer">
      <Button disabled={selectedCount === 0} onclick={() => searchStore.clearSkills()}>Clear</Button>
      <Button variant="primary" onclick={handleClose}>Done</Button>
    </div>
  </div>
</FloatingPanel>

<style>
  .trigger-group {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    overflow: hidden;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .trigger-group.active {
    background: var(--accent-bg-soft);
    border-color: var(--accent-bg);
    color: var(--accent-text);
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: 3px var(--space-4);
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-sm);
  }

  .trigger:hover {
    background: var(--surface-overlay);
    color: var(--text-primary);
  }

  .trigger-group.active .trigger:hover {
    background: var(--accent-bg-soft);
    color: var(--accent-text);
  }

  .label {
    white-space: nowrap;
  }

  .popover {
    display: flex;
    flex-direction: column;
    min-width: 220px;
    max-width: 320px;
    color: var(--text-primary);
    font-size: var(--font-size-md);
  }

  .local-search {
    position: relative;
    display: flex;
    align-items: center;
    margin-bottom: var(--space-3);
  }

  .local-search-icon {
    position: absolute;
    left: 8px;
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    pointer-events: none;
  }

  .local-search-input {
    flex: 1;
    width: 100%;
    padding: var(--space-2) var(--space-4) var(--space-2) 24px;
    background: var(--surface-elevated);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
    outline: none;
  }

  .local-search-input::-webkit-search-decoration,
  .local-search-input::-webkit-search-cancel-button {
    appearance: none;
  }

  .local-search-input:focus {
    border-color: var(--accent-border);
  }

  .skill-list {
    list-style: none;
    margin: var(--space-0);
    padding: var(--space-0);
    max-height: 280px;
    overflow-y: auto;
  }

  .skill-item {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: 5px var(--space-4);
    border-radius: var(--radius-md);
    cursor: pointer;
    user-select: none;
  }

  .skill-item:hover {
    background: var(--surface-overlay-faint);
  }

  .skill-item.selected {
    background: var(--accent-bg-soft);
  }

  .skill-item:focus-visible {
    outline: 1px solid var(--accent-border);
    outline-offset: -1px;
  }

  .checkbox {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.2);
  }

  .skill-item.selected .checkbox {
    background: var(--accent-border);
    border-color: var(--accent-ring);
  }

  .check {
    font-size: var(--font-size-xs);
    line-height: 1;
    color: var(--text-primary);
  }

  .skill-name {
    flex: 1;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-count {
    flex-shrink: 0;
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
  }

  .empty {
    padding: var(--space-4);
    color: var(--text-disabled);
    text-align: center;
    font-style: italic;
  }

  .footer {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-default);
  }
</style>
