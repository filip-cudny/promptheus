<script lang="ts">
  import { modelUseLabel } from "$lib/constants/surfaces";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import type { ModelUse } from "$lib/utils/modelUses";
  import type { SurfaceKind } from "$lib/types";

  let {
    uses,
    onShowUse,
  }: {
    uses: ModelUse[];
    onShowUse: (surface: SurfaceKind) => void;
  } = $props();

  const navigation = getSettingsNavigation();

  function handleClick(e: MouseEvent, use: ModelUse) {
    e.stopPropagation();
    if (use.kind === "skill") void navigation.navigateTo(`skills[${use.skill}]`);
    else onShowUse(use.surface);
  }
</script>

<span class="tags">
  {#each uses as use (modelUseLabel(use))}
    <button
      class="tag"
      class:inherited={use.kind === "surface" && use.inherited}
      title={use.kind === "skill" ? `Open skill ${use.skill}` : "Show in Assignments"}
      onclick={(e) => handleClick(e, use)}
    >
      {modelUseLabel(use)}
    </button>
  {/each}
</span>

<style>
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .tag {
    font: inherit;
    font-size: var(--font-size-xs);
    padding: 1px var(--space-3);
    background: var(--tag-bg);
    color: var(--text-primary);
    border: 1px solid var(--tag-border);
    border-radius: var(--radius-xl);
    line-height: 1.4;
    white-space: nowrap;
    cursor: pointer;
  }

  .tag.inherited {
    background: transparent;
    border-style: dashed;
  }

  .tag:hover {
    background: var(--tag-bg);
    text-decoration: underline;
  }
</style>
