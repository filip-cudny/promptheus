<script lang="ts">
  import { Moon, Sun } from "lucide-svelte";
  import { getTheme, setTheme } from "$lib/stores/theme.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { updateSetting } from "$lib/services/settings";
  import { error as logError } from "@tauri-apps/plugin-log";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import SelectDropdown from "$lib/components/shared/ui/SelectDropdown.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingField from "./SettingField.svelte";
  import type { AppearanceSettings, Skin } from "$lib/types";

  const SKIN_OPTIONS: ReadonlyArray<{ value: Skin; label: string }> = [
    { value: "color", label: "Color" },
    { value: "mono", label: "Mono" },
  ];

  const store = getSettingsStore();
  const appearance = $derived(store.settings?.appearance ?? null);
  const settings = $derived(store.settings);

  async function save(patch: Partial<AppearanceSettings>) {
    if (!appearance) return;
    try {
      await updateSetting("appearance", { ...appearance, ...patch });
    } catch (e) {
      logError(`update_setting appearance failed: ${e}`);
    }
  }

  async function saveToggle(key: "launch_at_startup" | "show_tray_icon", value: boolean) {
    try {
      await updateSetting(key, value);
    } catch (e) {
      logError(`update_setting ${key} failed: ${e}`);
    }
  }
</script>

<SettingsSection title="General" hint="How the app looks and starts.">
  {#snippet body()}
    <div class="group">
      <SettingField key="theme">
        <Button
          icon={getTheme() === "dark" ? Sun : Moon}
          onclick={() => setTheme(getTheme() === "dark" ? "light" : "dark")}
        >
          {getTheme() === "dark" ? "Switch to light mode" : "Switch to dark mode"}
        </Button>
      </SettingField>
      {#if appearance}
        <SettingField key="appearance.logo">
          <SelectDropdown
            value={appearance.logo}
            options={SKIN_OPTIONS}
            ariaLabel="Logo"
            onchange={(logo) => save({ logo })}
          />
        </SettingField>
        <SettingField key="appearance.icons">
          <SelectDropdown
            value={appearance.icons}
            options={SKIN_OPTIONS}
            ariaLabel="Icons"
            onchange={(icons) => save({ icons })}
          />
        </SettingField>
      {/if}
      {#if settings}
        <SettingField key="launch_at_startup">
          <label class="check">
            <input
              type="checkbox"
              checked={settings.launch_at_startup}
              onchange={(e) => saveToggle("launch_at_startup", e.currentTarget.checked)}
            />
          </label>
        </SettingField>
        <SettingField key="show_tray_icon">
          <label class="check">
            <input
              type="checkbox"
              checked={settings.show_tray_icon}
              onchange={(e) => saveToggle("show_tray_icon", e.currentTarget.checked)}
            />
          </label>
        </SettingField>
      {/if}
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

  .check input {
    cursor: pointer;
  }
</style>
