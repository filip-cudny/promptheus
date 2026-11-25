<script lang="ts">
  import { classifyLines, placePaste } from "$lib/utils/contextSource";
  import { getImageFromPasteEvent } from "$lib/utils/paste";
  import { readClipboardForEditable } from "$lib/services/clipboardPaste";
  import { isMac } from "$lib/utils/platform";
  import { TEXT_ATTACHMENT_CHAR_THRESHOLD } from "$lib/constants/ui";
  import type { ConversationImage } from "$lib/types/conversation";

  let {
    value = $bindable(""),
    placeholder = "Enter context text…",
    onimage,
  }: {
    value?: string;
    placeholder?: string;
    onimage: (image: ConversationImage) => void;
  } = $props();

  let textarea: HTMLTextAreaElement | undefined = $state();
  let mirror: HTMLDivElement | undefined = $state();
  let shiftHeld = false;

  const lines = $derived(classifyLines(value));
  const damagedLines = $derived(
    lines.flatMap((line, index) => (line.kind === "damaged" ? [{ ...line, lineNumber: index + 1 }] : [])),
  );

  function syncScroll() {
    if (textarea && mirror) mirror.scrollTop = textarea.scrollTop;
  }

  $effect(() => {
    value;
    requestAnimationFrame(syncScroll);
  });

  async function handlePaste(e: ClipboardEvent) {
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (text) {
      e.preventDefault();
      const target = e.currentTarget as HTMLTextAreaElement;
      const placement = placePaste(value, target.selectionStart, target.selectionEnd, text, {
        threshold: TEXT_ATTACHMENT_CHAR_THRESHOLD,
        plain: shiftHeld,
      });
      target.setSelectionRange(placement.start, placement.end);
      document.execCommand("insertText", false, placement.text);
      return;
    }
    const image = await getImageFromPasteEvent(e);
    if (image) onimage(image);
  }

  async function handleKeydown(e: KeyboardEvent) {
    shiftHeld = e.shiftKey;
    if (isMac && e.key.toLowerCase() === "v" && e.shiftKey && e.metaKey) {
      e.preventDefault();
      const result = await readClipboardForEditable();
      if (result?.kind === "text") {
        document.execCommand("insertText", false, result.text);
      } else if (result?.kind === "image") {
        onimage({ data: result.data, media_type: result.mediaType });
      }
    }
  }

  function handleKeyup(e: KeyboardEvent) {
    shiftHeld = e.shiftKey;
  }
</script>

<div class="field">
  <div class="frame">
    <div class="mirror" bind:this={mirror} aria-hidden="true">
      {#each lines as line}
        {#if line.kind === "marker"}
          <div class="line marker"><span class="marker-text" data-number={line.number ?? undefined}>{line.text}</span></div>
        {:else if line.kind === "damaged"}
          <div class="line damaged">{line.text}</div>
        {:else}
          <div class="line">{line.text || " "}</div>
        {/if}
      {/each}
    </div>
    <textarea
      bind:this={textarea}
      bind:value
      {placeholder}
      spellcheck="false"
      onscroll={syncScroll}
      onpaste={handlePaste}
      onkeydown={handleKeydown}
      onkeyup={handleKeyup}
    ></textarea>
  </div>
  {#each damagedLines as line}
    <div class="status" role="status">
      Line {line.lineNumber}: "{line.text.trim()}" is not a --- text marker; it is saved into Text #{line.itemNumber}
    </div>
  {/each}
</div>

<style>
  .field {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .frame {
    position: relative;
    flex: 1;
    min-height: 100px;
  }

  .mirror,
  textarea {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: var(--space-4);
    font: inherit;
    font-size: var(--font-size-base);
    line-height: var(--line-height-normal);
    letter-spacing: normal;
    tab-size: 2;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    box-sizing: border-box;
    border: none;
    scrollbar-gutter: stable;
  }

  .mirror {
    overflow: hidden;
    pointer-events: none;
    color: var(--text-primary);
  }

  textarea {
    overflow-y: auto;
    background: transparent;
    color: transparent;
    caret-color: var(--text-primary);
    resize: none;
    outline: none;
  }

  textarea::selection {
    background: var(--accent-bg);
    color: transparent;
  }

  textarea::placeholder {
    color: var(--text-disabled);
  }

  .marker {
    background: var(--accent-bg-soft);
  }

  .marker-text {
    position: relative;
  }

  .marker-text[data-number]::after {
    content: " #" attr(data-number);
    position: absolute;
    left: 100%;
    top: 0;
    white-space: pre;
    color: var(--text-disabled);
  }

  .damaged {
    color: var(--warning-text);
    background: var(--warning-bg-soft);
  }

  .status {
    font-size: var(--font-size-sm);
    color: var(--warning-text);
    padding: var(--space-1) var(--space-4);
  }
</style>
