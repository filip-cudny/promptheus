<script lang="ts">
  import { AlertCircle } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import Button from "$lib/components/shared/ui/Button.svelte";

  let {
    message,
    onRetry,
    variant = "error",
  }: {
    message: string;
    onRetry?: () => void;
    variant?: "error" | "cancelled";
  } = $props();
</script>

{#if variant === "cancelled"}
  <span class="cancelled-hint">{message}</span>
{:else}
  <div class="error-banner">
    <AlertCircle size={ICON_SIZE.sm} />
    <span class="error-text">{message}</span>
    {#if onRetry}
      <Button onclick={onRetry}>Retry</Button>
    {/if}
  </div>
{/if}

<style>
  .error-banner {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-top: var(--space-3);
    padding: var(--space-4) var(--space-5);
    background: var(--danger-bg-soft);
    border-radius: var(--radius-md);
    color: var(--danger-text);
    font-size: var(--font-size-md);
  }

  .error-banner :global(svg:first-child) {
    flex-shrink: 0;
  }

  .error-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: break-word;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }

  .cancelled-hint {
    display: block;
    margin-top: var(--space-3);
    font-size: var(--font-size-sm);
    font-style: italic;
    color: var(--text-disabled);
  }
</style>
