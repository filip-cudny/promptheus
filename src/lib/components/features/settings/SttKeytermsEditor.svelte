<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { X, FileCode2, Type, Plus } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { getSttKeyterms, saveSttKeyterms } from "$lib/services/keyterms";
  import { useSaveTracker } from "$lib/stores/saveTracker.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import HighlightedTextarea from "$lib/components/shared/ui/HighlightedTextarea.svelte";
  import FilePathReveal from "$lib/components/shared/ui/FilePathReveal.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import type { SttKeytermsDoc } from "$lib/types";

  let { description }: { description?: string } = $props();

  const settingsStore = getSettingsStore();

  let doc = $state<SttKeytermsDoc | null>(null);
  let rawContent = $state("");
  let savedRaw = $state("");
  let loadError = $state<string | null>(null);
  let mode = $state<"chips" | "raw">("chips");
  let inputValue = $state("");
  let inputEl = $state<HTMLInputElement | undefined>(undefined);

  const tracker = useSaveTracker({
    debounceMs: settingsStore.settings?.autosave_debounce_ms ?? 1000,
  });

  let unlistenChanged: (() => void) | null = null;

  type LineEntry =
    | { kind: "term"; text: string; index: number }
    | { kind: "comment"; text: string }
    | { kind: "blank" };

  const lines = $derived<LineEntry[]>(parseLines(rawContent));
  const terms = $derived(
    lines.filter((l): l is Extract<LineEntry, { kind: "term" }> => l.kind === "term"),
  );
  const hasComments = $derived(lines.some((l) => l.kind === "comment"));

  function parseLines(text: string): LineEntry[] {
    if (text === "") return [];
    let termIndex = 0;
    const out: LineEntry[] = [];
    for (const raw of text.split(/\r?\n/)) {
      const trimmed = raw.trim();
      if (trimmed === "") {
        out.push({ kind: "blank" });
      } else if (trimmed.startsWith("#")) {
        out.push({ kind: "comment", text: trimmed });
      } else {
        out.push({ kind: "term", text: trimmed, index: termIndex++ });
      }
    }
    return out;
  }

  onMount(() => {
    void load();
    tracker.attachKeyboard(window);
    tracker.attachBeforeUnload(window);
    void listen("stt-keyterms-changed", () => {
      if (tracker.dirty || tracker.saving || tracker.hasPending) return;
      void load();
    }).then((un) => {
      unlistenChanged = un;
    });
  });

  onDestroy(() => {
    tracker.destroy();
    if (unlistenChanged) unlistenChanged();
  });

  async function load() {
    try {
      const fetched = await getSttKeyterms();
      doc = fetched;
      rawContent = fetched.content;
      savedRaw = fetched.content;
      loadError = null;
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    }
  }

  $effect(() => {
    if (!doc) return;
    const _track = rawContent;
    if (rawContent === savedRaw) {
      tracker.cancel();
      return;
    }
    tracker.scheduleSave(persist);
  });

  async function persist() {
    if (!doc) return;
    if (rawContent === savedRaw) return;
    const snapshot = rawContent;
    const updated = await saveSttKeyterms(snapshot);
    doc = updated;
    savedRaw = snapshot;
  }

  function rebuildFromTerms(next: string[]) {
    if (hasComments) {
      const seen = new Set<number>();
      const lineOut: string[] = [];
      let nextIdx = 0;
      for (const line of lines) {
        if (line.kind === "term") {
          if (nextIdx < next.length) {
            lineOut.push(next[nextIdx]);
            seen.add(nextIdx);
            nextIdx++;
          }
        } else if (line.kind === "comment") {
          lineOut.push(line.text);
        } else {
          lineOut.push("");
        }
      }
      while (nextIdx < next.length) {
        lineOut.push(next[nextIdx]);
        nextIdx++;
      }
      rawContent = lineOut.join("\n");
    } else {
      rawContent = next.join("\n");
    }
  }

  function addTerm(value: string) {
    const trimmed = value.trim();
    if (trimmed === "") return;
    const tokens = trimmed
      .split(/[\n,]+/)
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
    if (tokens.length === 0) return;
    const existing = new Set(terms.map((t) => t.text.toLowerCase()));
    const next = terms.map((t) => t.text);
    for (const tok of tokens) {
      if (!existing.has(tok.toLowerCase())) {
        next.push(tok);
        existing.add(tok.toLowerCase());
      }
    }
    rebuildFromTerms(next);
    inputValue = "";
  }

  function removeTerm(index: number) {
    const next = terms.map((t) => t.text).filter((_, i) => i !== index);
    rebuildFromTerms(next);
  }

  function onInputKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      addTerm(inputValue);
      return;
    }
    if (e.key === "Backspace" && inputValue === "" && terms.length > 0) {
      e.preventDefault();
      removeTerm(terms.length - 1);
      return;
    }
  }

  function onInputPaste(e: ClipboardEvent) {
    const text = e.clipboardData?.getData("text") ?? "";
    if (text.includes("\n") || text.includes(",")) {
      e.preventDefault();
      const merged = inputValue + text;
      addTerm(merged);
    }
  }

  async function focusInput() {
    await tick();
    inputEl?.focus();
  }

  async function toggleMode() {
    if (mode === "chips") {
      mode = "raw";
    } else {
      mode = "chips";
      await focusInput();
    }
  }
</script>

<section class="keyterms-editor">
  <header class="head">
    <div class="title-row">
      <h3 class="title">Speech-to-text keyterms</h3>

      <span class="count" class:empty={terms.length === 0} aria-live="polite">
        {terms.length}
        <span class="count-label">{terms.length === 1 ? "term" : "terms"}</span>
      </span>

      <div class="head-actions">
        <ActionIconButton
          icon={mode === "raw" ? Type : FileCode2}
          label={mode === "raw" ? "Switch to chip view" : "Edit raw file"}
          active={mode === "raw"}
          onclick={toggleMode}
        />
        {#if doc?.path}
          <FilePathReveal path={doc.path} />
        {/if}
      </div>
    </div>

    {#if description}
      <p class="description">{description}</p>
    {/if}

    {#if hasComments && mode === "chips"}
      <p class="comments-note">
        File contains comment lines — edit raw to view or modify them.
      </p>
    {/if}
  </header>

  {#if loadError}
    <p class="load-error" role="alert">Failed to load: {loadError}</p>
  {:else if mode === "chips"}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="chip-pane"
      role="group"
      aria-label="Keyterms"
      onclick={() => focusInput()}
    >
      {#if terms.length === 0 && inputValue === ""}
        <div class="empty-state-chips" aria-hidden="true">
          <span class="ghost-chip">Anthropic</span>
          <span class="ghost-chip">Promptheus</span>
          <span class="ghost-chip">Tauri</span>
          <span class="empty-hint">— terms you'd like the model to recognize</span>
        </div>
      {/if}

      {#each terms as term (term.index + ":" + term.text)}
        <div class="chip">
          <span class="chip-index" aria-hidden="true">{term.index + 1
              < 10 ? "0" + (term.index + 1) : term.index + 1}</span>
          <span class="chip-text">{term.text}</span>
          <button
            type="button"
            class="chip-remove"
            onclick={(e) => {
              e.stopPropagation();
              removeTerm(term.index);
            }}
            title={`Remove ${term.text}`}
            aria-label={`Remove ${term.text}`}
          >
            <X size={10} strokeWidth={2.5} />
          </button>
        </div>
      {/each}

      <div class="input-shell" class:has-content={inputValue.length > 0}>
        <Plus size={ICON_SIZE.sm} />
        <input
          bind:this={inputEl}
          bind:value={inputValue}
          type="text"
          spellcheck="false"
          autocomplete="off"
          autocapitalize="off"
          placeholder={terms.length === 0
            ? "Type a term, press Enter…"
            : "Add term…"}
          onkeydown={onInputKey}
          onpaste={onInputPaste}
          onblur={() => addTerm(inputValue)}
          aria-label="Add keyterm"
        />
      </div>
    </div>

    <footer class="meta">
      <span class="meta-item">
        <span class="meta-key">Format</span>
        <span class="meta-val">one term per line</span>
      </span>
      <span class="meta-divider"></span>
      <span class="meta-item">
        <span class="meta-key">Bias</span>
        <span class="meta-val">improves recognition of names, jargon, brands</span>
      </span>
    </footer>
  {:else}
    <div class="raw-pane">
      <HighlightedTextarea
        bind:value={rawContent}
        placeholder={doc ? "" : "Loading…"}
        disabled={!doc}
        minHeight={0}
      />
    </div>
  {/if}
</section>

<style>
  .keyterms-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    flex: 1;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .title {
    margin: 0;
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
  }

  .count {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
    padding: 2px var(--space-3);
    margin-left: var(--space-1);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-full);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
    background: var(--surface-overlay-faint);
    transition:
      color var(--motion-fast) var(--ease-default),
      border-color var(--motion-fast) var(--ease-default);
  }

  .count.empty {
    color: var(--text-faint);
  }

  .count-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
    font-family: var(--font-sans);
  }

  .head-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .description {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .load-error {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--danger);
    overflow-wrap: anywhere;
  }

  .comments-note {
    margin: var(--space-1) 0 0;
    font-size: var(--font-size-xs);
    color: var(--text-faint);
    font-style: italic;
  }

  /* ---- chip pane ---- */
  .chip-pane {
    flex: 1;
    align-content: flex-start;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding: var(--space-5);
    background:
      linear-gradient(180deg, var(--surface-overlay-faint), transparent 60%),
      var(--surface-base);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-md);
    cursor: text;
    transition: border-color var(--motion-fast) var(--ease-default);
    overflow-y: auto;
  }

  .chip-pane:focus-within {
    border-color: var(--accent-border);
  }

  .empty-state-chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    pointer-events: none;
  }

  .ghost-chip {
    display: inline-flex;
    align-items: center;
    padding: var(--space-1) var(--space-4);
    border: 1px dashed var(--border-faint);
    border-radius: var(--radius-full);
    font-size: var(--font-size-sm);
    color: var(--text-faint);
    background: transparent;
  }

  .empty-hint {
    font-size: var(--font-size-xs);
    color: var(--text-faint);
    font-style: italic;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-3);
    background: var(--surface-elevated);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-full);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    line-height: 1;
    transition:
      transform var(--motion-fast) var(--ease-default),
      border-color var(--motion-fast) var(--ease-default),
      background var(--motion-fast) var(--ease-default);
    animation: chip-in 180ms cubic-bezier(0.2, 0.7, 0.3, 1);
  }

  .chip:hover {
    border-color: var(--accent-border);
    background: var(--surface-raised);
  }

  .chip-index {
    font-family: var(--font-mono);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    color: var(--text-faint);
    letter-spacing: 0.04em;
    user-select: none;
  }

  .chip-text {
    font-family: var(--font-sans);
    font-feature-settings: "ss01", "cv11";
  }

  .chip-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-full);
    color: var(--text-faint);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-default),
      color var(--motion-fast) var(--ease-default);
  }

  .chip-remove:hover {
    background: var(--accent-bg-soft);
    color: var(--accent-text);
  }

  .input-shell {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border: 1px dashed var(--border-faint);
    border-radius: var(--radius-full);
    color: var(--text-faint);
    background: transparent;
    transition:
      border-color var(--motion-fast) var(--ease-default),
      color var(--motion-fast) var(--ease-default),
      background var(--motion-fast) var(--ease-default);
    min-width: 180px;
  }

  .input-shell:focus-within,
  .input-shell.has-content {
    border-style: solid;
    border-color: var(--accent-border);
    color: var(--accent-text);
    background: var(--accent-bg-soft);
  }

  .input-shell input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font: inherit;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    min-width: 120px;
    padding: 0;
  }

  .input-shell input::placeholder {
    color: var(--text-faint);
  }

  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    padding: 0 var(--space-1);
    font-size: var(--font-size-xs);
    color: var(--text-faint);
  }

  .meta-item {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .meta-key {
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: var(--font-weight-semibold);
    color: var(--text-muted);
    font-size: 9px;
  }

  .meta-val {
    color: var(--text-faint);
  }

  .meta-divider {
    width: 12px;
    height: 1px;
    background: var(--border-faint);
  }

  /* ---- raw pane ---- */
  .raw-pane {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  @keyframes chip-in {
    from {
      opacity: 0;
      transform: translateY(2px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>
