<script lang="ts">
  import SettingsSection from "./SettingsSection.svelte";
  import ShortcutField from "./ShortcutField.svelte";

  const CONTEXT_MENU_ACTIONS = [
    "open_context_menu",
    "execute_active_prompt",
    "set_context_value",
    "append_context_value",
    "clear_context",
  ];
</script>

<SettingsSection
  title="Keyboard shortcuts"
  hint="Global shortcuts work in every app. One action can have several shortcuts."
>
  {#snippet body()}
    <p class="muted">
      On GNOME Wayland global shortcuts work through the Promptheus GNOME Shell extension. On
      macOS the app needs the Accessibility permission. A shortcut taken by another app does not
      fire.
    </p>

    <div class="group">
      <h3>Context menu</h3>

      {#each CONTEXT_MENU_ACTIONS as action (action)}
        <ShortcutField {action} />
      {/each}
    </div>

    <div class="group">
      <h3>Speech</h3>

      <ShortcutField action="speech_to_text_toggle" />
    </div>
  {/snippet}
</SettingsSection>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 560px;
  }

  h3 {
    margin: 0;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
    color: var(--text-disabled);
  }

  .muted {
    margin: 0;
    max-width: 560px;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }
</style>
