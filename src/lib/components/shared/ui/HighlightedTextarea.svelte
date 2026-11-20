<script lang="ts">
  import { escapeHtml as escapeAny } from "$lib/utils/format";

  const escapeHtml = (s: string) => escapeAny(s, "&<>");

  let {
    value = $bindable<string>(""),
    highlightPattern,
    unknownTokenChecker,
    placeholder,
    disabled = false,
    minHeight = 320,
  }: {
    value?: string;
    highlightPattern?: RegExp;
    unknownTokenChecker?: (token: string) => boolean;
    placeholder?: string;
    disabled?: boolean;
    minHeight?: number;
  } = $props();

  let textarea = $state<HTMLTextAreaElement | undefined>(undefined);
  let highlightLayer = $state<HTMLPreElement | undefined>(undefined);

  const decorate = $derived(Boolean(highlightPattern));

  const highlighted = $derived(buildHighlighted(value, decorate));

  function buildHighlighted(text: string, on: boolean): string {
    const padded = text.endsWith("\n") ? text + " " : text;
    if (!on || !highlightPattern) return escapeHtml(padded);
    let out = "";
    let lastIndex = 0;
    const re = new RegExp(highlightPattern.source, ensureGlobal(highlightPattern.flags));
    let m: RegExpExecArray | null;
    while ((m = re.exec(padded)) !== null) {
      out += escapeHtml(padded.slice(lastIndex, m.index));
      const token = m[0];
      const known = unknownTokenChecker ? unknownTokenChecker(token) : true;
      const cls = known ? "placeholder-token" : "placeholder-token unknown";
      out += `<span class="${cls}">${escapeHtml(token)}</span>`;
      lastIndex = m.index + token.length;
      if (token.length === 0) re.lastIndex++;
    }
    out += escapeHtml(padded.slice(lastIndex));
    return out;
  }

  function ensureGlobal(flags: string): string {
    return flags.includes("g") ? flags : flags + "g";
  }

  function syncScroll() {
    if (!textarea || !highlightLayer) return;
    highlightLayer.scrollTop = textarea.scrollTop;
    highlightLayer.scrollLeft = textarea.scrollLeft;
  }

  export function insert(token: string) {
    if (!textarea) return;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    value = value.slice(0, start) + token + value.slice(end);
    queueMicrotask(() => {
      if (!textarea) return;
      const pos = start + token.length;
      textarea.focus();
      textarea.setSelectionRange(pos, pos);
    });
  }

  function handleInput(e: Event) {
    value = (e.target as HTMLTextAreaElement).value;
  }
</script>

<div class="frame" class:disabled style="--ht-min-height: {minHeight}px">
  <pre
    class="highlight-layer"
    bind:this={highlightLayer}
    aria-hidden="true">{@html highlighted}</pre>
  <textarea
    bind:this={textarea}
    {value}
    {placeholder}
    {disabled}
    spellcheck="false"
    oninput={handleInput}
    onscroll={syncScroll}
  ></textarea>
</div>

<style>
  .frame {
    position: relative;
    flex: 1;
    min-height: var(--ht-min-height, 320px);
    background: var(--surface-elevated);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: border-color var(--motion-fast) var(--ease-default);
  }

  .frame:focus-within {
    border-color: var(--accent-border);
  }

  .frame.disabled {
    opacity: var(--opacity-disabled);
  }

  .highlight-layer,
  .frame textarea {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: var(--space-4);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    word-break: break-word;
    tab-size: 2;
    border: none;
    background: transparent;
    box-sizing: border-box;
  }

  .highlight-layer {
    pointer-events: none;
    color: var(--text-primary);
    overflow: auto;
    scrollbar-width: none;
  }

  .highlight-layer::-webkit-scrollbar {
    display: none;
  }

  .frame textarea {
    color: transparent;
    caret-color: var(--text-primary);
    resize: none;
    outline: none;
    overflow: auto;
  }

  .frame textarea::selection {
    background: var(--accent-bg);
    color: transparent;
  }

  .highlight-layer :global(.placeholder-token) {
    color: var(--accent-text);
    background: var(--accent-bg-soft);
    border-radius: 2px;
    box-shadow: 0 0 0 1px var(--accent-bg-soft);
    -webkit-box-decoration-break: clone;
    box-decoration-break: clone;
  }

  .highlight-layer :global(.placeholder-token.unknown) {
    color: var(--warning-text);
    background: var(--warning-bg-soft);
    box-shadow: 0 0 0 1px rgba(211, 154, 55, 0.35);
  }
</style>
