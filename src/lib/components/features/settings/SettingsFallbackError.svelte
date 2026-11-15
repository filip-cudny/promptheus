<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { CircleAlert } from "lucide-svelte";
  import { warn as logWarn } from "@tauri-apps/plugin-log";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import { onSettingsChanged } from "$lib/services/events";
  import {
    getSettingsFallback,
    retrySettingsLoad,
    type SettingsFallback,
  } from "$lib/services/settings";

  let fallback = $state<SettingsFallback | null>(null);
  let retrying = $state(false);
  let unlisten: (() => void) | null = null;

  async function refresh() {
    try {
      fallback = await getSettingsFallback();
    } catch (e) {
      logWarn("Failed to read get_settings_fallback: " + e);
    }
  }

  async function retry() {
    retrying = true;
    try {
      fallback = await retrySettingsLoad();
    } catch (e) {
      logWarn("Failed to retry the settings load: " + e);
      await refresh();
    } finally {
      retrying = false;
    }
  }

  onMount(async () => {
    unlisten = await onSettingsChanged(refresh);
    await refresh();
  });

  onDestroy(() => unlisten?.());
</script>

{#if fallback}
  <div class="fallback-error" role="alert">
    <CircleAlert size={16} />
    <div class="body">
      <p class="title">settings.json could not be read: {fallback.error}</p>
      <p class="hint">
        Promptheus runs on the last good copy. Changes are not saved until you fix
        <code>{fallback.path}</code>.
      </p>
    </div>
    <Button onclick={retry} disabled={retrying}>Retry</Button>
  </div>
{/if}

<style>
  .fallback-error {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-6);
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border-default);
    color: var(--danger);
    font-size: var(--font-size-base);
  }

  .body {
    flex: 1;
    min-width: 0;
    color: var(--text-primary);
  }

  p {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .hint {
    margin-top: var(--space-2);
    color: var(--text-secondary);
  }
</style>
