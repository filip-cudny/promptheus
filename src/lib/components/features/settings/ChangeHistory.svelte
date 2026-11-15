<script lang="ts">
  import { ChevronDown, ChevronRight, Undo2 } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import { lineDiff } from "$lib/utils/lineDiff";
  import { effectPathLabel, entryTitle, formatEntryTime, formatValue } from "$lib/utils/changeLog";
  import { getSettingsCatalogStore } from "$lib/stores/settingsCatalog.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import {
    listChangeLog,
    onChangeLogChanged,
    undoChange,
    type ChangeLogItem,
    type UndoConflict,
  } from "$lib/services/changeLog";
  import { error as logError } from "@tauri-apps/plugin-log";
  import UndoConflicts from "./UndoConflicts.svelte";

  let items = $state<ChangeLogItem[]>([]);
  let expanded = $state<Record<string, boolean>>({});
  let conflicts = $state<Record<string, UndoConflict[]>>({});
  let messages = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);

  const catalogStore = getSettingsCatalogStore();
  const settingsStore = getSettingsStore();
  const entries = $derived(items.map((i) => i.entry));

  async function refresh() {
    try {
      items = await listChangeLog();
    } catch (e) {
      logError(`list_change_log failed: ${e}`);
    }
  }

  async function undo(id: string, force: boolean) {
    busy = id;
    delete messages[id];
    try {
      const outcome = await undoChange(id, force);
      if (outcome.status === "conflicts") {
        conflicts[id] = outcome.conflicts;
      } else {
        delete conflicts[id];
        if (outcome.status === "skipped") messages[id] = `Secret backup is on ${outcome.held_by}`;
        else await refresh();
      }
    } catch (e) {
      logError(`undo_change failed: ${e}`);
      messages[id] = `Undo failed: ${e}`;
    } finally {
      busy = null;
    }
  }

  function toggle(id: string) {
    expanded[id] = !expanded[id];
  }

  $effect(() => {
    refresh();
    const unlisten = onChangeLogChanged(() => refresh());
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<svelte:window onfocus={refresh} />

<div class="group">
  <h3>Change history</h3>
  {#if items.length === 0}
    <p class="muted">No changes made by the settings agent yet.</p>
  {/if}
  {#each items as { entry, state } (entry.id)}
    <div class="entry">
      <div class="head">
        <button type="button" class="toggle" aria-expanded={!!expanded[entry.id]} onclick={() => toggle(entry.id)}>
          {#if expanded[entry.id]}<ChevronDown size={ICON_SIZE.sm} />{:else}<ChevronRight size={ICON_SIZE.sm} />{/if}
          <span class="title">{entryTitle(entry, entries, catalogStore.entries, settingsStore.models)}</span>
        </button>
        <span class="badge" class:undone={state === "undone"}>{state}</span>
        <Button icon={Undo2} disabled={busy !== null} onclick={() => undo(entry.id, false)}>Undo</Button>
      </div>
      <div class="meta">{formatEntryTime(entry.created_at)} · {entry.host} · {entry.kind}</div>
      {#if entry.request}<div class="request">{entry.request}</div>{/if}
      {#if messages[entry.id]}<div class="message">{messages[entry.id]}</div>{/if}
      {#if conflicts[entry.id]}
        <UndoConflicts
          conflicts={conflicts[entry.id]}
          disabled={busy !== null}
          oncancel={() => delete conflicts[entry.id]}
          onforce={() => undo(entry.id, true)}
        />
      {/if}
      {#if expanded[entry.id]}
        {#if entry.secret}
          <div class="effect"><code class="path">{entry.secret.name}</code></div>
        {/if}
        {#each entry.operations as op, n (n)}
          <div class="op">
            <div class="op-label">{op.label}</div>
            {#each op.effects as effect, i (i)}
              <div class="effect">
                <code class="path">{effectPathLabel(effect.target, catalogStore.entries, settingsStore.models)}</code>
                {#if effect.target.type === "file"}
                  <pre class="diff">{#each lineDiff(String(effect.before ?? ""), String(effect.after ?? "")) as line}<span class="line {line.kind}">{line.kind === "added" ? "+ " : line.kind === "removed" ? "- " : "  "}{line.text}
</span>{/each}</pre>
                {:else if effect.target.type === "setting"}
                  <div class="values">
                    <span class="before">{formatValue(effect.before)}</span>
                    <span>→</span>
                    <span class="after">{formatValue(effect.after)}</span>
                  </div>
                {/if}
              </div>
            {/each}
            {#if op.test}
              <div class="test" class:ok={op.test.ok}>
                {op.test.ok ? `OK · ${op.test.millis ?? 0} ms` : (op.test.error ?? "Test failed")}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/each}
</div>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 640px;
  }

  h3 {
    margin: 0;
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
  }

  .muted,
  .meta {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) 0;
    border-top: 1px solid var(--border-faint);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    padding: 0;
    color: var(--text-primary);
    font-weight: var(--font-weight-semibold);
    text-align: left;
    cursor: pointer;
  }

  .badge {
    font-size: var(--font-size-xs);
    color: var(--success);
  }

  .badge.undone {
    color: var(--text-muted);
  }

  .request {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }

  .message {
    font-size: var(--font-size-sm);
    color: var(--warning);
  }

  .op,
  .effect {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--font-size-sm);
  }

  .op-label {
    font-weight: var(--font-weight-semibold);
  }

  .path {
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    word-break: break-all;
  }

  .values {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: baseline;
  }

  .before {
    color: var(--danger);
    text-decoration: line-through;
    word-break: break-word;
  }

  .after {
    color: var(--success);
    word-break: break-word;
  }

  .diff {
    margin: 0;
    max-height: 200px;
    overflow: auto;
    font-size: var(--font-size-xs);
    white-space: pre-wrap;
    background: var(--surface-overlay-faint);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-3);
  }

  .line {
    display: block;
  }

  .line.added {
    color: var(--success);
    background: var(--success-bg-soft);
  }

  .line.removed {
    color: var(--danger);
    background: var(--danger-bg-soft);
  }

  .test {
    font-size: var(--font-size-xs);
    color: var(--danger);
  }

  .test.ok {
    color: var(--success);
  }
</style>
