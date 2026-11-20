<script lang="ts">
  import { ChevronLeft, ChevronRight } from "lucide-svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";

  let {
    pageSizes,
    pageSize,
    currentPage = $bindable(),
    totalPages,
    onChangePageSize,
  }: {
    pageSizes: readonly number[];
    pageSize: number;
    currentPage: number;
    totalPages: number;
    onChangePageSize: (size: number) => void;
  } = $props();
</script>

<div class="pagination-bar">
  <div class="page-size">
    {#each pageSizes as size}
      <button
        class="page-size-btn"
        class:active={pageSize === size}
        onclick={() => onChangePageSize(size)}
      >
        {size}
      </button>
    {/each}
  </div>

  <span class="page-label">Page {currentPage + 1} of {totalPages}</span>

  <div class="page-nav">
    <ActionIconButton
      icon={ChevronLeft}
      disabled={currentPage === 0}
      onclick={() => currentPage--}
      label="Previous page"
    />
    <ActionIconButton
      icon={ChevronRight}
      disabled={currentPage >= totalPages - 1}
      onclick={() => currentPage++}
      label="Next page"
    />
  </div>
</div>

<style>
  .pagination-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-4) var(--space-6);
    border-top: 1px solid var(--border-default);
    flex-shrink: 0;
  }

  .page-size {
    display: flex;
    gap: var(--space-1);
  }

  .page-size-btn {
    padding: var(--space-1) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font: inherit;
    font-size: var(--font-size-sm);
  }

  .page-size-btn:hover {
    background: var(--surface-overlay);
    color: var(--text-secondary);
  }

  .page-size-btn.active {
    background: var(--accent-bg-soft);
    border-color: var(--accent-bg);
    color: var(--accent-text);
  }

  .page-size-btn:focus-visible {
    outline: 2px solid var(--accent-ring);
    outline-offset: 1px;
  }

  .page-label {
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
  }

  .page-nav {
    display: flex;
    gap: var(--space-1);
  }
</style>
