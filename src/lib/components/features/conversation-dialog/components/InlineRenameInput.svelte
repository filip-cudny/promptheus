<script lang="ts">
  import { untrack } from "svelte";

  let {
    initial,
    onCommit,
    onCancel,
  }: {
    initial: string;
    onCommit: (newValue: string) => void;
    onCancel: () => void;
  } = $props();

  let value = $state(untrack(() => initial));
  let cancelled = $state(false);

  function handleBlur() {
    if (cancelled) {
      onCancel();
      return;
    }
    onCommit(value);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelled = true;
      (e.target as HTMLInputElement).blur();
    }
  }
</script>

<!-- svelte-ignore a11y_autofocus -->
<input
  class="tab-name-input"
  type="text"
  bind:value
  autofocus
  onclick={(e: MouseEvent) => e.stopPropagation()}
  onblur={handleBlur}
  onkeydown={handleKeydown}
/>

<style>
  .tab-name-input {
    width: 100%;
    font-size: var(--font-size-base);
    font-family: inherit;
    color: var(--text-primary);
    background: var(--surface-overlay);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: var(--space-0) var(--space-2);
    outline: none;
    line-height: inherit;
  }

  .tab-name-input:focus {
    border-color: var(--border-strong);
  }
</style>
