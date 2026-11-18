<script lang="ts">
  import ModelSelector from "$lib/components/shared/widgets/ModelSelector.svelte";
  import type { ModelConfig, ModelCapabilities, Provider } from "$lib/types";

  type SettingsModel = {
    id: string;
    display_name: string;
    model: string;
    provider: Provider;
    group: string | null;
  };

  type Props = {
    models: SettingsModel[];
    sttModels: SettingsModel[];
    defaultModelId: string | null;
    reasoningEffort: string | null;
    sttModelId: string | null;
    quickActionCapabilities: ModelCapabilities | null;
    preventDismiss: { suppress: () => void; resume: () => void };
    onModelSelect: (id: string) => Promise<void>;
    onReasoningSelect: (effort: string | null) => Promise<void>;
    onSttSelect: (id: string) => Promise<void>;
    onHover: () => void;
  };

  let {
    models,
    sttModels,
    defaultModelId,
    reasoningEffort,
    sttModelId,
    quickActionCapabilities,
    preventDismiss,
    onModelSelect,
    onReasoningSelect,
    onSttSelect,
    onHover,
  }: Props = $props();

  function toModelConfig(m: SettingsModel, type: "text" | "stt"): ModelConfig {
    return {
      id: m.id,
      type,
      model: m.model,
      display_name: m.display_name,
      provider: m.provider,
      group: m.group,
      api_key: null,
      base_url: null,
      parameters: null,
      context_window_size: null,
      api_mode: null,
      store: true,
    };
  }
</script>

{#if models.length > 0}
  <div class="panel-label">Quick action model</div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="models-row" onmouseenter={onHover}>
    <ModelSelector
      models={models.map((m) => toModelConfig(m, "text"))}
      selectedModelId={defaultModelId}
      {reasoningEffort}
      capabilities={quickActionCapabilities}
      onModelSelect={onModelSelect}
      onReasoningSelect={onReasoningSelect}
      {preventDismiss}
    />
  </div>
{/if}
{#if sttModels.length > 0}
  <div class="panel-label">Speech-to-text model</div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="models-row" onmouseenter={onHover}>
    <ModelSelector
      models={sttModels.map((m) => toModelConfig(m, "stt"))}
      selectedModelId={sttModelId}
      reasoningEffort={null}
      onModelSelect={onSttSelect}
      onReasoningSelect={() => {}}
      {preventDismiss}
    />
  </div>
{/if}

<style>
  .panel-label {
    font-size: var(--font-size-sm);
    color: var(--text-disabled);
    margin-bottom: var(--space-2);
  }

  .models-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-6);
  }
</style>
