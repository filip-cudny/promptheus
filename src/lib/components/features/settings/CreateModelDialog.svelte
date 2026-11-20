<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Modal from "$lib/components/shared/ui/Modal.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import EnvVarPicker from "./EnvVarPicker.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { addModel, testModel } from "$lib/services/settings";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { generateId } from "$lib/utils/id";
  import { buildNewModel, modelListUrl, newModelErrors } from "$lib/utils/newModel";
  import type { ModelType, Provider } from "$lib/types";

  let {
    modelType,
    onClose,
    onCreated,
  }: {
    modelType: ModelType;
    onClose: () => void;
    onCreated: (id: string, testPassed: boolean) => void;
  } = $props();

  const store = getSettingsStore();
  const id = generateId();

  const PROVIDER_OPTIONS: { value: Provider | ""; label: string }[] = [
    { value: "", label: "None" },
    { value: "openai", label: "OpenAI" },
    { value: "anthropic", label: "Anthropic" },
    { value: "gemini", label: "Gemini" },
    { value: "elevenlabs", label: "ElevenLabs" },
  ];

  const initialType = untrack(() => modelType);
  let type = $state<ModelType>(initialType);
  let provider = $state<Provider | "">(initialType === "text" ? "openai" : "");
  let displayName = $state("");
  let model = $state("");
  let apiKey = $state("");
  let baseUrl = $state("");
  let touched = $state<Record<string, boolean>>({});
  let submitting = $state(false);
  let testing = $state(false);
  let testRun = $state<{ draftKey: string; ok: boolean; message: string } | null>(null);
  let displayNameEl = $state<HTMLInputElement | undefined>(undefined);

  const draft = $derived({
    type,
    display_name: displayName,
    provider: provider || null,
    model,
    api_key: apiKey,
    base_url: baseUrl,
  });
  const errors = $derived(newModelErrors(draft));
  const listUrl = $derived(modelListUrl(draft.provider));
  const draftKey = $derived(JSON.stringify(draft));
  const testResult = $derived(testRun?.draftKey === draftKey ? testRun : null);

  onMount(() => {
    displayNameEl?.focus();
  });

  function errorFor(field: string): string | undefined {
    return (
      store.fieldError(`models[${id}].${field}`) ??
      (touched[field] ? errors[field] : undefined)
    );
  }

  function touch(field: string) {
    touched[field] = true;
  }

  function selectType(next: ModelType) {
    type = next;
    provider = next === "text" ? "openai" : "";
  }

  async function runTest() {
    if (testing || Object.keys(errors).length > 0) return;
    const ranOn = draftKey;
    testing = true;
    try {
      const result = await testModel(buildNewModel(draft, id));
      testRun = {
        draftKey: ranOn,
        ok: result.ok,
        message: result.ok
          ? `OK · ${result.millis ?? 0} ms · the model answered.`
          : (result.error ?? "Test failed"),
      };
    } catch (e) {
      testRun = {
        draftKey: ranOn,
        ok: false,
        message: e instanceof Error ? e.message : String(e),
      };
    } finally {
      testing = false;
    }
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (submitting || Object.keys(errors).length > 0) return;
    submitting = true;
    try {
      await addModel(buildNewModel(draft, id));
      await store.refresh();
      onCreated(id, testResult?.ok ?? false);
      onClose();
    } catch {
      return;
    } finally {
      submitting = false;
    }
  }
</script>

<Modal title="Create model" {onClose}>
  <form class="form" onsubmit={handleSubmit}>
    <div class="field">
      <span class="label">Type</span>
      <div class="segmented">
        <button type="button" class:active={type === "text"} onclick={() => selectType("text")}>Text</button>
        <button type="button" class:active={type === "stt"} onclick={() => selectType("stt")}>STT</button>
      </div>
    </div>

    <div class="field">
      <label for="cmd-name">Display name</label>
      <input
        id="cmd-name"
        type="text"
        bind:value={displayName}
        bind:this={displayNameEl}
        onblur={() => touch("display_name")}
        class:error={errorFor("display_name")}
        autocomplete="off"
      />
      {#if errorFor("display_name")}<span class="hint err">{errorFor("display_name")}</span>{/if}
    </div>

    <div class="field">
      <label for="cmd-provider">Provider</label>
      <select id="cmd-provider" bind:value={provider}>
        {#each PROVIDER_OPTIONS as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>

    <div class="field">
      <div class="label-row">
        <label for="cmd-model">Model</label>
        {#if listUrl}
          <button type="button" class="link" onclick={() => void openUrl(listUrl)}>Model ids</button>
        {/if}
      </div>
      <input
        id="cmd-model"
        type="text"
        bind:value={model}
        onblur={() => touch("model")}
        class:error={errorFor("model")}
        spellcheck="false"
        autocomplete="off"
      />
      {#if errorFor("model")}<span class="hint err">{errorFor("model")}</span>{/if}
      <span class="hint">Type the model id from the provider's docs.</span>
    </div>

    <div class="field">
      <span class="label">API key</span>
      <EnvVarPicker
        value={apiKey}
        onchange={(v) => {
          apiKey = v;
          touch("api_key");
        }}
      />
      {#if errorFor("api_key")}<span class="hint err">{errorFor("api_key")}</span>{/if}
    </div>

    <div class="field">
      <label for="cmd-url">Base URL</label>
      <input
        id="cmd-url"
        type="text"
        placeholder="Optional"
        bind:value={baseUrl}
        onblur={() => touch("base_url")}
        class:error={errorFor("base_url")}
        spellcheck="false"
        autocomplete="off"
      />
      {#if errorFor("base_url")}<span class="hint err">{errorFor("base_url")}</span>{/if}
    </div>

    {#if testResult}
      <p class="test-note" class:err={!testResult.ok} role="status">{testResult.message}</p>
    {/if}

    <footer>
      <Button onclick={onClose}>Cancel</Button>
      <Button
        onclick={() => void runTest()}
        disabled={Object.keys(errors).length > 0 || testing || submitting}
      >
        {testing ? "Testing…" : "Test"}
      </Button>
      <Button
        variant="primary"
        type="submit"
        disabled={Object.keys(errors).length > 0 || submitting}
      >
        {submitting ? "Creating…" : "Create"}
      </Button>
    </footer>
  </form>
</Modal>

<style>
  .form {
    display: contents;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .field label,
  .label {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }

  .label-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .link {
    background: none;
    border: none;
    padding: var(--space-0);
    font: inherit;
    font-size: var(--font-size-sm);
    color: var(--accent);
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  .test-note {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    overflow-wrap: anywhere;
  }

  .test-note.err {
    color: var(--danger);
  }

  input[type="text"],
  select {
    padding: var(--space-3) var(--space-4);
    background: var(--surface-sunken);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
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

  .hint {
    font-size: var(--font-size-xs);
    color: var(--text-faint);
  }

  .hint.err {
    color: var(--danger);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }
</style>
