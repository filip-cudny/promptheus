<script lang="ts">
  import { error as logError } from "@tauri-apps/plugin-log";
  import SelectDropdown from "$lib/components/shared/ui/SelectDropdown.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { getCachedCapabilities, prefetchCapabilities } from "$lib/stores/capabilities.svelte";
  import { updateSurfaceModel, updateSurfaceReasoningEffort } from "$lib/services/settings";
  import { effortOptions } from "$lib/utils/effortOptions";
  import { agentModelId } from "$lib/utils/agentModel";
  import { sameAsChatLabel } from "$lib/utils/modelUses";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingField from "./SettingField.svelte";
  import PromptEditor from "./PromptEditor.svelte";
  import WebviewProvidersEditor from "./WebviewProvidersEditor.svelte";
  import SurfaceToolsPicker from "./SurfaceToolsPicker.svelte";

  const store = getSettingsStore();
  const chat = $derived(store.settings?.surfaces.chat.generation ?? null);
  const titleModelId = $derived(
    store.settings?.surfaces.title_generation.generation.model_id ?? null,
  );

  const agent = $derived(store.settings?.surfaces.settings_agent.generation ?? null);
  const effectiveAgentModelId = $derived(
    store.settings ? agentModelId(store.settings.surfaces) : null,
  );

  const textModels = $derived(
    store.models
      .filter((m) => m.type === "text")
      .map((m) => ({ value: m.id, label: m.display_name || m.model })),
  );
  const chatModelOptions = $derived([{ value: "", label: "Not set" }, ...textModels]);
  const inheritingModelOptions = $derived([
    {
      value: "",
      label: store.settings ? sameAsChatLabel(store.settings.surfaces, store.models) : "Same as Chat",
    },
    ...textModels,
  ]);

  const chatEffortOptions = $derived(
    effortOptions(
      getCachedCapabilities(chat?.model_id ?? null)?.reasoning,
      chat?.parameters.reasoning_effort,
    ),
  );

  const agentEffortOptions = $derived(
    effortOptions(
      getCachedCapabilities(effectiveAgentModelId)?.reasoning,
      agent?.parameters.reasoning_effort,
    ),
  );

  $effect(() => prefetchCapabilities(chat?.model_id ?? null));
  $effect(() => prefetchCapabilities(effectiveAgentModelId));

  async function run(label: string, write: () => Promise<void>) {
    try {
      await write();
    } catch (e) {
      logError(`${label} failed: ${e}`);
    }
  }
</script>

<SettingsSection
  title="Chat"
  hint="The chat window: its model, tools and web chat providers."
>
  {#snippet body()}
    {#if chat}
      <div class="group">
        <SettingField key="surfaces.chat.generation.model_id">
          <SelectDropdown
            value={chat.model_id ?? ""}
            options={chatModelOptions}
            ariaLabel="Chat model"
            onchange={(id) =>
              run("update_surface_model chat", () => updateSurfaceModel("chat", id || null))}
          />
        </SettingField>

        <SettingField key="surfaces.chat.generation.parameters.reasoning_effort">
          <SelectDropdown
            value={chat.parameters.reasoning_effort ?? ""}
            options={chatEffortOptions}
            ariaLabel="Reasoning effort"
            onchange={(v) =>
              run("update reasoning_effort chat", () =>
                updateSurfaceReasoningEffort("chat", v || null),
              )}
          />
        </SettingField>

        <SettingField key="surfaces.chat.generation.enabled_tools">
          <SurfaceToolsPicker surface="chat" enabled={chat.enabled_tools} />
        </SettingField>

        <SettingField key="surfaces.title_generation.generation.model_id">
          <SelectDropdown
            value={titleModelId ?? ""}
            options={inheritingModelOptions}
            ariaLabel="Title model"
            onchange={(id) =>
              run("update_surface_model title_generation", () =>
                updateSurfaceModel("title_generation", id || null),
              )}
          />
        </SettingField>

        {#if agent}
          <SettingField key="surfaces.settings_agent.generation.model_id">
            <SelectDropdown
              value={agent.model_id ?? ""}
              options={inheritingModelOptions}
              ariaLabel="Settings agent model"
              onchange={(id) =>
                run("update_surface_model settings_agent", () =>
                  updateSurfaceModel("settings_agent", id || null),
                )}
            />
          </SettingField>

          <SettingField key="surfaces.settings_agent.generation.parameters.reasoning_effort">
            <SelectDropdown
              value={agent.parameters.reasoning_effort ?? ""}
              options={agentEffortOptions}
              ariaLabel="Settings agent reasoning effort"
              onchange={(v) =>
                run("update reasoning_effort settings_agent", () =>
                  updateSurfaceReasoningEffort("settings_agent", v || null),
                )}
            />
          </SettingField>

          <SettingField key="surfaces.settings_agent.generation.enabled_tools">
            <SurfaceToolsPicker surface="settings_agent" enabled={agent.enabled_tools} />
          </SettingField>
        {/if}

        <div class="prompt">
          <PromptEditor
            kind="title_generation"
            minHeight={0}
            description="Used to auto-name conversations after the first user message."
          />
        </div>

        <WebviewProvidersEditor />
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

  .prompt {
    display: flex;
    flex-direction: column;
    height: 280px;
  }
</style>
