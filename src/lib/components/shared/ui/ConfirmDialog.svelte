<script lang="ts">
  import Button from "$lib/components/shared/ui/Button.svelte";

  let {
    open,
    message,
    confirmLabel = "Delete",
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    message: string;
    confirmLabel?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      onConfirm();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="confirm-overlay" onclick={onCancel}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="confirm-dialog"
      onclick={(e: MouseEvent) => e.stopPropagation()}
      onkeydown={handleKeydown}
    >
      <p class="confirm-text">{message}</p>
      <div class="confirm-actions">
        <Button onclick={onCancel}>Cancel</Button>
        <Button variant="danger" onclick={onConfirm} autofocus>{confirmLabel}</Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .confirm-overlay {
    position: fixed;
    inset: 0;
    background: var(--surface-scrim);
    z-index: 400;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .confirm-dialog {
    background: var(--surface-sunken);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-xl);
    padding: var(--space-8) var(--space-10);
    min-width: 240px;
    display: flex;
    flex-direction: column;
    gap: var(--space-7);
  }

  .confirm-text {
    margin: var(--space-0);
    font-size: var(--font-size-base);
    color: var(--text-primary);
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-4);
  }
</style>
