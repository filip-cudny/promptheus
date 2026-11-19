<script lang="ts">
  import { untrack } from "svelte";
  import CollapsibleSection from "$lib/components/shared/ui/CollapsibleSection.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import ContextItemChip from "$lib/components/shared/ui/ContextItemChip.svelte";
  import FloatingPanel from "$lib/components/shared/ui/FloatingPanel.svelte";
  import ContextEditor from "$lib/components/shared/widgets/ContextEditor.svelte";
  import { openImagePreview } from "$lib/services/windowPreviews";
  import { parseContextTextItems } from "$lib/utils/contextBlocks";
  import { Save, Check, X, Trash2 } from "lucide-svelte";
  import type { createConversationStore } from "$lib/stores/conversation.svelte";
  import type { ContextValue } from "$lib/utils/contextFollow";

  let {
    store,
    contextDisabled = false,
    initialCollapsed = false,
    onClose,
  }: {
    store: ReturnType<typeof createConversationStore>;
    contextDisabled?: boolean;
    initialCollapsed?: boolean;
    onClose?: () => void;
  } = $props();

  const MAX_HEADER_CHIPS = 3;
  const TEXT_CHIP_TITLE_CHARS = 300;
  const TEXT_CHIP_MAX_WIDTH = "180px";

  let collapsed = $state(untrack(() => initialCollapsed));
  let saving = $state(false);
  let confirming = $state<"close" | "clear" | false>(false);

  let hintBadge: HTMLElement | undefined = $state();
  let hintOpen = $state(false);

  function chipsOf(context: ContextValue) {
    const images = context.images.map((image, idx) => ({ kind: "image" as const, number: image.number ?? idx + 1, image }));
    const texts = parseContextTextItems(context.text).map((item) => ({ kind: "text" as const, ...item }));
    return [...images, ...texts];
  }

  let hasContent = $derived(store.contextText.trim().length > 0 || store.contextImages.length > 0);
  let headerChips = $derived(chipsOf({ text: store.contextText, images: store.contextImages }));
  let globalChips = $derived(chipsOf(store.globalContext));
  let visibleChips = $derived(headerChips.slice(0, MAX_HEADER_CHIPS));
  let chipRow: HTMLElement | null = $state(null);
  let moreChipsLabel: HTMLElement | null = $state(null);
  let fittingChipCount = $state(MAX_HEADER_CHIPS);
  let shownChipCount = $derived(Math.min(fittingChipCount, visibleChips.length));
  let hiddenChipCount = $derived(headerChips.length - shownChipCount);

  $effect(() => {
    const row = chipRow;
    if (!row) return;
    const observer = new ResizeObserver(() => fitChipsInFirstLine(row));
    observer.observe(row);
    return () => observer.disconnect();
  });

  $effect(() => {
    void visibleChips;
    if (chipRow) fitChipsInFirstLine(chipRow);
  });

  function fitChipsInFirstLine(row: HTMLElement) {
    const firstLineBottom = row.getBoundingClientRect().bottom;
    const inFirstLine = (el: Element | null | undefined) => !!el && el.getBoundingClientRect().top < firstLineBottom;
    const slots = row.querySelectorAll(".chip-slot");
    const lastShown = slots[shownChipCount - 1];
    if (shownChipCount > 0 && (!inFirstLine(lastShown) || (moreChipsLabel && !inFirstLine(moreChipsLabel)))) {
      fittingChipCount = shownChipCount - 1;
    } else if (inFirstLine(slots[shownChipCount])) {
      fittingChipCount = shownChipCount + 1;
    }
  }

  async function saveContext() {
    saving = true;
    try {
      await store.saveActiveContextAsGlobal();
    } finally {
      saving = false;
    }
  }

  function keepOwn() {
    hintOpen = false;
    store.keepActiveOwnContext();
  }

  function useGlobal() {
    hintOpen = false;
    store.useGlobalContextInActiveTab();
  }

  function requestClose() {
    if (hasContent) {
      confirming = "close";
    } else {
      onClose?.();
    }
  }

  function requestClear() {
    confirming = "clear";
  }

  function confirmAction() {
    const action = confirming;
    confirming = false;
    if (action === "close") onClose?.();
    else if (action === "clear") store.clearActiveContext();
  }

  function cancelConfirm() {
    confirming = false;
  }
</script>

<div class="context-inline" class:disabled={contextDisabled} class:hinted={store.contextHint}>
  {#if confirming}
    <div class="confirm-bar">
      <span class="confirm-text">
        {confirming === "close" ? "Clear context and close?" : "Clear context for this conversation?"}
      </span>
      <div class="confirm-actions">
        <Button variant="danger" onclick={confirmAction}>Clear context</Button>
        <Button onclick={cancelConfirm}>Cancel</Button>
      </div>
    </div>
  {:else}
    <CollapsibleSection
      title="Context"
      titleTooltip="Attached to every message in this conversation. Click to edit."
      bind:collapsed
    >
      {#snippet summary()}
        {#if collapsed && hasContent}
          <span class="chip-row" bind:this={chipRow}>
            {#each visibleChips as chip, idx}
              <span class="chip-slot" class:clipped={idx >= shownChipCount}>
                {#if chip.kind === "image"}
                  <ContextItemChip
                    kind="image"
                    number={chip.number}
                    src={`data:${chip.image.media_type};base64,${chip.image.data}`}
                    title="Open image #{chip.number}"
                    onclick={() => openImagePreview(chip.image.data, chip.image.media_type)}
                  />
                {:else}
                  <ContextItemChip
                    kind="text"
                    number={chip.number}
                    text={chip.text}
                    maxWidth={TEXT_CHIP_MAX_WIDTH}
                    title={chip.text.slice(0, TEXT_CHIP_TITLE_CHARS)}
                    onclick={() => (collapsed = false)}
                  />
                {/if}
              </span>
            {/each}
            {#if hiddenChipCount > 0}
              <span class="more-chips" bind:this={moreChipsLabel}>+{hiddenChipCount}</span>
            {/if}
          </span>
        {/if}
        {#if store.contextHint}
          <button
            type="button"
            class="hint-badge"
            bind:this={hintBadge}
            onclick={(e) => {
              e.stopPropagation();
              hintOpen = !hintOpen;
            }}
          >
            <span class="hint-dot"></span>Global changed
          </button>
        {/if}
      {/snippet}
      {#snippet actions()}
        <ActionIconButton
          icon={Trash2}
          variant="danger"
          onclick={requestClear}
          label="Clear context in this conversation (global context unchanged)"
          disabled={contextDisabled || !hasContent}
        />
        <ActionIconButton
          icon={Save}
          confirmIcon={Check}
          onclick={saveContext}
          label="Replace the global context with this conversation's context"
          disabled={contextDisabled || saving}
        />
        <ActionIconButton
          icon={X}
          onclick={requestClose}
          label="Remove context from this conversation and hide it"
        />
      {/snippet}
      <ContextEditor
        bind:text={() => store.contextText, (v) => store.editActiveContextText(v)}
        bind:images={() => store.contextImages, (v) => store.editActiveContextImages(v)}
        disabled={contextDisabled}
        placeholder={contextDisabled ? "This prompt doesn't use {{context}}" : "Enter context text\u2026"}
      />
    </CollapsibleSection>
  {/if}
</div>

<FloatingPanel visible={hintOpen && store.contextHint} anchorEl={hintBadge} align="end" fitContent flush={false} onclose={() => (hintOpen = false)}>
  <div class="hint-panel">
    <p class="hint-text">The global context changed. This conversation uses its own version.</p>
    <div class="hint-caption">Global context</div>
    <div class="hint-chips">
      {#each globalChips as chip}
        {#if chip.kind === "image"}
          <ContextItemChip
            kind="image"
            number={chip.number}
            src={`data:${chip.image.media_type};base64,${chip.image.data}`}
            title="Open image #{chip.number}"
            onclick={() => openImagePreview(chip.image.data, chip.image.media_type)}
          />
        {:else}
          <ContextItemChip
            kind="text"
            number={chip.number}
            text={chip.text}
            maxWidth={TEXT_CHIP_MAX_WIDTH}
            title={chip.text.slice(0, TEXT_CHIP_TITLE_CHARS)}
          />
        {/if}
      {/each}
    </div>
    <div class="hint-actions">
      <Button variant="ghost" onclick={keepOwn}>Keep this</Button>
      <Button variant="primary" onclick={useGlobal}>Use global</Button>
    </div>
  </div>
</FloatingPanel>

<style>
  .context-inline {
    position: relative;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border-strong);
  }

  .context-inline.hinted {
    box-shadow: inset 2px 0 0 var(--warning);
  }

  .context-inline::before {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    z-index: 1;
    width: var(--chat-top-buttons-end);
    height: 40px;
    transition: width var(--motion-slow) var(--ease-default);
  }

  .context-inline.disabled {
    opacity: var(--opacity-disabled);
  }

  .context-inline :global(.collapsible-section) {
    border: none;
    border-radius: 0;
  }

  .context-inline :global(.collapsible-header),
  .confirm-bar {
    height: 40px;
    padding: 0 var(--space-5) 0 calc(var(--chat-top-buttons-end) + var(--space-3));
    transition: padding-left var(--motion-slow) var(--ease-default);
  }

  .context-inline :global(.collapsible-header) {
    background: transparent;
    font-size: var(--font-size-md);
  }

  .context-inline :global(.collapsible-header:hover) {
    background: var(--surface-overlay-faint);
  }

  .context-inline :global(.collapsible-title) {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .context-inline :global(.collapsible-header:not(:hover) .collapsible-actions) {
    opacity: 0.5;
  }

  .context-inline :global(.collapsible-content) {
    padding: var(--space-3) var(--space-5) var(--space-4);
    max-height: 35vh;
    overflow-y: auto;
  }

  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    flex: 1;
    min-width: 0;
    height: 20px;
    overflow: hidden;
  }

  .chip-slot {
    display: flex;
    min-width: 0;
  }

  .chip-slot.clipped {
    order: 2;
    visibility: hidden;
  }

  .more-chips {
    order: 1;
    flex: none;
    line-height: 20px;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .hint-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    height: 22px;
    padding: 0 8px;
    border: none;
    border-radius: 11px;
    background: var(--warning-bg-soft);
    color: var(--warning);
    font: inherit;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    white-space: nowrap;
    cursor: pointer;
  }

  .hint-dot {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: var(--warning);
  }

  .hint-panel {
    width: 330px;
    max-width: 100%;
  }

  .hint-text {
    margin: 0 0 var(--space-4);
    font-size: var(--font-size-md);
    color: var(--text-secondary);
  }

  .hint-caption {
    margin-bottom: var(--space-3);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .hint-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    margin-bottom: var(--space-5);
  }

  .hint-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
  }

  .confirm-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    background: var(--danger-bg-soft);
  }

  .confirm-text {
    font-size: var(--font-size-md);
    color: var(--text-primary);
  }

  .confirm-actions {
    display: flex;
    gap: var(--space-3);
    flex-shrink: 0;
  }
</style>
