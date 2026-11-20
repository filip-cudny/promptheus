<script lang="ts">
  import { error as logError } from "@tauri-apps/plugin-log";
  import SelectDropdown from "$lib/components/shared/ui/SelectDropdown.svelte";
  import SaveStatusIndicator from "$lib/components/shared/widgets/SaveStatusIndicator.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { getSkillsStore } from "$lib/stores/skills.svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import { getCachedCapabilities, prefetchCapabilities } from "$lib/stores/capabilities.svelte";
  import {
    setSpeechToTextModel,
    updateSurfaceModel,
    updateSurfaceReasoningEffort,
  } from "$lib/services/settings";
  import { SURFACE_LABELS } from "$lib/constants/surfaces";
  import { effortOptions } from "$lib/utils/effortOptions";
  import { agentModelId } from "$lib/utils/agentModel";
  import { quickActionsModelId, sameAsChatLabel } from "$lib/utils/modelUses";
  import type { ModelConfig, SurfaceKind } from "$lib/types";

  let headHeight = $state(0);

  type TextSurface = Exclude<SurfaceKind, "speech_to_text">;

  const TEXT_ROWS: ReadonlyArray<{
    surface: TextSurface;
    note: string;
    section: string;
    key: string;
    hasEffort: boolean;
  }> = [
    {
      surface: "chat",
      note: "Conversation window",
      section: "Chat",
      key: "surfaces.chat.generation.model_id",
      hasEffort: true,
    },
    {
      surface: "quick_actions",
      note: "Prompts from the context menu",
      section: "Context menu",
      key: "surfaces.quick_actions.generation.model_id",
      hasEffort: true,
    },
    {
      surface: "title_generation",
      note: "Names new conversations",
      section: "Chat",
      key: "surfaces.title_generation.generation.model_id",
      hasEffort: false,
    },
    {
      surface: "settings_agent",
      note: "\"Ask agent\" panel",
      section: "Chat",
      key: "surfaces.settings_agent.generation.model_id",
      hasEffort: true,
    },
  ];

  const store = getSettingsStore();
  const skills = getSkillsStore();
  const navigation = getSettingsNavigation();
  const highlighted = $derived(navigation.highlightedAssignment);

  const surfaces = $derived(store.settings?.surfaces ?? null);

  const textModels = $derived(
    store.models
      .filter((m) => m.type === "text")
      .map((m) => ({ value: m.id, label: m.display_name || m.model })),
  );
  const sttModels = $derived(
    store.models
      .filter((m) => m.type === "stt")
      .map((m) => ({ value: m.id, label: m.display_name || m.model })),
  );
  const chatModelOptions = $derived([{ value: "", label: "Not set" }, ...textModels]);
  const inheritingModelOptions = $derived([
    { value: "", label: surfaces ? sameAsChatLabel(surfaces, store.models) : "Same as Chat" },
    ...textModels,
  ]);
  const sttModelOptions = $derived([{ value: "", label: "Not set" }, ...sttModels]);

  const storedModelIds = $derived<Record<TextSurface, string | null>>({
    chat: surfaces?.chat.generation.model_id ?? null,
    quick_actions: surfaces?.quick_actions.generation.model_id ?? null,
    title_generation: surfaces?.title_generation.generation.model_id ?? null,
    settings_agent: surfaces?.settings_agent.generation.model_id ?? null,
  });
  const effectiveModelIds = $derived<Record<TextSurface, string | null>>({
    chat: storedModelIds.chat,
    quick_actions: surfaces ? quickActionsModelId(surfaces) : null,
    title_generation: null,
    settings_agent: surfaces ? agentModelId(surfaces) : null,
  });

  $effect(() => prefetchCapabilities(effectiveModelIds.chat));
  $effect(() => prefetchCapabilities(effectiveModelIds.quick_actions));
  $effect(() => prefetchCapabilities(effectiveModelIds.settings_agent));

  function modelOptions(surface: TextSurface) {
    return surface === "chat" ? chatModelOptions : inheritingModelOptions;
  }

  function storedEffort(surface: TextSurface): string | null {
    const generation = surfaces?.[surface].generation;
    return generation && "parameters" in generation ? generation.parameters.reasoning_effort ?? null : null;
  }

  function effortOptionsFor(surface: TextSurface) {
    return effortOptions(
      getCachedCapabilities(effectiveModelIds[surface])?.reasoning,
      storedEffort(surface),
    );
  }

  function modelName(id: string, models: ModelConfig[]): string {
    const model = models.find((m) => m.id === id);
    return model ? model.display_name || model.model : id;
  }

  const pinnedSkills = $derived(skills.full.filter((s) => s.model));

  async function run(label: string, write: () => Promise<void>) {
    try {
      await write();
    } catch (e) {
      logError(`${label} failed: ${e}`);
    }
  }
</script>

<div class="model-assignments" style:--sticky-head-height="{headHeight}px">
  <header class="editor-header" bind:clientHeight={headHeight}>
    <h1>Assignments</h1>
    <SaveStatusIndicator />
  </header>
  <p class="lead">
    Every place in the app that calls a model, in one table. The same values are still editable in
    the Chat, Context menu and Speech sections; a change here shows there and the other way round.
  </p>

  {#if surfaces}
    <section class="card">
      <h3>Text features</h3>
      <table>
        <thead>
          <tr>
            <th class="feature-col">Feature</th>
            <th>Model</th>
            <th>Reasoning effort</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each TEXT_ROWS as row (row.surface)}
            <tr
              id="assignment-{row.surface}"
              data-setting-target="assignment-{row.surface}"
              class:highlighted={highlighted === row.surface}
            >
              <td class="feature">
                {SURFACE_LABELS[row.surface]}
                <small>{row.note}</small>
              </td>
              <td>
                <SelectDropdown
                  value={storedModelIds[row.surface] ?? ""}
                  options={modelOptions(row.surface)}
                  ariaLabel="{SURFACE_LABELS[row.surface]} model"
                  onchange={(id) =>
                    run(`update_surface_model ${row.surface}`, () =>
                      updateSurfaceModel(row.surface, id || null),
                    )}
                />
              </td>
              <td data-label="Reasoning effort">
                {#if row.hasEffort}
                  <SelectDropdown
                    value={storedEffort(row.surface) ?? ""}
                    options={effortOptionsFor(row.surface)}
                    ariaLabel="{SURFACE_LABELS[row.surface]} reasoning effort"
                    onchange={(v) =>
                      run(`update reasoning_effort ${row.surface}`, () =>
                        updateSurfaceReasoningEffort(row.surface, v || null),
                      )}
                  />
                {/if}
              </td>
              <td>
                <button class="link" onclick={() => void navigation.navigateTo(row.key)}>
                  Open in {row.section} ›
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="hint">"Same as Chat" follows the Chat model: switch Chat and these switch with it.</p>
    </section>

    <section class="card">
      <h3>Speech</h3>
      <table>
        <tbody>
          <tr
            id="assignment-speech_to_text"
            data-setting-target="assignment-speech_to_text"
            class:highlighted={highlighted === "speech_to_text"}
          >
            <td class="feature feature-col">
              {SURFACE_LABELS.speech_to_text}
              <small>Recording pill</small>
            </td>
            <td>
              <SelectDropdown
                value={surfaces.speech_to_text.model_id ?? ""}
                options={sttModelOptions}
                ariaLabel="Speech model"
                onchange={(id) =>
                  run("set_speech_to_text_model", () => setSpeechToTextModel(id || null))}
              />
            </td>
            <td></td>
            <td>
              <button
                class="link"
                onclick={() => void navigation.navigateTo("surfaces.speech_to_text.model_id")}
              >
                Open in Speech ›
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </section>
  {/if}

  {#if pinnedSkills.length > 0}
    <section class="card">
      <h3>Pinned in skills</h3>
      <p class="hint">Skills with their own model ignore the table above.</p>
      <table>
        <tbody>
          {#each pinnedSkills as skill (skill.name)}
            <tr>
              <td class="feature feature-col">{skill.name}</td>
              <td class="pinned-model">{modelName(skill.model ?? "", store.models)}</td>
              <td></td>
              <td>
                <button
                  class="link"
                  onclick={() => void navigation.navigateTo(`skills[${skill.name}]`)}
                >
                  Open skill ›
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .model-assignments {
    container-type: inline-size;
    padding: 48px var(--space-12) var(--space-16);
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: 760px;
  }

  .model-assignments :global(:is(input, select, textarea, button, [tabindex])) {
    scroll-margin-top: calc(var(--sticky-head-height) + var(--space-4));
  }

  .editor-header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: var(--space-5);
    margin-inline: calc(-1 * var(--space-12));
    padding: var(--space-8) var(--space-12) var(--space-6);
    background: var(--surface-base);
    border-bottom: 1px solid var(--border-faint);
  }

  h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
    margin: var(--space-0);
    line-height: 1.2;
  }

  .lead {
    font-size: var(--font-size-md);
    color: var(--text-muted);
    margin: var(--space-0);
    line-height: 1.5;
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

  .hint {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    margin: var(--space-0);
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    text-align: left;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--text-muted);
    padding: var(--space-0) var(--space-4) var(--space-3) var(--space-0);
  }

  td {
    padding: var(--space-3) var(--space-4) var(--space-3) var(--space-0);
    border-top: 1px solid var(--border-faint);
    vertical-align: middle;
  }

  tr.highlighted {
    outline: 2px solid var(--accent);
    background: var(--accent-bg-soft);
  }

  tr[data-setting-target] {
    outline-offset: -2px;
  }

  .feature-col {
    width: 30%;
  }

  .feature {
    font-size: var(--font-size-base);
    color: var(--text-primary);
  }

  .feature small {
    display: block;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    margin-top: 1px;
  }

  .pinned-model {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    color: var(--text-secondary);
  }

  .link {
    background: none;
    border: none;
    padding: var(--space-0);
    font: inherit;
    font-size: var(--font-size-sm);
    color: var(--accent-text);
    cursor: pointer;
    white-space: nowrap;
  }

  .link:hover {
    text-decoration: underline;
  }

  td :global(.trigger .label) {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @container (max-width: 600px) {
    table,
    tbody {
      display: block;
    }

    thead,
    td:empty {
      display: none;
    }

    tr {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--space-3) var(--space-6);
      padding: var(--space-4) var(--space-0);
      border-top: 1px solid var(--border-faint);
    }

    td {
      min-width: 0;
      padding: var(--space-0);
      border-top: none;
      overflow-wrap: anywhere;
    }

    .feature {
      flex-basis: 100%;
    }

    td[data-label] {
      display: flex;
      align-items: center;
      gap: var(--space-3);
    }

    td[data-label]::before {
      content: attr(data-label);
      font-size: var(--font-size-sm);
      color: var(--text-muted);
    }
  }
</style>
