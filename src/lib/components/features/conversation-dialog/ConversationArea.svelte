<script lang="ts">
  import { Loader2 } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import type { createConversationStore } from "$lib/stores/conversation.svelte";
  import type { MessagePair } from "$lib/types/conversation";
  import UserBubble from "./UserBubble.svelte";
  import AssistantBubble from "./AssistantBubble.svelte";
  import { getSkillsStore } from "$lib/stores/skills.svelte";
  import { useAutoScroll } from "./drivers/useAutoScroll.svelte";

  const skillsStore = getSkillsStore();

  function classifyToken(token: string, _finished: boolean): string | null {
    return skillsStore.nameSet.has(token.slice(1)) ? "skill-badge" : null;
  }

  const PAGE_SIZE = 20;

  let {
    store,
    belowContextBar = false,
  }: {
    store: ReturnType<typeof createConversationStore>;
    belowContextBar?: boolean;
  } = $props();

  let container: HTMLDivElement | undefined = $state();
  let scrolled = $state(false);
  let viewportHeight = $state(0);

  const allPairs = $derived(store.messagePairs);

  const scroll = useAutoScroll({
    getContainer: () => container,
    pageSize: PAGE_SIZE,
    totalCount: () => allPairs.length,
    trackChange: () => [store.messagePairs, store.streamedContent],
    resetKey: () => store.activeTabId,
    pinRequest: () => store.pinRequest,
    resetRequest: () => store.scrollResetRequest,
  });

  const visiblePairs = $derived(
    scroll.hasMore ? allPairs.slice(allPairs.length - scroll.visibleCount) : allPairs,
  );

  function isLastAssistant(pair: MessagePair): boolean {
    return allPairs[allPairs.length - 1] === pair;
  }

  function handleScroll(e: UIEvent) {
    const el = e.currentTarget as HTMLDivElement;
    scrolled = el.scrollTop > 4;
    scroll.onScroll();
  }
</script>

<svelte:window onkeydown={scroll.onKeydown} />

<div
  class="conversation-area"
  role="presentation"
  class:scrolled
  class:below-context-bar={belowContextBar}
  bind:this={container}
  bind:clientHeight={viewportHeight}
  style:--viewport-height="{viewportHeight}px"
  onscroll={handleScroll}
  onwheel={scroll.onWheel}
  onpointerdown={scroll.onPointerdown}
>
  <div class="messages-column">
    {#if allPairs.length === 0}
      <div class="empty-state">Send a message to start the conversation.</div>
    {:else}
      {#if scroll.hasMore}
        <div class="load-more-zone">
          <Loader2 size={ICON_SIZE.sm} class="spin" />
          <span>Scroll up for older messages</span>
        </div>
      {/if}
      {#each visiblePairs as pair (pair.user.node_id)}
        <div class="pair" class:spacer={scroll.spacer && isLastAssistant(pair)}>
          <UserBubble
            node={pair.user}
            showDelete={false}
            {classifyToken}
            onContentChange={(content) => store.updateUserNodeContent(pair.user.node_id, content)}
            onDelete={() => {}}
            onRegenerate={() => { if (pair.assistant) store.regenerate(pair.assistant.node_id); }}
            onRemoveTextAttachment={(index) => store.removeNodeTextAttachment(pair.user.node_id, index)}
            onRemoveImage={(index) => store.removeNodeImage(pair.user.node_id, index)}
            onAddTextAttachment={(text) => store.addNodeTextAttachment(pair.user.node_id, text)}
            onAddImage={(data, mediaType) => store.addNodeImage(pair.user.node_id, data, mediaType)}
          />
          {#if pair.assistant}
            {@const assistant = pair.assistant}
            {@const streaming = store.isStreaming && isLastAssistant(pair)}
            <AssistantBubble
              node={assistant}
              displayContent={streaming ? store.streamedContent : assistant.content}
              showDelete={false}
              isStreaming={streaming}
              autoCopied={store.autoCopiedNodeId === assistant.node_id}
              thinkingContent={streaming ? store.streamedThinking : (assistant.thinking ?? "")}
              isThinkingActive={streaming && store.isThinking}
              branchInfo={store.getBranchInfo(assistant.node_id)}
              activeToolCalls={streaming ? store.activeToolCalls : []}
              onRegenerate={() => store.regenerate(assistant.node_id)}
              onBranchPrev={() => store.switchBranch(assistant.node_id, -1)}
              onBranchNext={() => store.switchBranch(assistant.node_id, 1)}
              onContentChange={(content) => store.updateNodeContent(assistant.node_id, content)}
              onDelete={() => {}}
            />
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .conversation-area {
    --top-padding: 40px;
    flex: 1;
    overflow-y: auto;
    padding: var(--top-padding) var(--space-8) var(--space-8);
    display: flex;
    flex-direction: column;
    position: relative;
    transition: box-shadow var(--motion-default) var(--ease-default),
      border-color var(--motion-default) var(--ease-default);
    border-top: 1px solid transparent;
  }

  .conversation-area.scrolled {
    border-top-color: var(--border-faint);
    box-shadow: inset 0 8px 12px -10px rgba(0, 0, 0, 0.55);
  }

  .conversation-area.below-context-bar {
    --top-padding: var(--space-8);
    border-top: none;
  }

  .messages-column {
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    flex: 1;
  }

  .pair {
    display: contents;
  }

  .pair.spacer {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-height: calc(var(--viewport-height) - var(--top-padding) - var(--space-8));
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-disabled);
    font-size: var(--font-size-lg);
  }

  .load-more-zone {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-4);
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
  }

  .load-more-zone :global(.spin) {
    animation: spin 1s linear infinite;
  }
</style>
