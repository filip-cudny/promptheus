<script lang="ts">
  import { untrack } from "svelte";
  import {
    Copy,
    Check,
    History,
    ChevronDown,
    CircleAlert,
    RotateCcw,
    LoaderCircle,
  } from "lucide-svelte";
  import { debug as logDebug, error as logError } from "@tauri-apps/plugin-log";
  import { ICON_SIZE } from "$lib/constants/ui";
  import { useCopyFeedback } from "$lib/utils/copyFeedback.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import Chip from "$lib/components/shared/ui/Chip.svelte";
  import RecentInteractionList from "./components/RecentInteractionList.svelte";
  import type { LastInteractionData } from "./itemExtractors";
  import type { LastInteractionChip, RecentQuickAction } from "$lib/services/history";

  let {
    data,
    listOpen = $bindable(false),
    onCopyChip,
    onOpenHistory,
    onRetryTranscription,
    onFetchRecent,
    onCopyPart,
    onOpenInChat,
  }: {
    data: LastInteractionData | null;
    listOpen?: boolean;
    onCopyChip: (chip: LastInteractionChip) => Promise<void>;
    onOpenHistory: () => Promise<void>;
    onRetryTranscription: (entryId: string) => Promise<void>;
    onFetchRecent: () => Promise<RecentQuickAction[]>;
    onCopyPart: (entryId: string, part: "input" | "output") => Promise<boolean>;
    onOpenInChat: (row: RecentQuickAction) => void;
  } = $props();

  const copyFeedback = useCopyFeedback<LastInteractionChip>();

  async function handleCopy(chip: LastInteractionChip, available: boolean) {
    if (!available) return;
    await onCopyChip(chip);
    copyFeedback.flash(chip);
  }

  type ChipEntry = { type: LastInteractionChip; label: string; preview: string | null; title: string };

  let transcription = $derived(data?.transcription ?? null);
  let transcriptionFailed = $derived(transcription?.status === "failed");
  let transcriptionRetrying = $derived(transcription?.status === "retrying");

  let chips = $derived<ChipEntry[]>([
    {
      type: "input",
      label: "Input",
      preview: data?.input?.preview ?? null,
      title: "Copy the last quick action's input",
    },
    {
      type: "output",
      label: "Output",
      preview: data?.output?.preview ?? null,
      title: "Copy the last quick action's output",
    },
  ]);

  let transcriptionTitle = $derived.by(() => {
    if (transcriptionRetrying) return "Retrying transcription…";
    if (transcriptionFailed) {
      const reason = transcription?.error ?? "Transcription failed";
      return transcription?.has_audio
        ? `${reason} — audio kept, retry available`
        : `${reason} — audio no longer available`;
    }
    return "Copy the last transcript";
  });

  async function handleRetry() {
    if (!transcription?.entry_id || !transcription.has_audio) return;
    await onRetryTranscription(transcription.entry_id);
  }

  let recentRows = $state<RecentQuickAction[] | null>(null);
  let fetchSequence = 0;

  async function refreshRecent() {
    const sequence = ++fetchSequence;
    try {
      const fetched = await onFetchRecent();
      if (sequence !== fetchSequence) return;
      recentRows = fetched;
      logDebug(`[last-interaction] fetched ${fetched.length} recent rows`);
    } catch (e) {
      logError(`[last-interaction] fetching recent rows failed: ${e}`);
    }
  }

  $effect(() => {
    if (!listOpen) {
      fetchSequence++;
      recentRows = null;
      return;
    }
    void data;
    untrack(refreshRecent);
  });
</script>

<div class="last-interaction-section">
  <div class="row">
    <div class="chips">
      {#each chips as chip}
        <Chip
          onclick={() => handleCopy(chip.type, chip.preview !== null)}
          disabled={!chip.preview}
          title={chip.title}
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
            {transcriptionRetrying ? "Retrying…" : "Transcript"}
          </span>
        </Chip>

        {#if transcriptionFailed && transcription?.has_audio}
          <ActionIconButton icon={RotateCcw} onclick={handleRetry} label="Retry transcription" />
        {/if}
      </div>
    </div>

    <div class="tools">
      <ActionIconButton
        icon={ChevronDown}
        active={listOpen}
        onclick={() => (listOpen = !listOpen)}
        label="Earlier items"
      />
      <ActionIconButton
        icon={History}
        onclick={onOpenHistory}
        label="View execution history"
      />
    </div>
  </div>

  {#if listOpen}
    <RecentInteractionList
      rows={recentRows}
      {onCopyPart}
      {onOpenInChat}
      onRetry={onRetryTranscription}
      onClose={() => (listOpen = false)}
    />
  {/if}
</div>

<style>
  .last-interaction-section {
    padding: var(--space-1) var(--space-0);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4) var(--space-2) var(--space-6);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-left: auto;
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
