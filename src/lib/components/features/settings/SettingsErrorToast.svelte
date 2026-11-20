<script lang="ts">
  import Button from "$lib/components/shared/ui/Button.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";

  const AUTO_DISMISS_MS = 6000;
  const store = getSettingsStore();

  $effect(() => {
    if (!store.toastError) return;
    const timer = setTimeout(store.dismissToast, AUTO_DISMISS_MS);
    return () => clearTimeout(timer);
  });
</script>

{#if store.toastError}
  <div class="toast error" role="alert">
    <span>{store.toastError}</span>
    <Button onclick={store.dismissToast}>Dismiss</Button>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: var(--space-4);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    background: var(--danger-bg-soft);
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-md);
    color: var(--danger-text);
    font-size: var(--font-size-sm);
    box-shadow: var(--shadow-md);
  }
</style>
