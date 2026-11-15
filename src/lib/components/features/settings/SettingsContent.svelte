<script lang="ts">
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import SectionModels from "./SectionModels.svelte";
  import SectionGeneral from "./SectionGeneral.svelte";
  import SectionPromptBase from "./SectionPromptBase.svelte";
  import SectionChat from "./SectionChat.svelte";
  import SectionContextMenu from "./SectionContextMenu.svelte";
  import SectionSkills from "./SectionSkills.svelte";
  import SectionNotifications from "./SectionNotifications.svelte";
  import SectionHistory from "./SectionHistory.svelte";
  import SectionSpeech from "./SectionSpeech.svelte";
  import SectionSync from "./SectionSync.svelte";
  import SectionMcpServers from "./SectionMcpServers.svelte";
  import SectionEnvironment from "./SectionEnvironment.svelte";
  import SectionShortcuts from "./SectionShortcuts.svelte";

  const store = getSettingsStore();
  const navigation = getSettingsNavigation();
  const activeSection = $derived(navigation.activeSection);
</script>

<main class="content" data-settings-content>
  {#if !store.settings}
    {#if store.error}
      <div class="error">Failed to load settings: {store.error}</div>
    {:else}
      <div class="loading">Loading…</div>
    {/if}
  {:else if activeSection === "models"}
    <SectionModels />
  {:else if activeSection === "prompt_base"}
    <SectionPromptBase />
  {:else if activeSection === "chat"}
    <SectionChat />
  {:else if activeSection === "context_menu"}
    <SectionContextMenu />
  {:else if activeSection === "notifications"}
    <SectionNotifications />
  {:else if activeSection === "general"}
    <SectionGeneral />
  {:else if activeSection === "skills"}
    <SectionSkills />
  {:else if activeSection === "mcp_servers"}
    <SectionMcpServers />
  {:else if activeSection === "history"}
    <SectionHistory />
  {:else if activeSection === "speech"}
    <SectionSpeech />
  {:else if activeSection === "sync"}
    <SectionSync />
  {:else if activeSection === "environment"}
    <SectionEnvironment />
  {:else if activeSection === "shortcuts"}
    <SectionShortcuts />
  {/if}
</main>

<style>
  .content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-width: 0;
    --column-width: 1040px;
    --column-gutter: max(0px, 100% - var(--column-width));
  }

  .loading,
  .error {
    margin: auto;
    color: var(--text-muted);
    font-size: var(--font-size-base);
    padding: var(--space-12);
    text-align: center;
  }

  .error {
    color: var(--danger);
  }
</style>
