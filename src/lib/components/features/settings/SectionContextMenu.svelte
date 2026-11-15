<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { GripVertical } from "lucide-svelte";
  import { error as logError } from "@tauri-apps/plugin-log";
  import SelectDropdown from "$lib/components/shared/ui/SelectDropdown.svelte";
  import NumberInput from "$lib/components/shared/ui/NumberInput.svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { useSaveTracker } from "$lib/stores/saveTracker.svelte";
  import { getCachedCapabilities, prefetchCapabilities } from "$lib/stores/capabilities.svelte";
  import {
    updateSetting,
    updateSurfaceModel,
    updateSurfaceReasoningEffort,
  } from "$lib/services/settings";
  import { effortOptions } from "$lib/utils/effortOptions";
  import { quickActionsModelId, sameAsChatLabel } from "$lib/utils/modelUses";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingField from "./SettingField.svelte";

  const SECTION_NAMES: Record<string, string> = {
    chat: "Chat",
    skills: "Skills",
    prompts: "Skills",
    SkillProvider: "Skills",
    models: "Models",
    settings: "Settings",
    speech: "Speech",
    SpeechMenuProvider: "Speech",
    lastInteraction: "Last interaction",
    LastInteractionMenuProvider: "Last interaction",
    context: "Context",
    ContextMenuProvider: "Context",
  };

  const store = getSettingsStore();
  const quick = $derived(store.settings?.surfaces.quick_actions.generation ?? null);
  const order = $derived(store.settings?.menu_section_order ?? []);

  const effectiveQuickModelId = $derived(
    store.settings ? quickActionsModelId(store.settings.surfaces) : null,
  );

  const modelOptions = $derived([
    {
      value: "",
      label: store.settings ? sameAsChatLabel(store.settings.surfaces, store.models) : "Same as Chat",
    },
    ...store.models
      .filter((m) => m.type === "text")
      .map((m) => ({ value: m.id, label: m.display_name || m.model })),
  ]);
  const quickEffortOptions = $derived(
    effortOptions(
      getCachedCapabilities(effectiveQuickModelId)?.reasoning,
      quick?.parameters.reasoning_effort,
    ),
  );

  $effect(() => prefetchCapabilities(effectiveQuickModelId));

  async function run(label: string, write: () => Promise<void>) {
    try {
      await write();
    } catch (e) {
      logError(`${label} failed: ${e}`);
    }
  }

  type DebouncedKey = "recent_apps_count" | "menu_digit_select_delay_ms";

  const tracker = useSaveTracker({ debounceMs: store.settings?.autosave_debounce_ms ?? 1000 });
  const pending: Partial<Record<DebouncedKey, number>> = {};

  async function persistPending() {
    const entries = Object.entries(pending) as [DebouncedKey, number][];
    for (const [key] of entries) delete pending[key];
    await Promise.all(entries.map(([key, n]) => updateSetting(key, n)));
  }

  function debounceSave(key: DebouncedKey, n: number) {
    pending[key] = n;
    tracker.scheduleSave(persistPending);
  }

  onMount(() => {
    tracker.attachKeyboard(window);
  });

  onDestroy(() => {
    tracker.destroy();
  });

  function saveDigitDelay(e: Event) {
    const n = Math.floor(Number((e.currentTarget as HTMLInputElement).value));
    if (n < 100 || n > 1000) return;
    debounceSave("menu_digit_select_delay_ms", n);
  }

  function saveRecentApps(e: Event) {
    const n = Math.max(0, Math.floor(Number((e.currentTarget as HTMLInputElement).value)));
    if (!Number.isFinite(n)) return;
    debounceSave("recent_apps_count", n);
  }

  let dragIndex = $state<number | null>(null);

  function drop(to: number) {
    const from = dragIndex;
    dragIndex = null;
    if (from === null || from === to) return;
    const next = [...order];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    void run("update menu_section_order", () => updateSetting("menu_section_order", next));
  }
</script>

<SettingsSection
  title="Context menu"
  hint="The menu opened over selected text: its model and its layout."
>
  {#snippet body()}
    {#if quick}
      <div class="group">
        <h3>Model</h3>

        <SettingField key="surfaces.quick_actions.generation.model_id">
          <SelectDropdown
            value={quick.model_id ?? ""}
            options={modelOptions}
            ariaLabel="Quick actions model"
            onchange={(id) =>
              run("update_surface_model quick_actions", () =>
                updateSurfaceModel("quick_actions", id || null),
              )}
          />
        </SettingField>

        <SettingField key="surfaces.quick_actions.generation.parameters.reasoning_effort">
          <SelectDropdown
            value={quick.parameters.reasoning_effort ?? ""}
            options={quickEffortOptions}
            ariaLabel="Reasoning effort"
            onchange={(v) =>
              run("update reasoning_effort quick_actions", () =>
                updateSurfaceReasoningEffort("quick_actions", v || null),
              )}
          />
        </SettingField>

        <h3>Menu</h3>

        <SettingField key="menu_section_order">
          <ul class="order">
            {#each order as id, i (id)}
              <li
                class:dragging={dragIndex === i}
                draggable="true"
                ondragstart={() => (dragIndex = i)}
                ondragover={(e) => e.preventDefault()}
                ondrop={() => drop(i)}
                ondragend={() => (dragIndex = null)}
              >
                <GripVertical size={ICON_SIZE.sm} />
                <span>{SECTION_NAMES[id] ?? id}</span>
              </li>
            {/each}
          </ul>
        </SettingField>

        <SettingField key="recent_apps_count">
          <NumberInput
            min={0}
            step={1}
            value={store.settings?.recent_apps_count ?? 0}
            oninput={saveRecentApps}
          />
        </SettingField>

        <SettingField key="menu_digit_select_delay_ms">
          <NumberInput
            min={100}
            max={1000}
            step={50}
            value={store.settings?.menu_digit_select_delay_ms ?? 200}
            oninput={saveDigitDelay}
          />
        </SettingField>
      </div>
    {/if}
  {/snippet}
</SettingsSection>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    max-width: 760px;
  }

  h3 {
    margin: 0;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
    color: var(--text-disabled);
  }

  .order {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .order li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-sunken);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
    font-size: var(--font-size-md);
    color: var(--text-primary);
    cursor: grab;
  }

  .order li.dragging {
    opacity: 0.5;
  }
</style>
