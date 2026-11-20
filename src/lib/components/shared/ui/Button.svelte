<script lang="ts">
  import type { ComponentType, Snippet, SvelteComponent } from "svelte";
  import type { IconProps } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";

  type LucideIcon = ComponentType<SvelteComponent<IconProps>>;

  let {
    variant = "ghost",
    icon: Icon,
    disabled = false,
    onclick,
    children,
    ...rest
  }: {
    variant?: "primary" | "ghost" | "danger";
    icon?: LucideIcon;
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
    [key: string]: unknown;
  } = $props();
</script>

<button
  class="btn btn-{variant}"
  type="button"
  {disabled}
  {onclick}
  {...rest}
>
  {#if Icon}<Icon size={ICON_SIZE.md} />{/if}
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    transition: background var(--motion-fast) var(--ease-default),
                color var(--motion-fast) var(--ease-default);
    white-space: nowrap;
    padding: var(--space-2) var(--space-4);
  }

  .btn:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  /* --- ghost (default) --- */
  .btn-ghost {
    background: transparent;
    color: var(--text-secondary);
  }
  .btn-ghost:hover:not(:disabled) {
    background: var(--surface-overlay);
    color: var(--text-primary);
  }

  /* --- primary --- */
  .btn-primary {
    background: var(--button-primary-bg);
    color: var(--accent-fg);
    padding: var(--space-2) var(--space-6);
  }
  .btn-primary:hover:not(:disabled) {
    background: var(--button-primary-bg-hover);
    color: var(--accent-fg);
  }

  /* --- danger --- */
  .btn-danger {
    background: var(--danger-bg-soft);
    color: var(--danger-text);
    border: 1px solid var(--danger-border);
  }
  .btn-danger:hover:not(:disabled) {
    border-color: var(--danger);
  }
</style>
