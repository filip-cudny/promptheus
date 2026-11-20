<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    icon,
    shortcut,
    active = false,
    activeStyle = "tint",
    disabled = false,
    onclick,
  }: {
    label: string;
    icon?: Snippet;
    shortcut?: Snippet;
    active?: boolean;
    activeStyle?: "tint" | "rail";
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
  } = $props();
</script>

<button
  class="menu-item"
  class:active
  class:rail={active && activeStyle === "rail"}
  class:is-disabled={disabled}
  {disabled}
  {onclick}
  role="menuitem"
>
  {#if icon}
    <span class="menu-item-icon">{@render icon()}</span>
  {/if}
  <span class="menu-item-label">{label}</span>
  {#if shortcut}
    <span class="menu-item-shortcut">{@render shortcut()}</span>
  {/if}
</button>

<style>
  .menu-item {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-4);
    text-align: left;
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-md);
    line-height: var(--line-height-normal);
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    box-sizing: border-box;
  }

  .menu-item:hover:not(:disabled):not(.is-disabled) {
    background: var(--surface-overlay);
  }

  .menu-item.active {
    color: var(--accent-text);
    background: var(--accent-bg-soft);
  }

  .menu-item.active.rail {
    color: var(--text-primary);
    background: var(--surface-overlay);
    box-shadow: inset 3px 0 0 0 var(--accent);
  }

  .menu-item:disabled,
  .menu-item.is-disabled {
    color: var(--text-disabled);
    cursor: default;
    opacity: var(--opacity-disabled);
  }

  .menu-item-icon {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: inherit;
  }

  .menu-item-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-item-shortcut {
    color: var(--text-muted);
    margin-left: auto;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
</style>
