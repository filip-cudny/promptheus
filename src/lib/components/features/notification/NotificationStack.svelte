<script lang="ts">
  import { Toaster } from "svelte-sonner";
  import { CircleCheck, CircleX, Info, CircleAlert } from "lucide-svelte";
  import type { ToasterHandlers } from "./drivers/useNotificationDrain.svelte";

  let { expand = false, handlers }: { expand?: boolean; handlers?: ToasterHandlers } = $props();
</script>

<Toaster
  position="bottom-right"
  closeButton={false}
  richColors={false}
  duration={2000}
  {expand}
  gap={14}
  onmouseenter={handlers?.onmouseenter}
  onmouseleave={handlers?.onmouseleave}
  onpointerdown={handlers?.onpointerdown}
  onpointerup={handlers?.onpointerup}
  onclick={handlers?.onclick}
>
  {#snippet successIcon()}
    <CircleCheck size={20} strokeWidth={2} />
  {/snippet}
  {#snippet errorIcon()}
    <CircleX size={20} strokeWidth={2} />
  {/snippet}
  {#snippet infoIcon()}
    <Info size={20} strokeWidth={2} />
  {/snippet}
  {#snippet warningIcon()}
    <CircleAlert size={20} strokeWidth={2} />
  {/snippet}
</Toaster>

<style>
  :global([data-sonner-toaster]) {
    --border-radius: 8px;
    --offset-right: 0px;
    --offset-bottom: 0px;
    --offset-top: 0px;
    --offset-left: 0px;
    inset: 0 !important;
    width: auto !important;
    height: auto !important;
    font-family: var(--font-sans) !important;
  }

  :global([data-sonner-toast]) {
    width: 300px !important;
    background: var(--notification-bg) !important;
    border: 1px solid rgba(200, 200, 200, 0.9) !important;
    box-shadow: none !important;
    padding: var(--space-6) var(--space-8) !important;
  }

  :global([data-sonner-toast][data-y-position="bottom"][data-x-position="right"]) {
    bottom: 20px !important;
    right: 20px !important;
    left: auto !important;
  }

  :global(html.linux [data-sonner-toast]),
  :global(html.linux [data-sonner-toast] *) {
    transition: none !important;
    animation: none !important;
  }

  :global([data-sonner-toast].p-clickable) {
    cursor: pointer;
  }

  :global([data-sonner-toast] [data-title]) {
    color: var(--notification-fg) !important;
    font-weight: var(--font-weight-semibold) !important;
    font-size: var(--font-size-lg) !important;
  }

  :global([data-sonner-toast] [data-description]) {
    color: var(--notification-fg-secondary) !important;
    font-size: var(--font-size-base) !important;
  }

  :global([data-sonner-toast] [data-icon]) {
    width: 20px;
    height: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  :global(.p-mono [data-icon]) {
    color: var(--notification-fg);
  }
  :global(.p-color-success [data-icon]) {
    color: var(--success);
  }
  :global(.p-color-error [data-icon]) {
    color: var(--danger);
  }
  :global(.p-color-info [data-icon]) {
    color: #6a7d93;
  }
  :global(.p-color-warning [data-icon]) {
    color: var(--warning);
  }
</style>
