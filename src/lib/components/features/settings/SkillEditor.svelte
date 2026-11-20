<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { Copy, Check, ChevronRight } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { error as logError } from "@tauri-apps/plugin-log";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { writeTextFile } from "@tauri-apps/plugin-fs";
  import FormRow from "$lib/components/shared/ui/FormRow.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import DangerZone from "./DangerZone.svelte";
  import ParametersKnown from "./ParametersKnown.svelte";
  import ParametersCustom, {
    entriesFromExtra,
    entriesToExtra,
    type CustomParamEntry,
  } from "./ParametersCustom.svelte";
  import SkillBodyEditor from "./SkillBodyEditor.svelte";
  import SkillPreview from "./SkillPreview.svelte";
  import {
    deleteSkill,
    duplicateSkill,
    exportSkill,
    updateSkill,
  } from "$lib/services/skills";
  import {
    getCachedCapabilities,
    prefetchCapabilities,
  } from "$lib/stores/capabilities.svelte";
  import { useSaveTracker } from "$lib/stores/saveTracker.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import {
    KNOWN_MODEL_PARAMETER_KEYS,
    type KnownModelParameterKey,
    type ModelParameters,
    type ModelCapabilities,
    type OfferedIn,
    type SkillFrontmatter,
    type SkillFull,
  } from "$lib/types";

  let {
    skill,
    all,
    onSelectSkill,
    onDeleted,
  }: {
    skill: SkillFull;
    all: SkillFull[];
    onSelectSkill: (slug: string) => void;
    onDeleted: () => void;
  } = $props();

  const settingsStore = getSettingsStore();

  type Draft = {
    display_name: string;
    description: string;
    model: string | null;
    parameters: ModelParameters | null;
    offered_in: OfferedIn[] | null;
    body: string;
  };

  const OFFERED_IN_OPTIONS: { value: OfferedIn; label: string }[] = [
    { value: "context_menu", label: "Context menu" },
    { value: "pill", label: "Recording widget" },
  ];

  let draft = $state<Draft>(untrack(() => buildDraft(skill)));
  let customEntries = $state<CustomParamEntry[]>(
    untrack(() => entriesFromExtra(extraOf(skill.parameters))),
  );
  let customErrors = $state<Record<string, string>>({});
  let bodyError = $state<string | null>(null);
  let inheritedExpanded = $state(false);
  let suppressCustomSave = true;
  let initializing = true;

  const tracker = useSaveTracker({
    debounceMs: settingsStore.settings?.autosave_debounce_ms ?? 1000,
  });

  onMount(() => {
    tracker.attachKeyboard(window);
    tracker.attachBeforeUnload(window);
    Promise.resolve().then(() => {
      initializing = false;
    });
  });

  onDestroy(() => {
    tracker.destroy();
  });

  $effect(() => {
    const next = skill;
    untrack(() => {
      if (tracker.dirty || tracker.saving || tracker.hasPending) return;
      draft = buildDraft(next);
      customEntries = entriesFromExtra(extraOf(next.parameters));
      customErrors = {};
      suppressCustomSave = true;
    });
  });

  function buildDraft(s: SkillFull): Draft {
    return {
      display_name: s.display_name ?? "",
      description: s.description ?? "",
      model: s.model ?? null,
      parameters: (s.parameters ?? null) as ModelParameters | null,
      offered_in: s.offered_in ?? null,
      body: s.body,
    };
  }

  function extraOf(params: Record<string, unknown> | null): Record<string, unknown> {
    if (!params) return {};
    const known = new Set<string>(KNOWN_MODEL_PARAMETER_KEYS);
    const out: Record<string, unknown> = {};
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
    if (draft.body.trim().length === 0) {
      bodyError = "Body cannot be empty — skill will produce no output.";
      return false;
    }
    bodyError = null;
    return true;
  }

  async function persist() {
    if (!validate()) {
      throw new Error(bodyError ?? "Validation failed");
    }
    const parameters = buildParametersToSend();
    if (Object.keys(customErrors).length > 0) {
      throw new Error("Custom parameter errors");
    }

    const fm: SkillFrontmatter = {
      name: skill.name,
      display_name: draft.display_name.trim() || null,
      description: draft.description.trim() || null,
      model: draft.model ?? null,
      parameters: parameters as Record<string, unknown> | null,
      offered_in: draft.offered_in,
    };
    await updateSkill(skill.name, fm, draft.body);
  }

  function scheduleSave(immediate: boolean) {
    if (initializing) return;
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

  function isOfferedIn(value: OfferedIn): boolean {
    return draft.offered_in === null || draft.offered_in.includes(value);
  }

  function toggleOfferedIn(value: OfferedIn, checked: boolean) {
    const next = OFFERED_IN_OPTIONS.map((o) => o.value).filter((v) =>
      v === value ? checked : isOfferedIn(v),
    );
    draft.offered_in = next.length === OFFERED_IN_OPTIONS.length ? null : next;
    scheduleSave(true);
  }

  $effect(() => {
    customEntries;
    if (suppressCustomSave) {
      suppressCustomSave = false;
      return;
    }
    if (initializing) return;
    scheduleSave(false);
  });

  $effect(() => {
    const _track = draft.body;
    if (initializing) return;
    if (draft.body === skill.body) {
      bodyError = null;
      return;
    }
    if (draft.body.trim().length === 0) {
      bodyError = "Body cannot be empty — skill will produce no output.";
      tracker.cancel();
      return;
    }
    bodyError = null;
    scheduleSave(false);
  });

  async function handleDuplicate() {
    const newSlug = `${skill.name}-copy`;
    let candidate = newSlug;
    const taken = new Set(all.map((s) => s.name));
    let i = 2;
    while (taken.has(candidate)) {
      candidate = `${skill.name}-copy-${i}`;
      i += 1;
      if (i > 50) return;
    }
    await tracker.runAction(async () => {
      const created = await duplicateSkill(skill.name, candidate);
      onSelectSkill(created.name);
    });
  }

  async function handleDelete() {
    tracker.cancel();
    const ok = await tracker.runAction(async () => {
      await deleteSkill(skill.name);
    });
    if (ok) onDeleted();
  }

  async function handleExport() {
    try {
      const exported = await exportSkill(skill.name);
      const target = await saveDialog({
        defaultPath: exported.filename,
        filters: [{ name: "Skill", extensions: ["md"] }],
      });
      if (!target) return;
      await writeTextFile(target, exported.content);
    } catch {}
  }

  async function copySlug() {
    try {
      await navigator.clipboard.writeText(skill.name);
    } catch (e) {
      logError(`Copy skill slug failed: ${e}`);
    }
  }

  const textModels = $derived(settingsStore.models.filter((m) => m.type === "text"));

  const inheritedModelId = $derived(
    settingsStore.settings?.surfaces.quick_actions.generation.model_id ??
      settingsStore.settings?.surfaces.chat.generation.model_id ??
      null,
  );
  const inheritedModelLabel = $derived.by<string>(() => {
    if (!inheritedModelId) return "no fallback configured";
    const found = settingsStore.models.find((m) => m.id === inheritedModelId);
    return found ? found.display_name : inheritedModelId;
  });

  const pinnedModelConfig = $derived(
    draft.model ? settingsStore.models.find((m) => m.id === draft.model) ?? null : null,
  );

  const unlistedModelId = $derived(
    draft.model && !textModels.some((m) => m.id === draft.model) ? draft.model : null,
  );

  const unlistedModelReason = $derived<string | null>(
    !unlistedModelId
      ? null
      : pinnedModelConfig
        ? "is not a text model"
        : "no longer exists",
  );

  const effectiveModelConfig = $derived.by(() => {
    const id = draft.model ?? inheritedModelId;
    if (!id) return null;
    return settingsStore.models.find((m) => m.id === id) ?? null;
  });

  $effect(() => {
    prefetchCapabilities(effectiveModelConfig?.id ?? null);
  });

  const resolvedCapabilities = $derived<ModelCapabilities | null>(
    effectiveModelConfig?.capabilities ??
      getCachedCapabilities(effectiveModelConfig?.id ?? null) ??
      null,
  );

  const inheritedSurfaceParams = $derived(
    settingsStore.settings?.surfaces.quick_actions.generation.parameters ?? null,
  );
</script>

{#snippet deleteWarning()}
  Past conversations using it will keep their history, but
  <code>/{skill.name}</code> will stop working.
{/snippet}

<div class="skill-editor">
  <header class="editor-header">
    <h1>{draft.display_name || skill.name}</h1>
    <code class="slug-chip">/{skill.name}</code>
  </header>

  <section class="card">
    <h3>Identity</h3>

    <FormRow label="Display name">
      <input
        type="text"
        value={draft.display_name}
        oninput={(e) => {
          draft.display_name = (e.target as HTMLInputElement).value;
          scheduleSave(false);
        }}
      />
    </FormRow>

    <div class="field">
      <span class="field-label">Slug</span>
      <div class="slug-row">
        <code class="slug-readonly">/{skill.name}</code>
        <ActionIconButton icon={Copy} confirmIcon={Check} label="Copy slug" onclick={copySlug} />
      </div>
      <p class="helper">
        The slug cannot be changed — duplicate the skill to create a variant with a different
        slug.
      </p>
    </div>

    <FormRow label="Description">
      <input
        type="text"
        value={draft.description}
        oninput={(e) => {
          draft.description = (e.target as HTMLInputElement).value;
          scheduleSave(false);
        }}
      />
    </FormRow>
  </section>

  <section class="card">
    <h3>Availability</h3>
    <div class="field toggle-field">
      <span class="field-label">Offered in</span>
      {#each OFFERED_IN_OPTIONS as option (option.value)}
        <label>
          <input
            type="checkbox"
            checked={isOfferedIn(option.value)}
            onchange={(e) =>
              toggleOfferedIn(option.value, (e.target as HTMLInputElement).checked)}
          />
          <span>{option.label}</span>
        </label>
      {/each}
      <p class="helper">
        Where the skill appears as a pick. Unchecking both keeps it available only through
        <code>/{skill.name}</code> in the chat.
      </p>
    </div>
  </section>

  <section class="card">
    <h3>Model</h3>
    <FormRow label="Model">
      <select
        value={draft.model ?? ""}
        onchange={(e) => {
          const v = (e.target as HTMLSelectElement).value;
          draft.model = v || null;
          scheduleSave(true);
        }}
      >
        <option value="">
          Inherit (Quick Actions: {inheritedModelLabel})
        </option>
        {#if unlistedModelId}
          <option value={unlistedModelId}>
            {unlistedModelId} — {unlistedModelReason}
          </option>
        {/if}
        {#each textModels as m (m.id)}
          <option value={m.id}>
            {m.display_name}{m.id === inheritedModelId ? " (pinned — same as Quick Actions)" : ""}
          </option>
        {/each}
      </select>
    </FormRow>
    {#if unlistedModelId}
      <p class="helper warn-text">
        This skill is pinned to <code>{unlistedModelId}</code>, which {unlistedModelReason} —
        <code>/{skill.name}</code> will fail to run. Pick another model or switch back to Inherit.
      </p>
    {/if}
    <p class="helper">
      Skills without a model use the Quick Actions surface model. Pick one explicitly to pin a
      skill (e.g. a longer-context model for <code>text-polish</code>).
    </p>
  </section>

  <section class="card">
    <h3>Parameters</h3>
    <p class="helper">
      Skipped parameters (no override) inherit from Quick Actions. Toggle <em>Show inherited</em>
      below to peek at the effective values.
    </p>
    <h4>Known</h4>
    <ParametersKnown
      parameters={draft.parameters}
      capabilities={resolvedCapabilities}
      inherited={inheritedSurfaceParams}
      onChange={setKnownParameter}
    />
    <h4 class="custom-heading">Custom</h4>
    <ParametersCustom bind:entries={customEntries} errors={customErrors} />

    <button
      type="button"
      class="inherited-toggle"
      class:open={inheritedExpanded}
      onclick={() => (inheritedExpanded = !inheritedExpanded)}
    >
      <ChevronRight size={ICON_SIZE.sm} />
      <span>{inheritedExpanded ? "Hide" : "Show"} inherited (Quick Actions)</span>
    </button>
    {#if inheritedExpanded && inheritedSurfaceParams}
      <table class="inherited-table">
        <tbody>
          {#each Object.entries(inheritedSurfaceParams) as [k, v] (k)}
            {#if v !== null && v !== undefined}
              <tr>
                <th>{k}</th>
                <td>{typeof v === "object" ? JSON.stringify(v) : String(v)}</td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <SkillBodyEditor
    bind:value={draft.body}
    filePath={skill.file_path}
    error={bodyError}
  />

  <SkillPreview body={draft.body} skillName={skill.name} />

  <DangerZone
    compact
    noun="skill"
    name={draft.display_name || skill.name}
    warning={deleteWarning}
    onDuplicate={handleDuplicate}
    onExport={handleExport}
    onDelete={handleDelete}
  />
</div>

<style>
  .skill-editor {
    padding: 18px var(--space-12) var(--space-16);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    max-width: 760px;
  }

  .editor-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .editor-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
    margin: 0;
    line-height: 1.2;
  }

  .slug-chip {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    padding: 2px var(--space-3);
    background: var(--surface-overlay-faint);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-xl);
    color: var(--accent-text);
  }

  .card {
    background: var(--surface-base);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-lg);
    padding: var(--space-6) var(--space-7);
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    margin: 0;
  }

  h4 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-disabled);
    margin: var(--space-2) 0 0;
  }

  h4.custom-heading {
    margin-top: var(--space-5);
    border-top: 1px solid var(--border-faint);
    padding-top: var(--space-5);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .field-label {
    font-size: var(--font-size-md);
    color: var(--text-secondary);
  }

  input[type="text"],
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

  .toggle-field label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    color: var(--text-primary);
    cursor: pointer;
    align-self: flex-start;
  }

  .slug-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .slug-readonly {
    flex: 1;
    padding: var(--space-3) var(--space-4);
    background: var(--surface-sunken);
    border: 1px dashed var(--border-faint);
    border-radius: var(--radius-md);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .helper {
    font-size: var(--font-size-sm);
    color: var(--text-disabled);
    margin: 0;
    line-height: 1.5;
  }

  .helper.warn-text {
    color: var(--warning);
  }

  .helper code {
    font-family: var(--font-mono);
    font-size: 0.92em;
    padding: 0 4px;
    background: var(--surface-overlay-faint);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
  }

  .inherited-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font: inherit;
    font-size: var(--font-size-sm);
    cursor: pointer;
    align-self: flex-start;
  }

  .inherited-toggle :global(svg) {
    transition: transform var(--motion-fast) var(--ease-default);
  }

  .inherited-toggle.open :global(svg) {
    transform: rotate(90deg);
  }

  .inherited-toggle:hover {
    background: var(--surface-overlay-faint);
    color: var(--text-primary);
  }

  .inherited-table {
    border-collapse: collapse;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    background: var(--surface-sunken);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .inherited-table th,
  .inherited-table td {
    padding: var(--space-2) var(--space-4);
    text-align: left;
    border-bottom: 1px solid var(--border-faint);
  }

  .inherited-table th {
    font-weight: var(--font-weight-medium);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }

  .inherited-table tr:last-child th,
  .inherited-table tr:last-child td {
    border-bottom: none;
  }
</style>
