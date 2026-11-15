<script lang="ts">
  import { tick, type Snippet } from "svelte";
  import { ArrowUp, Mic, Square } from "lucide-svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import { getSettingsAgent } from "$lib/stores/settingsAgent.svelte";
  import { insertText } from "$lib/utils/insertText";
  import { autoResize, resizeTextarea } from "$lib/utils/autoResize";
  import ComposerDictationBar from "$lib/components/shared/widgets/ComposerDictationBar.svelte";
  import { useComposerDictation } from "$lib/components/shared/widgets/useComposerDictation.svelte";

  let {
    running,
    modelReady,
    onsend,
    oncancel,
    beforeSend,
  }: {
    running: boolean;
    modelReady: boolean;
    onsend: (text: string) => void;
    oncancel: () => void;
    beforeSend?: Snippet;
  } = $props();

  const agent = getSettingsAgent();

  let input = $state<HTMLTextAreaElement>();
  const dictation = useComposerDictation({ owner: "settings", insert: insertDictation });

  $effect(() => {
    agent.draft;
    if (input) resizeTextarea(input);
  });

  $effect(() => {
    if (agent.focusRequest === 0) return;
    tick().then(() => input?.focus());
  });

  async function insertDictation(_tabId: string | undefined, start: number, end: number, text: string) {
    const result = insertText(agent.draft, start, end, text);
    agent.setDraft(result.text);
    await tick();
    input?.focus();
    input?.setSelectionRange(result.caret, result.caret);
  }

  function startDictation() {
    const length = agent.draft.length;
    dictation.start(undefined, {
      start: input?.selectionStart ?? length,
      end: input?.selectionEnd ?? length,
    });
  }

  function submit() {
    if (!agent.draft.trim() || running || !modelReady || dictation.active) return;
    onsend(agent.draft);
    agent.setDraft("");
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }
</script>

<div class="composer">
  <div class="box">
    <textarea
      bind:this={input}
      bind:value={() => agent.draft, agent.setDraft}
      onkeydown={handleKeydown}
      use:autoResize={"35vh"}
      rows="2"
      readonly={dictation.active}
      placeholder={modelReady ? "Ask about or change a setting…" : "Pick a model to start"}
    ></textarea>
    <div class="bar">
      {#if dictation.active}
        <ComposerDictationBar {dictation} />
      {:else}
        {@render beforeSend?.()}
        <ActionIconButton
          icon={Mic}
          label="Dictate"
          title={dictation.blockedTitle ?? undefined}
          disabled={running || !!dictation.blockedTitle}
          onclick={startDictation}
        />
        {#if running}
          <ActionIconButton icon={Square} variant="stop" label="Stop" onclick={oncancel} />
        {:else}
          <ActionIconButton
            icon={ArrowUp}
            variant="primary"
            label="Send"
            title="Send (Enter)"
            disabled={!agent.draft.trim() || !modelReady}
            onclick={submit}
          />
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .composer {
    padding: 10px 12px 12px;
    border-top: 1px solid var(--border-faint);
  }

  .box {
    max-width: 760px;
    margin: 0 auto;
    border: 1px solid var(--border-default);
    border-radius: 8px;
    background: var(--surface-sunken);
    padding: 8px 8px 6px 10px;
  }

  .box:focus-within {
    border-color: var(--accent-border);
  }

  textarea {
    display: block;
    width: 100%;
    padding: 0;
    background: transparent;
    color: var(--text-primary);
    border: none;
    outline: none;
    font: inherit;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 4px;
  }
</style>
