<script lang="ts">
  import type { Snippet } from "svelte";
  let {
    label,
    hint,
    error,
    children,
  }: {
    label: string;
    hint?: string;
    error?: string;
    children: Snippet;
  } = $props();
</script>

<div class="form-row">
  <span class="form-label">{label}</span>
  <div class="form-control">{@render children()}</div>
  {#if hint && !error}<p class="form-hint">{hint}</p>{/if}
  {#if error}<p class="form-error">{error}</p>{/if}
</div>

<style>
  .form-row {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .form-label {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--text-secondary);
  }
  .form-hint {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }
  .form-error {
    font-size: var(--font-size-sm);
    color: var(--danger);
  }
  .form-control {
    display: contents;
  }
</style>
