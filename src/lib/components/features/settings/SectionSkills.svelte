<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SkillList from "./SkillList.svelte";
  import SkillEditor from "./SkillEditor.svelte";
  import CreateSkillDialog from "./CreateSkillDialog.svelte";
  import ImportConflictDialog from "./ImportConflictDialog.svelte";
  import { getSkillsStore } from "$lib/stores/skills.svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import { createSkill, importSkillFile } from "$lib/services/skills";
  import { open } from "@tauri-apps/plugin-dialog";
  import { readTextFile } from "@tauri-apps/plugin-fs";
  import type { ImportConflictMode, SkillFrontmatter } from "$lib/types";

  const store = getSkillsStore();
  const navigation = getSettingsNavigation();

  let selectedSlug = $state<string | null>(null);
  let createOpen = $state(false);
  let importPending = $state<{ content: string; existingSlug: string } | null>(null);
  let importError = $state<string | null>(null);

  onMount(() => {
    void store.init();
  });

  onDestroy(() => {});

  let requestedSlug = $state<string | null>(null);

  $effect(() => {
    const item = navigation.selectedItem;
    if (item?.list === "skills") requestedSlug = item.id;
  });

  $effect(() => {
    const names = store.full.map((s) => s.name);
    const requested = requestedSlug;
    if (requested && names.includes(requested)) {
      selectedSlug = requested;
      requestedSlug = null;
    } else if (selectedSlug && !names.includes(selectedSlug)) {
      selectedSlug = names[0] ?? null;
    } else if (!selectedSlug && names.length > 0) {
      selectedSlug = names[0];
    }
  });

  const selected = $derived(
    selectedSlug ? store.full.find((s) => s.name === selectedSlug) ?? null : null,
  );

  async function handleCreate(payload: {
    slug: string;
    displayName: string;
    description: string | null;
    template: string | null;
  }) {
    const templateBody = payload.template
      ? store.full.find((s) => s.name === payload.template)?.body ?? ""
      : "";
    const fm: SkillFrontmatter = {
      name: payload.slug,
      display_name: payload.displayName || null,
      description: payload.description,
      model: null,
      parameters: null,
    };
    try {
      const created = await createSkill(payload.slug, fm, templateBody);
      selectedSlug = created.name;
    } catch (e) {
      throw e;
    }
  }

  async function tryImportContent(content: string, mode: ImportConflictMode = "reject") {
    importError = null;
    try {
      const created = await importSkillFile(content, mode);
      selectedSlug = created.name;
      importPending = null;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      const slugMatch = msg.match(/Skill\s+'?([a-z][a-z0-9-]*)'?\s+already exists/);
      if (slugMatch && mode === "reject") {
        importPending = { content, existingSlug: slugMatch[1] };
      } else {
        importError = msg;
      }
    }
  }

  async function pickAndImport() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Skill", extensions: ["md"] }],
    });
    if (!picked || typeof picked !== "string") return;
    const content = await readTextFile(picked);
    await tryImportContent(content);
  }

  async function importDropped(content: string) {
    await tryImportContent(content);
  }
</script>

<SettingsSection
  title="Skills"
  hint="Per-skill prompts invoked via /<name>. Drag to reorder; the order controls autocomplete."
>
  {#snippet body()}
    <div class="skills-body">
      <SkillList
        items={store.full}
        selectedSlug={selectedSlug}
        onSelect={(slug) => (selectedSlug = slug)}
        onNew={() => (createOpen = true)}
        onImportPick={pickAndImport}
        onImportDropped={importDropped}
      />
      <div class="editor-pane">
        {#if selected}
          {#key selected.name}
            <SkillEditor
              skill={selected}
              all={store.full}
              onSelectSkill={(slug) => (selectedSlug = slug)}
              onDeleted={() => {
                selectedSlug = null;
              }}
            />
          {/key}
        {:else}
          <div class="empty">
            <h2>No skill selected</h2>
            <p>Create or pick a skill from the list to edit it.</p>
          </div>
        {/if}
      </div>
    </div>

    {#if importError}
      <div class="toast error">{importError}</div>
    {/if}
  {/snippet}
</SettingsSection>

{#if createOpen}
  <CreateSkillDialog
    existingSlugs={new Set(store.full.map((s) => s.name))}
    templates={store.full.map((s) => ({ slug: s.name, label: s.display_name }))}
    onClose={() => (createOpen = false)}
    onSubmit={handleCreate}
  />
{/if}

{#if importPending}
  <ImportConflictDialog
    existingSlug={importPending.existingSlug}
    onClose={() => (importPending = null)}
    onResolve={(mode) => {
      const pending = importPending;
      if (pending) void tryImportContent(pending.content, mode);
    }}
  />
{/if}

<style>
  .skills-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .editor-pane {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    background: var(--surface-base);
  }

  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    padding: var(--space-16);
    text-align: center;
  }

  .empty h2 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-medium);
    color: var(--text-secondary);
    margin-bottom: var(--space-3);
  }

  .empty p {
    font-size: var(--font-size-md);
  }

  .toast.error {
    position: absolute;
    bottom: var(--space-4);
    right: var(--space-4);
    padding: var(--space-3) var(--space-4);
    background: var(--danger-bg-soft);
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-md);
    color: var(--danger-text);
    font-size: var(--font-size-sm);
    box-shadow: var(--shadow-md);
  }
</style>
