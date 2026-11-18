<script lang="ts">
  import { onMount, untrack } from "svelte";
  import CollapsibleSection from "$lib/components/shared/ui/CollapsibleSection.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import ContextItemChip from "$lib/components/shared/ui/ContextItemChip.svelte";
  import ContextEditor from "$lib/components/shared/widgets/ContextEditor.svelte";
  import { appendContext, appendContextImage, clearContext } from "$lib/services/context";
  import { openImagePreview } from "$lib/services/windowPreviews";
  import { parseContextTextItems } from "$lib/utils/contextBlocks";
  import { Save, Check, X, Trash2 } from "lucide-svelte";
  import { getGlobalContext, unwrapContextBlocks, type createConversationStore } from "$lib/stores/conversation.svelte";
  import type { ConversationImage } from "$lib/types/conversation";

  let {
    store,
    contextDisabled = false,
    initialCollapsed = false,
    onHasContent,
    onClose,
  }: {
    store: ReturnType<typeof createConversationStore>;
    contextDisabled?: boolean;
    initialCollapsed?: boolean;
    onHasContent?: () => void;
    onClose?: () => void;
  } = $props();

  const MAX_HEADER_CHIPS = 3;
  const TEXT_CHIP_TITLE_CHARS = 300;
  const TEXT_CHIP_MAX_WIDTH = "180px";

  let collapsed = $state(untrack(() => initialCollapsed));
  let saving = $state(false);
  let confirming = $state<"close" | "clear" | false>(false);

  let localText = $state(untrack(() => store.contextText));
  let localImages = $state<ConversationImage[]>(untrack(() => store.contextImages));

  let hasContent = $derived(localText.trim().length > 0 || localImages.length > 0);
  let headerChips = $derived.by(() => {
    const images = localImages.map((image, idx) => ({ kind: "image" as const, number: image.number ?? idx + 1, image }));
    const texts = parseContextTextItems(localText).map((item) => ({ kind: "text" as const, ...item }));
    return [...images, ...texts];
  });
  let visibleChips = $derived(headerChips.slice(0, MAX_HEADER_CHIPS));
  let chipRow: HTMLElement | null = $state(null);
  let moreChipsLabel: HTMLElement | null = $state(null);
  let fittingChipCount = $state(MAX_HEADER_CHIPS);
  let shownChipCount = $derived(Math.min(fittingChipCount, visibleChips.length));
  let hiddenChipCount = $derived(headerChips.length - shownChipCount);
  let contextHeaderClass = $derived(collapsed && hasContent ? "context-has-content" : "");

  $effect(() => {
    store.updateContextText(localText);
  });

  $effect(() => {
    store.updateContextImages(localImages);
  });

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

  onMount(async () => {
    try {
      ({ text: localText, images: localImages } = await getGlobalContext());
      if (localText.trim().length > 0 || localImages.length > 0) {
        onHasContent?.();
      }
    } catch {
      // non-fatal
    }
  });

  async function saveContext() {
    saving = true;
    try {
      await clearContext();
      for (const img of localImages) {
        await appendContextImage(img.data, img.media_type);
      }
      const savedText = unwrapContextBlocks(localText);
      if (savedText.trim()) {
        await appendContext(savedText);
      }
      ({ text: localText, images: localImages } = await getGlobalContext());
    } finally {
      saving = false;
    }
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
    if (confirming === "close") {
      localText = "";
      localImages = [];
      confirming = false;
      onClose?.();
    } else if (confirming === "clear") {
      localText = "";
      localImages = [];
      confirming = false;
    }
  }

  function cancelConfirm() {
    confirming = false;
  }
</script>

<div class="context-inline" class:disabled={contextDisabled}>
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
      titleTooltip="Sent with every message in this conversation"
      bind:collapsed
      headerClass={contextHeaderClass}
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
        bind:text={localText}
        bind:images={localImages}
        disabled={contextDisabled}
        placeholder={contextDisabled ? "This prompt doesn't use {{context}}" : "Enter context text\u2026"}
      />
    </CollapsibleSection>
  {/if}
</div>

<style>
  .context-inline {
    position: relative;
    flex-shrink: 0;
    background: var(--accent-bg-soft);
    border-bottom: 1px solid var(--border-strong);
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

  .context-inline :global(.collapsible-header.context-has-content .collapsible-title) {
    color: var(--info);
    font-weight: 700;
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
