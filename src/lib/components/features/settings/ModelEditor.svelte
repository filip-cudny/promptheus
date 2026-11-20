<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { Copy, Check } from "lucide-svelte";
  import { error as logError, warn as logWarn } from "@tauri-apps/plugin-log";
  import { modelUseLabel } from "$lib/constants/surfaces";
  import type { ModelUse } from "$lib/utils/modelUses";
  import EnvVarPicker from "./EnvVarPicker.svelte";
  import ParametersKnown from "./ParametersKnown.svelte";
  import CapabilitiesEditor from "./CapabilitiesEditor.svelte";
  import ParametersCustom, {
    entriesFromExtra,
    entriesToExtra,
    type CustomParamEntry,
  } from "./ParametersCustom.svelte";
  import SettingField from "./SettingField.svelte";
  import DangerZone from "./DangerZone.svelte";
  import ModelUseTags from "./ModelUseTags.svelte";
  import CrossLink from "./CrossLink.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import SaveStatusIndicator from "$lib/components/shared/widgets/SaveStatusIndicator.svelte";
  import { useSaveTracker } from "$lib/stores/saveTracker.svelte";
  import {
    addModel,
    deleteModel,
    getEnvVars,
    updateModel,
  } from "$lib/services/settings";
  import { onSettingsChanged } from "$lib/services/events";
  import { parseEnvRef } from "$lib/utils/envVarOptions";
  import { envLink } from "$lib/utils/settingsCrossLinks";
  import {
    prefetchCapabilities,
    getCachedCapabilities,
  } from "$lib/stores/capabilities.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { generateId } from "$lib/utils/id";
  import {
    KNOWN_MODEL_PARAMETER_KEYS,
    type ApiMode,
    type KnownModelParameterKey,
    type ModelCapabilities,
    type ModelConfig,
    type ModelParameters,
    type EnvVarStatus,
    type ModelType,
    type Provider,
    type SurfaceKind,
  } from "$lib/types";

  let headHeight = $state(0);

  let {
    model,
    uses,
    onShowUse,
    onShowAssignments,
    debounceMs,
    onDeleted,
    onDuplicated,
  }: {
    model: ModelConfig;
    uses: ModelUse[];
    onShowUse: (surface: SurfaceKind) => void;
    onShowAssignments: () => void;
    debounceMs: number;
    onDeleted: () => void;
    onDuplicated: (id: string) => void;
  } = $props();

  const settingsStore = getSettingsStore();

  function errorFor(field: string): string | undefined {
    return (
      validationErrors[field] ??
      settingsStore.fieldError(`models[${model.id}].${field}`)
    );
  }

  const usesLabel = $derived(uses.map(modelUseLabel).join(", "));

  let draft = $state<ModelConfig>(untrack(() => structuredClone(model)));
  let customEntries = $state<CustomParamEntry[]>(
    untrack(() => entriesFromExtra(extraOf(model.parameters))),
  );
  let customErrors = $state<Record<string, string>>({});
  let validationErrors = $state<Record<string, string>>({});

  const tracker = useSaveTracker({ debounceMs: untrack(() => debounceMs) });

  let envVars = $state<EnvVarStatus[]>([]);
  let unlistenEnv: (() => void) | null = null;

  const missingKeyVariable = $derived.by(() => {
    const name = parseEnvRef(draft.api_key);
    return name && envVars.some((v) => v.name === name && !v.set) ? name : null;
  });

  async function refreshEnvVars() {
    try {
      envVars = await getEnvVars();
    } catch (e) {
      logWarn("Failed to read get_env_vars: " + e);
    }
  }

  onMount(async () => {
    tracker.attachKeyboard(window);
    tracker.attachBeforeUnload(window);
    unlistenEnv = await onSettingsChanged(refreshEnvVars);
    await refreshEnvVars();
  });

  let suppressCustomEntriesSave = true;

  $effect(() => {
    const m = model;
    untrack(() => {
      if (tracker.dirty || tracker.saving || tracker.hasPending) return;
      draft = structuredClone(m);
      customEntries = entriesFromExtra(extraOf(m.parameters));
      customErrors = {};
      validationErrors = {};
      suppressCustomEntriesSave = true;
    });
  });

  onDestroy(() => {
    tracker.destroy();
    unlistenEnv?.();
  });

  function extraOf(params: ModelParameters | null | undefined): Record<string, unknown> {
    if (!params) return {};
    const out: Record<string, unknown> = {};
    const known = new Set<string>(KNOWN_MODEL_PARAMETER_KEYS);
    for (const [k, v] of Object.entries(params)) {
      if (known.has(k)) continue;
      out[k] = v;
    }
    return out;
  }

  function buildParametersToSend(): ModelParameters | null {
    const customResult = entriesToExtra(customEntries);
    customErrors = customResult.errors;

    const known: Partial<Record<KnownModelParameterKey, number | string | null>> = {
      temperature: numericOrNull(draft.parameters?.temperature),
      max_tokens: numericOrNull(draft.parameters?.max_tokens),
      top_p: numericOrNull(draft.parameters?.top_p),
      frequency_penalty: numericOrNull(draft.parameters?.frequency_penalty),
      presence_penalty: numericOrNull(draft.parameters?.presence_penalty),
      reasoning_effort: stringOrNull(draft.parameters?.reasoning_effort),
    };

    const allEmpty =
      Object.values(known).every((v) => v === null || v === undefined) &&
      Object.keys(customResult.extra).length === 0;
    if (allEmpty) return null;

    const merged: ModelParameters = {
      temperature: known.temperature as number | null,
      max_tokens: known.max_tokens as number | null,
      top_p: known.top_p as number | null,
      frequency_penalty: known.frequency_penalty as number | null,
      presence_penalty: known.presence_penalty as number | null,
      reasoning_effort: known.reasoning_effort as string | null,
      ...customResult.extra,
    };
    return merged;
  }

  function numericOrNull(v: unknown): number | null {
    return typeof v === "number" && Number.isFinite(v) ? v : null;
  }

  function stringOrNull(v: unknown): string | null {
    return typeof v === "string" && v.length > 0 ? v : null;
  }

  function validate(): boolean {
    const errs: Record<string, string> = {};
    if (!draft.display_name.trim()) errs.display_name = "Required";
    if (!draft.model.trim()) errs.model = "Required";
    validationErrors = errs;
    return Object.keys(errs).length === 0;
  }

  async function persist() {
    if (!validate()) return;
    const parameters = buildParametersToSend();
    if (Object.keys(customErrors).length > 0) return;

    const config: ModelConfig = {
      ...draft,
      parameters,
    };
    await updateModel(model.id, config);
  }

  function scheduleSave(immediate: boolean) {
    if (immediate) {
      void tracker.flush(persist);
    } else {
      tracker.scheduleSave(persist);
    }
  }

  function setKnownParameter(
    key: KnownModelParameterKey,
    value: number | string | null,
    immediate = true,
  ) {
    const params: ModelParameters =
      draft.parameters ?? {
        temperature: null,
        max_tokens: null,
        top_p: null,
        frequency_penalty: null,
        presence_penalty: null,
        reasoning_effort: null,
      };
    draft.parameters = { ...params, [key]: value as never };
    scheduleSave(immediate);
  }

  function setCapabilities(next: ModelCapabilities | null) {
    draft.capabilities = next;
    scheduleSave(true);
  }

  $effect(() => {
    customEntries;
    if (suppressCustomEntriesSave) {
      suppressCustomEntriesSave = false;
      return;
    }
    tracker.scheduleSave(persist);
  });

  async function handleDelete() {
    const ok = await tracker.flush(async () => {
      await deleteModel(model.id);
    });
    if (ok) onDeleted();
  }

  async function handleDuplicate() {
    const copy: ModelConfig = {
      ...structuredClone(draft),
      id: generateId(),
      display_name: `${draft.display_name || "Untitled"} (copy)`,
    };
    const ok = await tracker.flush(async () => {
      await addModel(copy);
    });
    if (ok) onDuplicated(copy.id);
  }

  async function copyId() {
    try {
      await navigator.clipboard.writeText(model.id);
    } catch (e) {
      logError(`Copy model id failed: ${e}`);
    }
  }

  const PROVIDER_OPTIONS: { value: Provider | ""; label: string }[] = [
    { value: "", label: "None" },
    { value: "openai", label: "OpenAI" },
    { value: "anthropic", label: "Anthropic" },
    { value: "gemini", label: "Gemini" },
    { value: "elevenlabs", label: "ElevenLabs" },
  ];

  const TYPE_OPTIONS: { value: ModelType; label: string }[] = [
    { value: "text", label: "Text" },
    { value: "stt", label: "STT" },
  ];

  const API_MODE_OPTIONS: { value: ApiMode; label: string }[] = [
    { value: "responses", label: "Responses (/v1/responses)" },
    { value: "completions", label: "Chat Completions (/v1/chat/completions)" },
  ];

  const baseUrlPlaceholder = $derived(
    draft.provider === "openai" ? "https://api.openai.com/v1" : "https://…",
  );

  const showApiMode = $derived(draft.type === "text" && draft.provider === "openai");
  const isText = $derived(draft.type === "text");

  $effect(() => {
    prefetchCapabilities(model?.id ?? null);
  });

  const resolvedCapabilities = $derived(
    draft.capabilities ?? getCachedCapabilities(model?.id ?? null),
  );
</script>

{#snippet inUseWarning()}
  It is used by {usesLabel}.
{/snippet}

<div class="model-editor" style:--sticky-head-height="{headHeight}px">
  <div class="sticky-head" bind:clientHeight={headHeight}>
    <header class="editor-header">
      <h1>{draft.display_name || "Untitled model"}</h1>
      <SaveStatusIndicator />
    </header>

    <div class="used-by">
      <span class="used-by-label">Used by</span>
      {#if uses.length > 0}
        <ModelUseTags {uses} {onShowUse} />
      {:else}
        <span class="used-by-label">
          nothing —
          <button class="link" onclick={onShowAssignments}>assign in Assignments ›</button>
        </span>
      {/if}
    </div>
  </div>

  <section class="card">
    <h3>Basic</h3>

    <SettingField key="models[].display_name" itemId={model.id}>
      <input
        id="display_name"
        type="text"
        class:error={errorFor("display_name")}
        value={draft.display_name}
        oninput={(e) => {
          draft.display_name = (e.target as HTMLInputElement).value;
          scheduleSave(false);
        }}
      />
      {#if validationErrors.display_name}
        <p class="field-error">{validationErrors.display_name}</p>
      {/if}
    </SettingField>

    <SettingField key="models[].model" itemId={model.id}>
      <input
        id="model_id"
        type="text"
        placeholder="gpt-4.1-mini"
        class:error={errorFor("model")}
        value={draft.model}
        oninput={(e) => {
          draft.model = (e.target as HTMLInputElement).value;
          scheduleSave(false);
        }}
      />
      {#if validationErrors.model}
        <p class="field-error">{validationErrors.model}</p>
      {/if}
    </SettingField>

    <SettingField key="models[].type" itemId={model.id}>
      <div class="segmented">
        {#each TYPE_OPTIONS as opt}
          <button
            class:active={draft.type === opt.value}
            onclick={() => {
              if (draft.type === opt.value) return;
              draft.type = opt.value;
              scheduleSave(true);
            }}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </SettingField>

    <SettingField key="models[].provider" itemId={model.id}>
      <select
        id="provider"
        value={draft.provider ?? ""}
        onchange={(e) => {
          const v = (e.target as HTMLSelectElement).value;
          draft.provider = (v as Provider) || null;
          scheduleSave(true);
        }}
      >
        {#each PROVIDER_OPTIONS as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </SettingField>

    <SettingField key="models[].group" itemId={model.id}>
      <input
        id="group"
        type="text"
        placeholder="Optional grouping"
        value={draft.group ?? ""}
        oninput={(e) => {
          const v = (e.target as HTMLInputElement).value;
          draft.group = v || null;
          scheduleSave(false);
        }}
      />
    </SettingField>
  </section>

  <section class="card">
    <h3>Connection</h3>

    <SettingField key="models[].api_key" itemId={model.id}>
      <EnvVarPicker
        value={draft.api_key ?? ""}
        onchange={(v) => {
          draft.api_key = v || null;
          scheduleSave(true);
        }}
      />
    </SettingField>

    {#if missingKeyVariable}
      <CrossLink target={envLink(missingKeyVariable)} label="Set key">
        {missingKeyVariable} has no value on this machine.
      </CrossLink>
    {/if}

    <SettingField key="models[].base_url" itemId={model.id}>
      <input
        id="base_url"
        type="text"
        placeholder={baseUrlPlaceholder}
        class:error={errorFor("base_url")}
        value={draft.base_url ?? ""}
        oninput={(e) => {
          const v = (e.target as HTMLInputElement).value;
          draft.base_url = v || null;
          scheduleSave(false);
        }}
      />
    </SettingField>

    {#if showApiMode}
      <SettingField key="models[].api_mode" itemId={model.id}>
        <select
          id="api_mode"
          value={draft.api_mode ?? "responses"}
          onchange={(e) => {
            draft.api_mode = (e.target as HTMLSelectElement).value as ApiMode;
            scheduleSave(true);
          }}
        >
          {#each API_MODE_OPTIONS as opt}
            <option value={opt.value}>{opt.label}</option>
          {/each}
        </select>
        <p class="helper">
          Which OpenAI-compatible endpoint to call. <strong>Responses</strong> is required for
          built-in tools like web search and reasoning summaries on GPT-5/o-series. Switch to
          <strong>Chat Completions</strong> if the Base URL points to a provider that only
          implements <code>/v1/chat/completions</code> (most OpenAI-compatible gateways: OpenRouter,
          vLLM, Ollama, Together, etc.).
        </p>
      </SettingField>
    {/if}

    {#if draft.api_mode !== "completions"}
      <SettingField key="models[].store" itemId={model.id}>
        <div class="toggle-field">
          <label>
            <input
              type="checkbox"
              checked={draft.store}
              onchange={(e) => {
                draft.store = (e.target as HTMLInputElement).checked;
                scheduleSave(true);
              }}
            />
            <span>Store request/response on provider</span>
          </label>
        </div>
      </SettingField>
    {/if}
  </section>

  {#if isText}
    <section class="card">
      <h3>Capabilities</h3>
      <SettingField key="models[].context_window_size" itemId={model.id}>
        <input
          id="ctx_window"
          type="number"
          placeholder="auto from model"
          min="1"
          value={draft.context_window_size ?? ""}
          oninput={(e) => {
            const v = (e.target as HTMLInputElement).value;
            const n = v ? Number(v) : null;
            draft.context_window_size =
              n !== null && Number.isFinite(n) && n > 0 ? Math.floor(n) : null;
            scheduleSave(false);
          }}
        />
      </SettingField>
      <CapabilitiesEditor
        capabilities={draft.capabilities ?? null}
        onChange={setCapabilities}
      />
    </section>

    <section class="card">
      <h3>Parameters</h3>
      <h4>Known</h4>
      <ParametersKnown
        parameters={draft.parameters}
        capabilities={resolvedCapabilities}
        onChange={setKnownParameter}
      />
      <h4 class="custom-heading">Custom</h4>
      <ParametersCustom bind:entries={customEntries} errors={customErrors} />
    </section>
  {/if}

  <DangerZone
    noun="model"
    name={draft.display_name || "this model"}
    warning={uses.length > 0 ? inUseWarning : undefined}
    onDuplicate={handleDuplicate}
    onDelete={handleDelete}
  />

  <footer class="meta">
    <span>ID: <code>{model.id}</code></span>
    <ActionIconButton icon={Copy} confirmIcon={Check} label="Copy ID" onclick={copyId} />
  </footer>
</div>

<style>
  .model-editor {
    padding: 48px var(--space-12) var(--space-16);
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: 720px;
  }

  .model-editor :global(:is(input, select, textarea, button, [tabindex])) {
    scroll-margin-top: calc(var(--sticky-head-height) + var(--space-4));
  }

  .sticky-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    margin-inline: calc(-1 * var(--space-12));
    padding: var(--space-8) var(--space-12) var(--space-6);
    background: var(--surface-base);
    border-bottom: 1px solid var(--border-faint);
  }

  .editor-header {
    display: flex;
    align-items: center;
    gap: var(--space-5);
  }

  .editor-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
    margin: var(--space-0);
    line-height: 1.2;
  }

  .used-by {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .used-by-label {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .link {
    background: none;
    border: none;
    padding: var(--space-0);
    font: inherit;
    color: var(--accent);
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  .card {
    background: var(--surface-base);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-lg);
    padding: var(--space-7) var(--space-8);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    margin: var(--space-0);
  }

  h4 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-disabled);
    margin: var(--space-2) var(--space-0) var(--space-0);
  }

  h4.custom-heading {
    margin-top: var(--space-6);
    border-top: 1px solid var(--border-faint);
    padding-top: var(--space-6);
  }

  input[type="text"],
  input[type="number"],
  select {
    width: 100%;
    padding: var(--space-3) var(--space-4);
    background-color: var(--surface-sunken);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
  }

  select {
    padding-right: var(--space-8);
  }

  .field-error {
    font-size: var(--font-size-sm);
    color: var(--danger);
    margin: var(--space-0);
  }

  input.error {
    border-color: var(--danger);
  }

  .segmented {
    display: inline-flex;
    background: var(--surface-sunken);
    border: 1px solid var(--border-hard);
    border-radius: 5px;
    padding: var(--space-1);
    width: fit-content;
  }

  .segmented button {
    padding: var(--space-2) var(--space-6);
    background: transparent;
    border: none;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--font-size-md);
    cursor: pointer;
    border-radius: var(--radius-sm);
  }

  .segmented button.active {
    background: var(--accent-bg-soft);
    color: var(--accent-text);
  }

  .toggle-field label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    color: var(--text-primary);
    cursor: pointer;
  }

  .helper {
    font-size: var(--font-size-sm);
    color: var(--text-disabled);
    margin-top: var(--space-1);
    line-height: 1.5;
  }

  .helper code {
    font-family: var(--font-mono);
    font-size: 0.92em;
    padding: 0 4px;
    background: var(--surface-overlay-faint);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
  }

  .helper strong {
    color: var(--text-secondary);
    font-weight: var(--font-weight-semibold);
  }

  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    font-size: var(--font-size-sm);
    color: var(--text-disabled);
    padding: var(--space-4) var(--space-0) var(--space-0);
  }

  .meta code {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }
</style>
