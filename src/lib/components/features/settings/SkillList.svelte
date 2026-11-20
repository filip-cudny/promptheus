<script lang="ts">
  import { GripVertical, Plus, Upload, Search } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { reorderSkills } from "$lib/services/skills";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount, onDestroy } from "svelte";
  import { readTextFile } from "@tauri-apps/plugin-fs";
  import { error as logError } from "@tauri-apps/plugin-log";
  import type { SkillFull } from "$lib/types";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";

  let {
    items,
    selectedSlug,
    onSelect,
    onNew,
    onImportPick,
    onImportDropped,
  }: {
    items: SkillFull[];
    selectedSlug: string | null;
    onSelect: (slug: string) => void;
    onNew: () => void;
    onImportPick: () => void;
    onImportDropped: (content: string) => void;
  } = $props();

  let query = $state("");
  let dragSlug = $state<string | null>(null);
  let dropSlug = $state<string | null>(null);
  let dropPosition = $state<"above" | "below" | null>(null);
  let fileDropActive = $state(false);
  let listEl = $state<HTMLDivElement | undefined>(undefined);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((s) =>
      [s.display_name, s.name, s.description ?? ""].some((field) =>
        field.toLowerCase().includes(q),
      ),
    );
  });

  function startDrag(e: DragEvent, slug: string) {
    if (!e.dataTransfer) return;
    dragSlug = slug;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", slug);
  }

  function rowDragOver(e: DragEvent, slug: string) {
    if (!dragSlug || dragSlug === slug) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const above = e.clientY - rect.top < rect.height / 2;
    dropSlug = slug;
    dropPosition = above ? "above" : "below";
  }

  function rowDragLeave(slug: string) {
    if (dropSlug === slug) {
      dropSlug = null;
      dropPosition = null;
    }
  }

  async function rowDrop(e: DragEvent, slug: string) {
    e.preventDefault();
    if (!dragSlug || dragSlug === slug) {
      resetDrag();
      return;
    }
    const order = items.map((s) => s.name);
    const fromIdx = order.indexOf(dragSlug);
    let toIdx = order.indexOf(slug);
    if (fromIdx < 0 || toIdx < 0) {
      resetDrag();
      return;
    }
    if (dropPosition === "below") toIdx += 1;
    const moving = order.splice(fromIdx, 1)[0];
    if (fromIdx < toIdx) toIdx -= 1;
    order.splice(toIdx, 0, moving);
    resetDrag();
    try {
      await reorderSkills(order);
    } catch (e) {
      logError(`Reorder skills failed: ${e}`);
    }
  }

  function resetDrag() {
    dragSlug = null;
    dropSlug = null;
    dropPosition = null;
  }

  let dragUnlisten: (() => void) | null = null;

  onMount(async () => {
    const webview = getCurrentWebview();
    dragUnlisten = await webview.onDragDropEvent(async (event) => {
      if (event.payload.type === "over") {
        if (insideList(event.payload.position)) {
          fileDropActive = true;
        } else if (fileDropActive) {
          fileDropActive = false;
        }
      } else if (event.payload.type === "leave") {
        fileDropActive = false;
      } else if (event.payload.type === "drop") {
        if (!fileDropActive) return;
        fileDropActive = false;
        const paths = event.payload.paths ?? [];
        for (const p of paths) {
          if (!p.toLowerCase().endsWith(".md")) continue;
          try {
            const text = await readTextFile(p);
            onImportDropped(text);
          } catch {}
        }
      }
    });
  });

  function insideList(pos: { x: number; y: number }): boolean {
    if (!listEl) return false;
    const r = listEl.getBoundingClientRect();
    return pos.x >= r.left && pos.x <= r.right && pos.y >= r.top && pos.y <= r.bottom;
  }

  onDestroy(() => {
    dragUnlisten?.();
  });
</script>

<aside class="skill-list">
  <header>
    <div class="search">
      <Search size={ICON_SIZE.sm} />
      <input
        type="text"
        placeholder="Search skills"
        bind:value={query}
        spellcheck="false"
        autocomplete="off"
      />
    </div>
    <div class="actions">
      <ActionIconButton icon={Upload} label="Import .md file" onclick={onImportPick} />
      <ActionIconButton icon={Plus} label="New skill" onclick={onNew} />
    </div>
  </header>

  <div class="scroll" bind:this={listEl} class:drop-active={fileDropActive}>
    {#if filtered.length === 0}
      {#if items.length === 0}
        <div class="empty">
          <p class="title">No skills yet</p>
          <p class="muted">Create one or drop a <code>.md</code> file here.</p>
          <Button variant="primary" icon={Plus} onclick={onNew}>Add skill</Button>
          <Button icon={Upload} onclick={onImportPick}>Import .md</Button>
        </div>
      {:else}
        <div class="empty">
          <p class="muted">No matches for "{query}".</p>
        </div>
      {/if}
    {:else}
      {#each filtered as skill (skill.name)}
        <div
          class="row-wrap"
          class:drop-above={dropSlug === skill.name && dropPosition === "above"}
          class:drop-below={dropSlug === skill.name && dropPosition === "below"}
          ondragover={(e) => rowDragOver(e, skill.name)}
          ondragleave={() => rowDragLeave(skill.name)}
          ondrop={(e) => rowDrop(e, skill.name)}
          role="presentation"
        >
          <button
            class="row"
            class:active={skill.name === selectedSlug}
            class:dragging={dragSlug === skill.name}
            onclick={() => onSelect(skill.name)}
            draggable={query.trim() === ""}
            ondragstart={(e) => startDrag(e, skill.name)}
            ondragend={resetDrag}
          >
            <span class="handle" aria-hidden="true">
              <GripVertical size={ICON_SIZE.sm} />
            </span>
            <span class="row-main">
              <span class="row-name">{skill.display_name}</span>
              <span class="row-meta">
                <code class="slug">/{skill.name}</code>
                {#if skill.description}
                  <span class="separator">·</span>
                  <span class="desc">{skill.description}</span>
                {/if}
              </span>
            </span>
          </button>
        </div>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .skill-list {
    width: 280px;
    flex-shrink: 0;
    border-right: 1px solid var(--border-faint);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5) var(--space-6);
    border-bottom: 1px solid var(--border-faint);
    flex-shrink: 0;
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    background: var(--surface-elevated);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-md);
    padding: 0 var(--space-3);
    color: var(--text-faint);
  }

  .search input {
    flex: 1;
    padding: var(--space-2) 0;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-sm);
    outline: none;
  }

  .actions {
    display: flex;
    gap: var(--space-2);
  }

  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-3) var(--space-0);
    position: relative;
    transition: background var(--motion-fast) var(--ease-default);
  }

  .scroll.drop-active {
    background: var(--accent-bg-soft);
    box-shadow: inset 0 0 0 2px var(--accent-border);
  }

  .empty {
    padding: var(--space-12) var(--space-6);
    text-align: center;
    color: var(--text-muted);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
  }

  .empty .title {
    font-size: var(--font-size-md);
    color: var(--text-secondary);
    margin: 0;
  }

  .empty .muted {
    color: var(--text-disabled);
    margin: 0;
  }

  .empty code {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    padding: 0 4px;
    background: var(--surface-overlay-faint);
    border-radius: var(--radius-sm);
  }

  .row-wrap {
    position: relative;
  }

  .row-wrap.drop-above::before,
  .row-wrap.drop-below::after {
    content: "";
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    height: 2px;
    background: var(--accent);
    border-radius: 1px;
    pointer-events: none;
  }

  .row-wrap.drop-above::before {
    top: -1px;
  }

  .row-wrap.drop-below::after {
    bottom: -1px;
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-3) var(--space-6);
    background: transparent;
    border: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    border-left: 2px solid transparent;
  }

  .row:hover:not(.active) {
    background: var(--surface-overlay-faint);
  }

  .row.active {
    background: var(--accent-bg-soft);
    border-left-color: var(--accent);
  }

  .row.dragging {
    opacity: 0.4;
  }

  .handle {
    display: inline-flex;
    align-self: center;
    color: var(--text-faint);
    cursor: grab;
  }

  .row.dragging .handle {
    cursor: grabbing;
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .row-name {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-meta {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .row-meta .slug {
    font-family: var(--font-mono);
    color: var(--accent-text);
    font-size: 0.92em;
  }

  .row-meta .separator {
    color: var(--text-faint);
  }

  .row-meta .desc {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-disabled);
  }
</style>
