<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import MarkdownRenderer from "$lib/components/shared/ui/MarkdownRenderer.svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import { getSettingsAgent } from "$lib/stores/settingsAgent.svelte";
  import ToolCallGroup from "$lib/components/features/conversation-dialog/tool-call/ToolCallGroup.svelte";
  import ThinkingBlock from "$lib/components/shared/ui/ThinkingBlock.svelte";
  import SourceList from "$lib/components/shared/ui/SourceList.svelte";
  import ProcessingIndicator from "$lib/components/features/conversation-dialog/components/ProcessingIndicator.svelte";
  import SecretField from "../SecretField.svelte";
  import ProposalCard from "./ProposalCard.svelte";
  import AgentEmptyState from "./AgentEmptyState.svelte";
  import { parseSettingLink, showSettingLinks } from "$lib/utils/settingLinks";
  import { showsProcessing, thinkingText } from "$lib/utils/agentReply";
  import { parseContentSegments, toRenderBlocks } from "$lib/utils/toolCallMarkers";
  import { createScrollFollower } from "$lib/utils/scrollFollow";
  import type { ToolCall } from "$lib/types/ai";
  import type { AgentMessage } from "$lib/stores/settingsAgent.svelte";

  let { docked }: { docked: boolean } = $props();

  const agent = getSettingsAgent();
  const navigation = getSettingsNavigation();

  let container: HTMLDivElement | undefined = $state();
  let column: HTMLDivElement | undefined = $state();
  let viewportHeight = $state(0);
  let spacer = $state(false);
  let handledPin = agent.pinRequest;

  const follower = createScrollFollower({
    getContainer: () => container,
    getPinnedTarget: () =>
      container?.querySelector<HTMLElement>(".pair:last-child > :first-child"),
    onChange: (state) => {
      spacer = state.spacer;
    },
  });

  const pairs = $derived(
    agent.messages.reduce<AgentMessage[][]>((acc, message) => {
      if (message.role === "user" || acc.length === 0) acc.push([message]);
      else acc[acc.length - 1].push(message);
      return acc;
    }, []),
  );
  const lastMessageId = $derived(agent.messages[agent.messages.length - 1]?.id);

  $effect(() => {
    if (agent.messages.length === 0) follower.dispatch({ type: "reset" });
  });

  $effect(() => {
    const request = agent.pinRequest;
    if (request === handledPin) return;
    handledPin = request;
    follower.dispatch({ type: "pin" });
  });

  $effect(() => {
    if (!column) return;
    const observer = new ResizeObserver(() => follower.dispatch({ type: "contentChange" }));
    observer.observe(column);
    return () => observer.disconnect();
  });

  async function open(url: string) {
    const key = parseSettingLink(url);
    if (!key) {
      openUrl(url);
      return;
    }
    if (!docked) await agent.hidePanel("show-link");
    navigation.navigateTo(key);
  }

  function groupCalls(ids: string[], calls: ToolCall[]): ToolCall[] {
    return ids
      .map((id) => calls.find((call) => call.tool_call_id === id))
      .filter((call): call is ToolCall => call != null);
  }
</script>

<svelte:window onkeydown={follower.onKeydown} />

{#snippet tail()}
  {#if agent.proposal}
    {#key agent.proposal.proposal_id}
      <ProposalCard proposal={agent.proposal} />
    {/key}
  {/if}
  {#if agent.error}
    <div class="error">{agent.error}</div>
  {/if}
{/snippet}

<div
  class="messages"
  role="presentation"
  class:empty={agent.messages.length === 0}
  bind:this={container}
  bind:clientHeight={viewportHeight}
  style:--viewport-height="{viewportHeight}px"
  onscroll={follower.onScroll}
  onwheel={follower.onWheel}
  onpointerdown={follower.onPointerdown}
>
  <div class="column" bind:this={column}>
    {#if pairs.length === 0}
      <AgentEmptyState />
      {@render tail()}
    {/if}
    {#each pairs as pair, p (pair[0].id)}
      <div class="pair" class:spacer={spacer && p === pairs.length - 1}>
        {#each pair as message (message.id)}
          {#if message.role === "user"}
            <div class="user">{message.text}</div>
          {:else}
            {@const reasoning = thinkingText(message)}
            {@const last = message.id === lastMessageId}
            <div class="assistant">
              {#if reasoning || message.isThinking}
                <ThinkingBlock
                  thinkingContent={reasoning}
                  isThinkingActive={message.isThinking}
                  isStreaming={agent.running && last}
                />
              {/if}
              {#each toRenderBlocks(parseContentSegments(message.text)) as block}
                {#if block.kind === "text"}
                  <MarkdownRenderer
                    content={showSettingLinks(block.text)}
                    isStreaming={agent.running && last}
                    onopen={open}
                  />
                {:else}
                  {@const calls = groupCalls(block.toolCallIds, message.toolCalls)}
                  {#if calls.length > 0}
                    <ToolCallGroup toolCalls={calls} isStreaming={agent.running && last} />
                  {/if}
                {/if}
              {/each}
              <SourceList sources={message.sources} onopen={open} />
              {#each message.secretViews as name, view (view)}
                <SecretField {name} readonly />
              {/each}
              {#if last && showsProcessing(message, agent.running)}
                <ProcessingIndicator />
              {/if}
            </div>
          {/if}
        {/each}
        {#if p === pairs.length - 1}
          {@render tail()}
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-6);
  }

  .messages.empty {
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  .column {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
  }

  .pair {
    display: contents;
  }

  .pair.spacer {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    min-height: calc(var(--viewport-height) - 2 * var(--space-6));
  }

  .user {
    align-self: flex-end;
    max-width: 85%;
    padding: 7px 11px;
    background: var(--accent-bg-soft);
    border-radius: 10px 10px 2px 10px;
    white-space: pre-wrap;
  }

  .assistant {
    line-height: 1.55;
  }

  .error {
    color: var(--danger, #e5484d);
    font-size: var(--font-size-sm);
  }

  .assistant :global(a[href^="setting:"]) {
    padding: 0 var(--space-3);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-md);
    background: var(--surface-overlay-faint);
    color: var(--text-primary);
    text-decoration: none;
    font-size: var(--font-size-xs);
  }
</style>
