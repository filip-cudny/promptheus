<script lang="ts">
  import type { Snippet } from "svelte";
  import { warn as logWarn } from "@tauri-apps/plugin-log";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { getSettingsCatalogStore } from "$lib/stores/settingsCatalog.svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import { catalogPath } from "$lib/utils/settingsSearch";
  import FormRow from "$lib/components/shared/ui/FormRow.svelte";

  let {
    key,
    itemId,
    children,
  }: {
    key: string;
    itemId?: string;
    children: Snippet;
  } = $props();

  const store = getSettingsStore();
  const catalog = getSettingsCatalogStore();
  const navigation = getSettingsNavigation();

  const entry = $derived(catalog.entry(key));
  const restartNote = $derived(
    entry?.applies === "after_restart" ? "Applies after restart." : "",
  );
  const hint = $derived(
    [entry?.description, restartNote].filter(Boolean).join(" ") || undefined,
  );
  const error = $derived(
    Object.entries(store.fieldErrors).find(
      ([concrete]) =>
        catalogPath(concrete) === key &&
        (itemId === undefined || concrete.includes(`[${itemId}]`)),
    )?.[1],
  );

  const highlighted = $derived(
    (navigation.highlighted !== null && catalogPath(navigation.highlighted) === key) ||
      navigation.isPersistentTarget(key),
  );

  $effect(() => {
    if (catalog.loaded && !entry) logWarn(`settings catalog has no entry for ${key}`);
  });
</script>

<div id={`setting-${key}`} class:highlighted>
  <FormRow label={entry?.label ?? key} {hint} {error}>
    {@render children()}
  </FormRow>
</div>

<style>
  .highlighted {
    border-radius: var(--radius-md);
    outline: 2px solid var(--accent);
    outline-offset: 4px;
    background: var(--accent-bg-soft);
  }
</style>
