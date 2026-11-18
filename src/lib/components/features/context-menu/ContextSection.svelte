<script lang="ts">
  import type { ContextItem } from "$lib/types/context";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import SectionHeader from "./components/SectionHeader.svelte";
  import ContextItemChip from "$lib/components/shared/ui/ContextItemChip.svelte";
  import {
    FileSymlink,
    FilePlus,
    Pencil,
    Copy,
    Trash2,
    Check,
  } from "lucide-svelte";
  import { useCopyFeedback } from "$lib/utils/copyFeedback.svelte";

  let {
    items,
    onReplaceFromClipboard,
    onAppendFromClipboard,
    onOpenEditor,
    onCopyAll,
    onClear,
    onRemoveItem,
    onOpenImagePreview,
  }: {
    items: ContextItem[];
    onReplaceFromClipboard: () => Promise<void>;
    onAppendFromClipboard: () => Promise<void>;
    onOpenEditor: () => Promise<void>;
    onCopyAll: () => Promise<void>;
    onClear: () => Promise<void>;
    onRemoveItem: (index: number) => Promise<boolean | void>;
    onOpenImagePreview: (data: string, mediaType: string) => void;
  } = $props();

  let hasTextItems = $derived(items.some((i) => i.item_type === "text"));
  let isEmpty = $derived(items.length === 0);
  const copyFeedback = useCopyFeedback<number>();

  const imageEntries = $derived(
    items.flatMap((item, idx) => (item.item_type === "image" ? [{ item, idx }] : [])),
  );
  const textEntries = $derived(
    items.flatMap((item, idx) => (item.item_type === "text" ? [{ item, idx }] : [])),
  );

  async function handleChipCopy(idx: number, content: string) {
    await navigator.clipboard.writeText(content);
    copyFeedback.flash(idx);
  }
</script>

<div class="context-section">
  <SectionHeader title="Context" count={isEmpty ? undefined : items.length}>
    {#snippet actions()}
      <ActionIconButton
        icon={FileSymlink}
        confirmIcon={Check}
        onclick={onReplaceFromClipboard}
        label="Replace context with clipboard"
      />
      <ActionIconButton
        icon={FilePlus}
        confirmIcon={Check}
        onclick={onAppendFromClipboard}
        label="Append clipboard to context"
      />
      <ActionIconButton
        icon={Pencil}
        onclick={onOpenEditor}
        label="Edit context"
      />
      <ActionIconButton
        icon={Copy}
        confirmIcon={Check}
        onclick={onCopyAll}
        label="Copy context text"
        disabled={!hasTextItems}
      />
      <ActionIconButton
        icon={Trash2}
        confirmIcon={Check}
        onclick={onClear}
        label="Clear all context"
        variant="danger"
        disabled={isEmpty}
      />
    {/snippet}
  </SectionHeader>

  {#if !isEmpty}
    <div class="chips">
      {#each imageEntries as { item, idx } (idx)}
        <ContextItemChip
          kind="image"
          number={item.number}
          src={`data:${item.media_type};base64,${item.data}`}
          title="Open image #{item.number}"
          onclick={() => onOpenImagePreview(item.data, item.media_type)}
          onremove={() => onRemoveItem(idx)}
          removeLabel="Remove image #{item.number}"
        />
      {/each}
      {#each textEntries as { item, idx } (idx)}
        <ContextItemChip
          kind="text"
          number={item.number}
          text={item.content}
          icon={copyFeedback.confirmed === idx ? Check : undefined}
          title={item.content}
          onclick={() => handleChipCopy(idx, item.content)}
          onremove={() => onRemoveItem(idx)}
          removeLabel="Remove text #{item.number}"
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .context-section {
    padding: var(--space-1) var(--space-0);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-6) var(--space-1);
    overflow: hidden;
  }
</style>
