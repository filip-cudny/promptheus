<script lang="ts">
  import { X } from "lucide-svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";

  let { onCancel }: { onCancel: () => void } = $props();
</script>

<div class="capture-bar">
  <span class="status">Select text…</span>
  <span class="spacer"></span>
  <ActionIconButton icon={X} label="Cancel" onclick={onCancel} />
</div>

<style>
  .capture-bar {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .status {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    user-select: none;
  }

  .spacer {
    flex: 1;
  }
</style>
