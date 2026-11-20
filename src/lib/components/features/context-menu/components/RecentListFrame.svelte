<script lang="ts">
  import type { Snippet } from "svelte";
  import { Search } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";

  let {
    filter,
    children,
  }: {
    filter: Snippet;
    children?: Snippet;
  } = $props();
</script>

<div class="recent-list">
  <div class="filter">
    <Search size={ICON_SIZE.md} />
    {@render filter()}
  </div>

  <div class="box" role="listbox">
    {@render children?.()}
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

  .filter :global(input) {
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

  .filter :global(input::placeholder) {
    color: var(--text-faint);
  }

  .box {
    height: 160px;
    overflow-y: auto;
  }
</style>
