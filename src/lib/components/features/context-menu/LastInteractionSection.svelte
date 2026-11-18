<script lang="ts">
  import {
    Copy,
    Check,
    History,
    CircleAlert,
    RotateCcw,
    LoaderCircle,
  } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { useCopyFeedback } from "$lib/utils/copyFeedback.svelte";
  import SectionHeader from "./components/SectionHeader.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import Chip from "$lib/components/shared/ui/Chip.svelte";
  import type { LastInteractionData } from "./itemExtractors";
  import type { LastInteractionChip } from "$lib/services/history";

  let {
    data,
    onCopyChip,
    onOpenHistory,
    onRetryTranscription,
  }: {
    data: LastInteractionData | null;
    onCopyChip: (chip: LastInteractionChip) => Promise<void>;
    onOpenHistory: () => Promise<void>;
    onRetryTranscription: (entryId: string) => Promise<void>;
  } = $props();

  const copyFeedback = useCopyFeedback<LastInteractionChip>();

  async function handleCopy(chip: LastInteractionChip, available: boolean) {
    if (!available) return;
    await onCopyChip(chip);
    copyFeedback.flash(chip);
  }

  type ChipEntry = { type: LastInteractionChip; label: string; preview: string | null };

  let transcription = $derived(data?.transcription ?? null);
  let transcriptionFailed = $derived(transcription?.status === "failed");
  let transcriptionRetrying = $derived(transcription?.status === "retrying");

  let chips = $derived<ChipEntry[]>([
    { type: "input", label: "Input", preview: data?.input?.preview ?? null },
    { type: "output", label: "Output", preview: data?.output?.preview ?? null },
  ]);

  let transcriptionTitle = $derived.by(() => {
    if (transcriptionRetrying) return "Retrying transcription…";
    if (transcriptionFailed) {
      const reason = transcription?.error ?? "Transcription failed";
      return transcription?.has_audio
        ? `${reason} — audio kept, retry available`
        : `${reason} — audio no longer available`;
    }
    return transcription?.preview ?? "No content";
  });

  let hasAnyContent = $derived(
    chips.some((c) => c.preview !== null) || transcription !== null,
  );

  async function handleRetry() {
    if (!transcription?.entry_id || !transcription.has_audio) return;
    await onRetryTranscription(transcription.entry_id);
  }
</script>

<div class="last-interaction-section">
  <SectionHeader title="Last interaction">
    {#snippet actions()}
      <ActionIconButton
        icon={History}
        onclick={onOpenHistory}
        label="View execution history"
      />
    {/snippet}
  </SectionHeader>

  {#if hasAnyContent}
    <div class="chips">
      {#each chips as chip}
        <Chip
          onclick={() => handleCopy(chip.type, chip.preview !== null)}
          disabled={!chip.preview}
          title={chip.preview ?? "No content"}
        >
          <span class="chip-copy">
            {#if copyFeedback.confirmed === chip.type}
              <Check size={ICON_SIZE.md} />
            {:else}
              <Copy size={ICON_SIZE.md} />
            {/if}
          </span>
          <span class="chip-label">{chip.label}</span>
        </Chip>
      {/each}

      <div class="transcription-group" class:failed={transcriptionFailed}>
        <Chip
          onclick={() => handleCopy("transcription", !!transcription?.preview && !transcriptionRetrying)}
          disabled={!transcription?.preview || transcriptionRetrying}
          title={transcriptionTitle}
        >
          <span class="chip-copy">
            {#if transcriptionRetrying}
              <span class="spinning"><LoaderCircle size={ICON_SIZE.md} /></span>
            {:else if transcriptionFailed}
              <CircleAlert size={ICON_SIZE.md} />
            {:else if copyFeedback.confirmed === "transcription"}
              <Check size={ICON_SIZE.md} />
            {:else}
              <Copy size={ICON_SIZE.md} />
            {/if}
          </span>
          <span class="chip-label">
            {transcriptionRetrying ? "Retrying…" : "Transcription"}
          </span>
        </Chip>

        {#if transcriptionFailed && transcription?.has_audio}
          <ActionIconButton icon={RotateCcw} onclick={handleRetry} label="Retry transcription" />
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .last-interaction-section {
    padding: var(--space-1) var(--space-0);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-6) var(--space-3);
  }

  .chip-copy {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .chip-label {
    font-weight: var(--font-weight-medium);
  }

  .transcription-group {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .transcription-group.failed :global(.chip) {
    color: var(--danger);
    border-color: var(--danger-border);
  }

  .spinning {
    display: flex;
    animation: spin 1s linear infinite;
  }
</style>
