<script lang="ts">
  import SettingsSection from "./SettingsSection.svelte";
  import NumberInput from "$lib/components/shared/ui/NumberInput.svelte";
  import SelectDropdown from "$lib/components/shared/ui/SelectDropdown.svelte";
  import PillAnchorPicker from "./PillAnchorPicker.svelte";
  import SettingField from "./SettingField.svelte";
  import PromptEditor from "./PromptEditor.svelte";
  import SttKeytermsEditor from "./SttKeytermsEditor.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { setSpeechToTextModel, updateSpeechToTextConfig } from "$lib/services/settings";
  import { error as logError } from "@tauri-apps/plugin-log";
  import type { ClaudeTarget, SpeechToTextConfig, StopChatTarget } from "$lib/types";

  const CLAUDE_TARGET_OPTIONS: ReadonlyArray<{ value: ClaudeTarget; label: string }> = [
    { value: "webview", label: "Webview" },
    { value: "desktop", label: "Claude Desktop" },
  ];

  const STOP_CHAT_TARGET_OPTIONS: ReadonlyArray<{ value: StopChatTarget; label: string }> = [
    { value: "chat", label: "Chat" },
    { value: "claude", label: "Claude" },
  ];

  const MAX_ATTEMPTS = 10;
  const MAX_RETENTION_HOURS = 72;

  const store = getSettingsStore();
  const config = $derived(store.settings?.surfaces?.speech_to_text ?? null);
  const sttModelOptions = $derived([
    { value: "", label: "Not set" },
    ...store.models
      .filter((m) => m.type === "stt")
      .map((m) => ({ value: m.id, label: m.display_name || m.model })),
  ]);

  async function saveModel(id: string) {
    try {
      await setSpeechToTextModel(id || null);
    } catch (e) {
      logError(`set_speech_to_text_model failed: ${e}`);
    }
  }

  async function save(patch: Partial<SpeechToTextConfig>) {
    if (!config) return;
    try {
      await updateSpeechToTextConfig({ ...config, ...patch });
    } catch (e) {
      logError(`update_speech_to_text_config failed: ${e}`);
    }
  }

  function clampedNumber(e: Event, min: number, max: number): number {
    const raw = Math.round(Number((e.target as HTMLInputElement).value) || 0);
    return Math.min(max, Math.max(min, raw));
  }

  const attemptsHint = $derived(
    config?.auto_retry_attempts && config.auto_retry_attempts > 1
      ? `A rate limit or provider outage is re-sent up to ${config.auto_retry_attempts - 1} more ${config.auto_retry_attempts === 2 ? "time" : "times"} with growing delays. Invalid keys and rejected requests are never retried.`
      : "Background retrying is off — a single failure ends the transcription.",
  );

  const retentionHint = $derived(
    config?.audio_retention_hours && config.audio_retention_hours > 0
      ? `A failed recording stays on disk for ${config.audio_retention_hours}h so you can retry or export it from history. Roughly 2 MB per minute of audio.`
      : "Audio is discarded immediately. A failed transcription cannot be retried — you would have to dictate again.",
  );
</script>

<SettingsSection
  title="Speech to text"
  hint="Transcription model and prompts, what happens when a transcription fails, and how long the recording survives so you can try again."
>
  {#snippet body()}
    {#if !config}
      <p class="muted">Settings unavailable.</p>
    {:else}
      <div class="group">
        <SettingField key="surfaces.speech_to_text.model_id">
          <SelectDropdown
            value={config.model_id ?? ""}
            options={sttModelOptions}
            ariaLabel="Speech model"
            onchange={saveModel}
          />
        </SettingField>

        <SettingField key="surfaces.speech_to_text.language">
          <input
            class="text"
            type="text"
            aria-label="Language"
            value={config.language ?? ""}
            onchange={(e: Event) =>
              save({ language: (e.target as HTMLInputElement).value.trim() || null })}
          />
        </SettingField>

        <SettingField key="surfaces.speech_to_text.no_verbatim">
          <input
            type="checkbox"
            checked={config.no_verbatim === true}
            onchange={(e: Event) =>
              save({ no_verbatim: (e.target as HTMLInputElement).checked })}
          />
        </SettingField>
      </div>

      <div class="editors">
        <div class="editor">
          <PromptEditor
            kind="speech_to_text"
            minHeight={0}
            description="Optional bias prompt for the speech-to-text model. Leave empty to disable."
          />
        </div>
        <div class="editor">
          <SttKeytermsEditor
            description="Domain words to bias transcription — names, jargon, brands, acronyms. One term per line."
          />
        </div>
      </div>

      <div class="group">
        <h3>Recording</h3>

        <SettingField key="surfaces.speech_to_text.show_recording_widget">
          <input
            type="checkbox"
            checked={config.show_recording_widget}
            onchange={(e: Event) =>
              save({ show_recording_widget: (e.target as HTMLInputElement).checked })}
          />
        </SettingField>
        <SettingField key="surfaces.speech_to_text.recording_widget_at_cursor">
          <input
            type="checkbox"
            checked={config.recording_widget_at_cursor}
            onchange={(e: Event) =>
              save({ recording_widget_at_cursor: (e.target as HTMLInputElement).checked })}
          />
        </SettingField>
        <SettingField key="surfaces.speech_to_text.recording_widget_anchor">
          <PillAnchorPicker
            value={config.recording_widget_anchor}
            disabled={!config.recording_widget_at_cursor}
            onchange={(a) => save({ recording_widget_anchor: a })}
          />
        </SettingField>

        <SettingField key="surfaces.speech_to_text.claude_target">
          <SelectDropdown
            value={config.claude_target}
            options={CLAUDE_TARGET_OPTIONS}
            ariaLabel="Claude target"
            onchange={(claude_target) => save({ claude_target })}
          />
        </SettingField>

        <SettingField key="surfaces.speech_to_text.stop_chat_target">
          <SelectDropdown
            value={config.stop_chat_target}
            options={STOP_CHAT_TARGET_OPTIONS}
            ariaLabel="Stop + chat target"
            onchange={(stop_chat_target) => save({ stop_chat_target })}
          />
        </SettingField>
      </div>

      <div class="group">
        <h3>Failure handling</h3>

        <SettingField key="surfaces.speech_to_text.auto_retry_attempts">
          <NumberInput
            min={1}
            max={MAX_ATTEMPTS}
            step={1}
            value={config.auto_retry_attempts}
            onchange={(e: Event) =>
              save({ auto_retry_attempts: clampedNumber(e, 1, MAX_ATTEMPTS) })}
          />
        </SettingField>
        <p class="muted">{attemptsHint}</p>

        <SettingField key="surfaces.speech_to_text.audio_retention_hours">
          <NumberInput
            min={0}
            max={MAX_RETENTION_HOURS}
            step={1}
            value={config.audio_retention_hours}
            onchange={(e: Event) =>
              save({ audio_retention_hours: clampedNumber(e, 0, MAX_RETENTION_HOURS) })}
          />
        </SettingField>
        <p class="muted">{retentionHint}</p>

        <SettingField key="surfaces.speech_to_text.keep_audio_on_success">
          <input
            type="checkbox"
            checked={config.keep_audio_on_success}
            onchange={(e: Event) =>
              save({ keep_audio_on_success: (e.target as HTMLInputElement).checked })}
          />
        </SettingField>
      </div>
    {/if}
  {/snippet}
</SettingsSection>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 560px;
  }

  h3 {
    margin: 0;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
    color: var(--text-disabled);
  }

  .muted {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .text {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    background: var(--surface-sunken);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
    font: inherit;
    font-size: var(--font-size-md);
    color: var(--text-primary);
  }

  .editors {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: 760px;
  }

  .editor {
    display: flex;
    flex-direction: column;
    height: 320px;
  }

  input[type="checkbox"] {
    cursor: pointer;
  }
</style>
