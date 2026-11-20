<script lang="ts" generics="T extends string">
  import { ChevronDown } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import FloatingPanel from "./FloatingPanel.svelte";

  let {
    value = $bindable(),
    options,
    ariaLabel,
    activeWhenNot,
    onchange,
  }: {
    value: T;
    options: ReadonlyArray<{ value: T; label: string }>;
    ariaLabel: string;
    activeWhenNot?: T;
    onchange?: (value: T) => void;
  } = $props();

  let triggerEl = $state<HTMLButtonElement | undefined>();
  let open = $state(false);

  let selectedLabel = $derived(
    options.find((o) => o.value === value)?.label ?? "",
  );
  let isActive = $derived(
    activeWhenNot !== undefined && value !== activeWhenNot,
  );

  function toggleOpen() {
    open = !open;
  }

  function handleClose() {
    open = false;
  }

  function selectOption(next: T) {
    value = next;
    open = false;
    triggerEl?.focus();
    onchange?.(next);
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

<button
  bind:this={triggerEl}
  type="button"
  class="trigger"
  class:active={isActive}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-label="{ariaLabel}: {selectedLabel}"
  onclick={toggleOpen}
>
  <span class="label">{selectedLabel}</span>
  <ChevronDown size={ICON_SIZE.sm} />
</button>

<FloatingPanel
  visible={open}
  anchorEl={triggerEl}
  onclose={handleClose}
  fitContent
>
  <ul class="option-list" role="listbox" aria-label={ariaLabel}>
    {#each options as option (option.value)}
      {@const selected = option.value === value}
      <li
        class="option"
        class:selected
        role="option"
        aria-selected={selected}
        tabindex="0"
        onclick={() => selectOption(option.value)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            selectOption(option.value);
          }
        }}
      >
        {option.label}
      </li>
    {/each}
  </ul>
</FloatingPanel>

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 3px 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-elevated);
    color: var(--text-secondary);
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-sm);
    line-height: 1.4;
  }

  .trigger:hover {
    background: var(--surface-overlay);
    color: var(--text-primary);
  }

  .trigger.active {
    background: var(--accent-bg-soft);
    border-color: var(--accent-bg);
    color: var(--accent-text);
  }

  .trigger.active:hover {
    background: var(--accent-bg-soft);
    color: var(--accent-text);
  }

  .trigger:focus-visible {
    outline: 2px solid var(--accent-ring);
    outline-offset: 1px;
  }

  .label {
    white-space: nowrap;
  }

  .trigger :global(svg) {
    flex-shrink: 0;
  }

  .option-list {
    list-style: none;
    margin: var(--space-0);
    padding: var(--space-0);
    min-width: 140px;
    color: var(--text-primary);
    font-size: var(--font-size-md);
  }

  .option {
    padding: 5px 12px;
    cursor: pointer;
    user-select: none;
    color: var(--text-primary);
  }

  .option:hover {
    background: var(--surface-overlay-faint);
  }

  .option.selected {
    background: var(--accent-bg-soft);
    color: var(--accent-text);
  }

  .option:focus-visible {
    outline: 1px solid var(--accent-border);
    outline-offset: -1px;
  }
</style>
