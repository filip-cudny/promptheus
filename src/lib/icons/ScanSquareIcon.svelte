<script lang="ts">
  import { ScanSquare } from "lucide-static";

  let {
    size = 24,
    strokeWidth = 2,
  }: {
    size?: number;
    strokeWidth?: number;
    [key: string]: unknown;
  } = $props();
</script>

<span class="icon" style:--icon-size="{size}px" style:--icon-stroke-width={strokeWidth} aria-hidden="true">{@html ScanSquare}</span>

<style>
  .icon {
    display: inline-flex;
    flex: none;
    width: var(--icon-size);
    height: var(--icon-size);
  }

  .icon :global(svg) {
    width: 100%;
    height: 100%;
    stroke-width: var(--icon-stroke-width);
  }
</style>
