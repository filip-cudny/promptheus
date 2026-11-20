<script lang="ts">
  import { X } from "lucide-svelte";
  import type { ComponentType, SvelteComponent } from "svelte";
  import type { IconProps } from "lucide-svelte";

  type LucideIcon = ComponentType<SvelteComponent<IconProps>>;

  let {
    label,
    icon,
    ondismiss,
  }: {
    label: string;
    icon?: LucideIcon;
    ondismiss: () => void;
  } = $props();
</script>

<button class="tool-chip" title="Remove {label}" aria-label="Remove {label}" onclick={ondismiss}>
  {#if icon}
    {@const Icon = icon}
    <Icon size={14} />
  {:else}
    <span class="tool-chip-label">{label}</span>
  {/if}
  <span class="tool-chip-dismiss">
    <X size={10} />
  </span>
</button>

<style>
  .tool-chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-0);
    width: 24px;
    height: 24px;
    padding: var(--space-0);
    background: var(--accent-bg-soft);
    border: 1px solid var(--accent-bg);
    color: var(--accent-text);
    border-radius: var(--radius-lg);
    font: inherit;
    font-size: var(--font-size-sm);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    overflow: hidden;
    transition: background var(--motion-default) var(--ease-default), width var(--motion-default) var(--ease-default);
  }

  .tool-chip:hover {
    background: var(--accent-bg);
    width: 36px;
  }

  .tool-chip-label {
    line-height: 1;
  }

  .tool-chip-dismiss {
    display: inline-flex;
    align-items: center;
    width: 0;
    opacity: 0;
    overflow: hidden;
    transition: width var(--motion-default) var(--ease-default), opacity var(--motion-default) var(--ease-default), margin var(--motion-default) var(--ease-default);
    margin-left: var(--space-0);
    color: var(--accent-text);
    pointer-events: none;
  }

  .tool-chip:hover .tool-chip-dismiss {
    width: 10px;
    opacity: 1;
    margin-left: var(--space-1);
  }
</style>
