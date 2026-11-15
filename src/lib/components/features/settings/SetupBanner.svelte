<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { X } from "lucide-svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import { getSettingsAgent } from "$lib/stores/settingsAgent.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { getSetupBanner } from "$lib/stores/setupBanner.svelte";

  const AGENT_REQUEST =
    "Help me finish setting up: keyboard shortcuts for the context menu and dictation, and a speech-to-text model.";

  const setup = getSetupBanner();
  const settings = getSettingsStore();
  const agent = getSettingsAgent();

  onMount(() => void setup.init());
  onDestroy(setup.destroy);

  $effect(() => {
    if (setup.active && settings.settings && setup.current) setup.applyInitial();
  });

  async function finishWithAgent() {
    await agent.openPanel("reopen");
    agent.fillDraft(AGENT_REQUEST);
  }
</script>

{#if setup.active && setup.current}
  {@const step = setup.current}
  <div class="banner" role="region" aria-label="Set up Promptheus">
    <div class="text">
      <div class="step">SET UP PROMPTHEUS · STEP {step.step} OF {setup.stepCount}</div>
      <h3>{step.title}</h3>
      <p>{step.body}</p>
      {#if setup.note}
        <p class="note">{setup.note}</p>
      {/if}
      {#if setup.assignmentMessage}
        <p class="note">{setup.assignmentMessage}</p>
      {/if}
      <div class="dots">
        {#each { length: setup.stepCount } as _, n}
          <span class="dot" class:done={n + 1 < step.step} class:current={n + 1 === step.step}></span>
        {/each}
      </div>
    </div>
    <div class="actions">
      {#if step.showTest}
        <Button data-setting-target="setup-test" disabled={!setup.canTest} onclick={setup.runTest}>
          {setup.testing ? "Testing…" : "Test"}
        </Button>
      {/if}
      {#if step.step > 1}
        <Button onclick={setup.back}>Back</Button>
      {/if}
      {#if step.showSkip}
        <Button onclick={setup.skip}>Skip</Button>
      {/if}
      {#if step.showAgent}
        <Button onclick={finishWithAgent}>Finish with the agent</Button>
      {/if}
      {#if step.isLast}
        <Button variant="primary" onclick={setup.finish}>Finish</Button>
      {:else}
        <Button variant="primary" disabled={!step.canNext} onclick={setup.next}>Next</Button>
      {/if}
      <ActionIconButton icon={X} label="Hide setup" onclick={setup.finish} />
    </div>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    gap: var(--space-6);
    align-items: flex-start;
    margin: var(--space-4) var(--space-6) 0;
    padding: var(--space-5) var(--space-6);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-lg);
    background: var(--accent-bg-soft);
  }

  .step {
    color: var(--accent);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    letter-spacing: 0.3px;
  }

  h3 {
    margin: var(--space-1) 0;
    font-size: var(--font-size-lg);
  }

  p {
    max-width: 640px;
    margin: 0;
    color: var(--text-secondary);
  }

  .note {
    margin-top: var(--space-2);
    color: var(--text-primary);
  }

  .dots {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }

  .dot {
    width: 22px;
    height: 3px;
    border-radius: 2px;
    background: var(--border-strong);
  }

  .dot.done {
    background: var(--success);
  }

  .dot.current {
    background: var(--accent);
  }

  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-3);
    margin-left: auto;
  }
</style>
