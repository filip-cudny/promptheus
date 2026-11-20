<script lang="ts">
  import { getSkillsStore } from "$lib/stores/skills.svelte";
  import { highlightSkills, fuzzyMatch } from "$lib/utils/skillHighlight";
  import { resizeTextarea } from "$lib/utils/autoResize";
  import { UndoStack } from "$lib/utils/undoStack";
  import { handleListNavKey } from "$lib/utils/listNavigation";
  import type { SkillSummary } from "$lib/types";

  let {
    text = $bindable(""),
    placeholder = "",
    editableClass = "",
    readonly = false,
    oninput,
    onkeydown,
    onpaste,
  }: {
    text?: string;
    placeholder?: string;
    editableClass?: string;
    readonly?: boolean;
    oninput?: () => void;
    onkeydown?: (e: KeyboardEvent) => void;
    onpaste?: (e: ClipboardEvent) => void;
  } = $props();

  let textarea: HTMLTextAreaElement | undefined = $state();
  let overlay: HTMLDivElement | undefined = $state();
  const skillsStore = getSkillsStore();
  let showAutocomplete = $state(false);
  let autocompleteItems = $state<SkillSummary[]>([]);
  let autocompleteIndex = $state(0);
  let slashStart = $state(-1);
  let dropdownEl: HTMLDivElement | undefined = $state();

  const undoStack = new UndoStack();
  let isUndoRedoAction = false;
  let snapshotTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (showAutocomplete && dropdownEl) {
      const _idx = autocompleteIndex;
      const selected = dropdownEl.querySelector(".autocomplete-item.selected");
      selected?.scrollIntoView({ block: "nearest" });
    }
  });

  const highlightedHtml = $derived(
    highlightSkills(text, classifySkillToken, "<br>"),
  );

  export function focus() {
    textarea?.focus();
  }

  export function setTextAndHighlight(newText: string) {
    text = newText;
  }

  export function getSelection(): { start: number; end: number } {
    return { start: textarea?.selectionStart ?? text.length, end: textarea?.selectionEnd ?? text.length };
  }

  export function restoreCursor(offset: number) {
    if (!textarea) return;
    textarea.selectionStart = textarea.selectionEnd = offset;
  }

  export function resetUndoStack(seedText?: string) {
    clearTimeout(snapshotTimer);
    const cursor = seedText?.length ?? 0;
    undoStack.reset(seedText != null ? { text: seedText, cursorStart: cursor, cursorEnd: cursor } : undefined);
  }

  function captureSnapshot() {
    if (!textarea) return;
    undoStack.push({
      text: text,
      cursorStart: textarea.selectionStart,
      cursorEnd: textarea.selectionEnd,
    });
  }

  function scheduleSnapshot() {
    clearTimeout(snapshotTimer);
    snapshotTimer = setTimeout(captureSnapshot, 300);
  }

  function flushSnapshot() {
    if (snapshotTimer !== undefined) {
      clearTimeout(snapshotTimer);
      captureSnapshot();
    }
  }

  function performUndo() {
    flushSnapshot();
    const entry = undoStack.undo();
    if (!entry) return;
    isUndoRedoAction = true;
    text = entry.text;
    requestAnimationFrame(() => {
      if (!textarea) return;
      textarea.selectionStart = entry.cursorStart;
      textarea.selectionEnd = entry.cursorEnd;
    });
  }

  function performRedo() {
    const entry = undoStack.redo();
    if (!entry) return;
    isUndoRedoAction = true;
    text = entry.text;
    requestAnimationFrame(() => {
      if (!textarea) return;
      textarea.selectionStart = entry.cursorStart;
      textarea.selectionEnd = entry.cursorEnd;
    });
  }

  function classifySkillToken(token: string, finished: boolean): string | null {
    const name = token.slice(1);
    if (!name) return "hl-skill-partial";
    if (skillsStore.nameSet.has(name)) return "hl-skill";
    if (finished) return null;
    const hasMatch = skillsStore.items.some(
      (s) =>
        fuzzyMatch(name, s.name) !== null ||
        fuzzyMatch(name, s.display_name.toLowerCase()) !== null,
    );
    return hasMatch ? "hl-skill-partial" : null;
  }

  $effect(() => {
    text;
    if (textarea) requestAnimationFrame(() => resizeTextarea(textarea!));
  });

  function handleInput() {
    if (textarea) text = textarea.value;
    if (isUndoRedoAction) {
      isUndoRedoAction = false;
    } else {
      scheduleSnapshot();
    }
    detectSlashCommand();
    oninput?.();
  }

  function syncScroll() {
    if (overlay && textarea) {
      overlay.scrollTop = textarea.scrollTop;
    }
  }

  function detectSlashCommand() {
    if (!textarea) {
      closeAutocomplete();
      return;
    }

    const offset = textarea.selectionStart;
    const textBefore = text.slice(0, offset);
    const match = textBefore.match(/(^|\s)(\/[a-z0-9-]*)$/);

    if (match) {
      const slashToken = match[2];
      const query = slashToken.slice(1);
      slashStart = offset - slashToken.length;

      const scored = skillsStore.items
        .map((s) => {
          if (!query) return { skill: s, score: 0 };
          const nameScore = fuzzyMatch(query, s.name);
          const displayScore = fuzzyMatch(query, s.display_name.toLowerCase());
          const best = Math.max(nameScore ?? -1, displayScore ?? -1);
          return best >= 0 ? { skill: s, score: best } : null;
        })
        .filter((x): x is { skill: SkillSummary; score: number } => x !== null)
        .sort((a, b) => b.score - a.score);

      if (scored.length > 0) {
        autocompleteItems = scored.map((s) => s.skill);
        autocompleteIndex = 0;
        showAutocomplete = true;
        return;
      }
    }

    closeAutocomplete();
  }

  function closeAutocomplete() {
    showAutocomplete = false;
    autocompleteItems = [];
    autocompleteIndex = 0;
    slashStart = -1;
  }

  function insertSkill(skill: SkillSummary) {
    if (!textarea || slashStart < 0) return;

    flushSnapshot();

    const cursorOffset = textarea.selectionStart;
    const before = text.slice(0, slashStart);
    const after = text.slice(cursorOffset);
    text = `${before}/${skill.name} ${after}`;

    const newOffset = slashStart + skill.name.length + 2;
    closeAutocomplete();

    undoStack.push({ text, cursorStart: newOffset, cursorEnd: newOffset });

    requestAnimationFrame(() => {
      if (!textarea) return;
      textarea.value = text;
      textarea.selectionStart = textarea.selectionEnd = newOffset;
      textarea.focus();
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === "z" && !e.shiftKey) {
      e.preventDefault();
      performUndo();
      return;
    }
    if (mod && e.key === "z" && e.shiftKey) {
      e.preventDefault();
      performRedo();
      return;
    }

    if (e.key === "Escape" && showAutocomplete) {
      e.preventDefault();
      closeAutocomplete();
      return;
    }

    if (showAutocomplete) {
      const next = handleListNavKey(e, autocompleteIndex, autocompleteItems.length, { wrap: true });
      if (next !== null) {
        e.preventDefault();
        e.stopPropagation();
        autocompleteIndex = next;
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        if (autocompleteItems.length > 0) {
          insertSkill(autocompleteItems[autocompleteIndex]);
        }
        return;
      }
    }

    onkeydown?.(e);
  }
</script>

<div class="skill-editable-wrapper">
  <div
    bind:this={overlay}
    class="highlight-overlay {editableClass}"
    aria-hidden="true"
  >{@html highlightedHtml}&nbsp;</div>
  <textarea
    bind:this={textarea}
    bind:value={text}
    class="skill-textarea {editableClass}"
    {placeholder}
    {readonly}
    rows={1}
    oninput={handleInput}
    onkeydown={handleKeydown}
    onpaste={onpaste}
    onscroll={syncScroll}
  ></textarea>

  {#if showAutocomplete && autocompleteItems.length > 0}
    <div class="autocomplete-dropdown" bind:this={dropdownEl}>
      {#each autocompleteItems as item, i}
        <button
          class="autocomplete-item"
          class:selected={i === autocompleteIndex}
          onmousedown={(e) => { e.preventDefault(); insertSkill(item); }}
          onmouseenter={() => autocompleteIndex = i}
        >
          <span class="autocomplete-name">/{item.name}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .skill-editable-wrapper {
    position: relative;
  }

  .highlight-overlay,
  .skill-textarea {
    width: 100%;
    min-height: 1.5em;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font: inherit;
    white-space: pre-wrap;
    word-wrap: break-word;
    overflow-y: auto;
    box-sizing: border-box;
    margin: var(--space-0);
    padding: var(--space-0);
    line-height: inherit;
    letter-spacing: inherit;
  }

  .skill-textarea {
    position: relative;
    z-index: 1;
    color: transparent;
    caret-color: var(--text-primary);
    outline: none;
    resize: none;
  }

  .highlight-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: var(--z-base);
    overflow: hidden;
  }

  .highlight-overlay :global(.hl-skill) {
    color: var(--accent);
  }

  .highlight-overlay :global(.hl-skill-partial) {
    color: var(--accent);
    text-decoration: underline dashed;
  }

  .autocomplete-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    right: 0;
    max-height: 180px;
    overflow-y: auto;
    background: var(--surface-elevated);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    margin-bottom: var(--space-2);
    z-index: var(--z-dropdown);
  }

  .autocomplete-item {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    width: 100%;
    padding: var(--space-3) var(--space-5);
    border: none;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
    cursor: pointer;
    text-align: left;
  }

  .autocomplete-item.selected {
    background: var(--accent-bg-soft);
  }

  .autocomplete-item:hover {
    background: var(--accent-bg-soft);
  }

  .autocomplete-name {
    color: var(--accent-text);
    font-family: var(--font-mono);
    flex-shrink: 0;
  }
</style>
