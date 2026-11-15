<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import ContextSection from "./ContextSection.svelte";
  import AttachMenu from "./AttachMenu.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import SkillEditable from "$lib/components/shared/widgets/SkillEditable.svelte";
  import ModelSelector from "$lib/components/shared/widgets/ModelSelector.svelte";
  import ComposerDictationBar from "$lib/components/shared/widgets/ComposerDictationBar.svelte";
  import { useComposerDictation } from "$lib/components/shared/widgets/useComposerDictation.svelte";
  import ToolChip from "./ToolChip.svelte";
  import AttachmentRow from "./components/AttachmentRow.svelte";
  import { prefetchCapabilities, getCachedCapabilities } from "$lib/stores/capabilities.svelte";
  import { ArrowUp, RefreshCw, Square, CopyCheck, Globe, Mic } from "lucide-svelte";
  import type { ComponentType, SvelteComponent } from "svelte";
  import type { IconProps } from "lucide-svelte";

  type LucideIcon = ComponentType<SvelteComponent<IconProps>>;
  import { handleEditablePaste } from "$lib/utils/paste";
  import { formatTokenCount } from "$lib/utils/contextWindow";
  import { focusConversationInput, setConversationInputFocus } from "$lib/utils/conversationFocus";
  import type { createConversationStore } from "$lib/stores/conversation.svelte";
  import type { ConversationImage } from "$lib/types/conversation";
  import type { ModelConfig } from "$lib/types";
  import { openImagePreview, openTextPreview } from "$lib/services/windowPreviews";
  import { readClipboardForEditable } from "$lib/services/clipboardPaste";
  import { isMac } from "$lib/utils/platform";
  import { useInputSync } from "./drivers/useInputSync.svelte";
  import { useMcpTools } from "./drivers/useMcpTools.svelte";
  import { useTextAttachmentBridge } from "./drivers/useTextAttachmentBridge.svelte";

  let {
    store,
    models = [],
    contextVisible,
    contextDisabled,
    contextInitialCollapsed = false,
    contextWindowSize = 0,
    onSendAndCopy,
    onContextAutoShow,
    onCloseContext,
    onToggleContext,
    defaultModelId = null,
  }: {
    store: ReturnType<typeof createConversationStore>;
    models?: ModelConfig[];
    contextVisible: boolean;
    contextDisabled: boolean;
    contextInitialCollapsed?: boolean;
    contextWindowSize?: number;
    onSendAndCopy: () => void;
    onContextAutoShow: () => void;
    onCloseContext: () => void;
    onToggleContext: () => void;
    defaultModelId?: string | null;
  } = $props();

  let skillEditable: ReturnType<typeof SkillEditable> | undefined = $state();

  const sync = useInputSync({ store, getSkillEditable: () => skillEditable });
  const mcp = useMcpTools();
  const dictation = useComposerDictation({ owner: "chat", insert: insertDictation });
  const attachmentBridge = useTextAttachmentBridge({
    getAttachments: () => sync.localTextAttachments,
    setAttachments: (next) => {
      sync.localTextAttachments = next;
    },
  });

  let unlistenMcp: (() => void) | undefined;
  let unlistenAttachmentBridge: (() => void) | undefined;
  let shiftHeld = $state(false);

  const activeModel = $derived.by(() => {
    const activeModelId = store.modelId ?? defaultModelId;
    if (!activeModelId) return null;
    return models.find((m) => m.id === activeModelId) ?? null;
  });

  $effect(() => { prefetchCapabilities(activeModel?.id ?? null); });
  const activeModelCapabilities = $derived(getCachedCapabilities(activeModel?.id ?? null));

  const builtinWebSearchAvailable = $derived(activeModel?.provider === "openai");
  const mcpWebSearchAvailable = $derived(!!mcp.webSearchQualifiedId && !!activeModel);
  const webSearchAvailable = $derived(builtinWebSearchAvailable || mcpWebSearchAvailable);
  const bothWebSearchAvailable = $derived(builtinWebSearchAvailable && mcpWebSearchAvailable);

  const availableTools = $derived.by(() => {
    const tools: { id: string; label: string; icon: LucideIcon; active: boolean }[] = [];
    if (bothWebSearchAvailable) {
      tools.push({ id: "web_search", label: "Web Search (Built-in)", icon: Globe, active: store.selectedTools.includes("web_search") });
      tools.push({ id: mcp.webSearchQualifiedId!, label: "Web Search (MCP)", icon: Globe, active: store.selectedTools.includes(mcp.webSearchQualifiedId!) });
    } else if (webSearchAvailable) {
      const toolId = mcpWebSearchAvailable && mcp.webSearchQualifiedId ? mcp.webSearchQualifiedId : "web_search";
      tools.push({ id: toolId, label: "Web Search", icon: Globe, active: store.selectedTools.includes(toolId) });
    }
    return tools;
  });

  function handleToggleTool(id: string, enabled: boolean) {
    if (enabled && bothWebSearchAvailable) {
      const otherId = id === "web_search" ? mcp.webSearchQualifiedId : "web_search";
      if (otherId && store.selectedTools.includes(otherId)) {
        store.toggleTool(otherId, false);
      }
    }
    store.toggleTool(id, enabled);
    focusConversationInput();
  }

  function handleSelectModel(modelId: string) {
    store.updateModelId(modelId);
    focusConversationInput();
  }

  function handleSelectReasoning(effort: string | null) {
    store.updateReasoningEffort(effort);
    focusConversationInput();
  }

  function handleRemoveTextAttachment(idx: number) {
    sync.localTextAttachments = sync.localTextAttachments.filter((_, i) => i !== idx);
    focusConversationInput();
  }

  function handleRemoveImage(idx: number) {
    sync.localImages = sync.localImages.filter((_, i) => i !== idx);
    focusConversationInput();
  }

  function handleDismissTool(id: string) {
    store.toggleTool(id, false);
    focusConversationInput();
  }

  const contextFillRatio = $derived.by(() => {
    if (contextWindowSize <= 0 || store.totalTokens <= 0) return 0;
    return Math.min(store.totalTokens / contextWindowSize, 1);
  });

  const contextFillTone = $derived.by<"ok" | "warn" | "danger">(() => {
    if (contextFillRatio >= 0.9) return "danger";
    if (contextFillRatio >= 0.7) return "warn";
    return "ok";
  });

  const contextFillTitle = $derived.by(() => {
    if (contextWindowSize <= 0) return "Context tokens";
    const percent = Math.round(contextFillRatio * 1000) / 10;
    return `${formatTokenCount(store.totalTokens)} of ${formatTokenCount(contextWindowSize)} tokens (${percent}%)`;
  });

  function onKeyDown(e: KeyboardEvent) { shiftHeld = e.shiftKey; }
  function onKeyUp(e: KeyboardEvent) { shiftHeld = e.shiftKey; }

  onMount(async () => {
    setConversationInputFocus(() => skillEditable?.focus());
    skillEditable?.focus();
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    unlistenMcp = await mcp.init();
    unlistenAttachmentBridge = await attachmentBridge.init();
  });

  onDestroy(() => {
    setConversationInputFocus(null);
    unlistenAttachmentBridge?.();
    unlistenMcp?.();
    window.removeEventListener("keydown", onKeyDown);
    window.removeEventListener("keyup", onKeyUp);
  });

  async function insertDictation(tabId: string | undefined, start: number, end: number, text: string) {
    if (!tabId) return;
    const caret = store.insertInputText(tabId, start, end, text);
    if (caret === null || tabId !== store.activeTabId) return;
    await tick();
    requestAnimationFrame(() => {
      skillEditable?.focus();
      skillEditable?.restoreCursor(caret);
    });
  }

  function startDictation() {
    const caret = skillEditable?.getSelection() ?? { start: sync.localText.length, end: sync.localText.length };
    dictation.start(store.activeTabId, caret);
  }

  function sendOrRegenerate() {
    const abortNodeId = store.abortRegenerateNodeId;
    if (abortNodeId) {
      store.editAndRegenerate(abortNodeId, sync.localText.trim());
      return;
    }
    if (store.isRegenerateMode) {
      const path = store.tree.current_path;
      if (path.length > 0) store.regenerate(path[path.length - 1]);
    } else if (store.canSend) {
      store.sendMessage();
    }
  }

  async function handleKeydown(e: KeyboardEvent) {
    if (isMac && e.key.toLowerCase() === "v" && e.shiftKey && e.metaKey) {
      e.preventDefault();
      const result = await readClipboardForEditable();
      if (result?.kind === "text") {
        document.execCommand("insertText", false, result.text);
      } else if (result?.kind === "image") {
        sync.localImages = [...sync.localImages, { data: result.data, media_type: result.mediaType }];
      }
      return;
    }

    if (e.key === "Enter" && dictation.active) {
      e.preventDefault();
      return;
    }

    if (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      sendOrRegenerate();
      return;
    }

    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      if (store.canSend) onSendAndCopy();
      return;
    }
  }

  async function handlePaste(e: ClipboardEvent) {
    await handleEditablePaste(e, {
      skipTextAttachment: shiftHeld,
      onTextAttachment: (text) => { sync.localTextAttachments = [...sync.localTextAttachments, text]; },
      onImage: (image) => { sync.localImages = [...sync.localImages, image]; },
    });
  }

  function handleInputAreaClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest("button, .attach-menu, .model-selector, .autocomplete-dropdown, .context-inline, .attachment-row")) return;
    skillEditable?.focus();
  }

  function handleOpenImage(image: ConversationImage) {
    openImagePreview(image.data, image.media_type);
  }
</script>

<div class="input-area" onclick={handleInputAreaClick}>
  {#if contextVisible}
    <ContextSection {store} {contextDisabled} initialCollapsed={contextInitialCollapsed} onHasContent={onContextAutoShow} onClose={onCloseContext} />
  {/if}

  <div class="input-field">
    <AttachmentRow
      textAttachments={sync.localTextAttachments}
      images={sync.localImages}
      variant="small"
      onRemoveText={handleRemoveTextAttachment}
      onRemoveImage={handleRemoveImage}
      onOpenText={openTextPreview}
      onOpenImage={handleOpenImage}
    />
    <SkillEditable
      bind:this={skillEditable}
      bind:text={sync.localText}
      placeholder="Type a message… (use /skill-name for skills)"
      editableClass="input-editable"
      readonly={dictation.active}
      onkeydown={handleKeydown}
      onpaste={handlePaste}
    />
  </div>

  <div class="button-bar">
    {#if dictation.active}
      <ComposerDictationBar {dictation} />
    {:else}
      <div class="bar-left">
        <AttachMenu
          onSelectContext={onToggleContext}
          {contextDisabled}
          {availableTools}
          onToggleTool={handleToggleTool}
        />
        {#each availableTools.filter(t => t.active) as tool (tool.id)}
          <ToolChip label={tool.label} icon={tool.icon} ondismiss={() => handleDismissTool(tool.id)} />
        {/each}
      </div>

      <div class="bar-right">
        {#if store.totalTokens > 0}
          <span class="token-count" data-tone={contextFillTone} title={contextFillTitle}>
            ~{formatTokenCount(store.totalTokens)}{#if contextWindowSize > 0}&nbsp;/ {formatTokenCount(contextWindowSize)}{/if}
          </span>
        {/if}
        {#if models.length > 0}
          <ModelSelector
            {models}
            selectedModelId={store.modelId}
            reasoningEffort={store.reasoningEffort}
            capabilities={activeModelCapabilities}
            onModelSelect={handleSelectModel}
            onReasoningSelect={handleSelectReasoning}
          />
        {/if}

        <ActionIconButton
          icon={Mic}
          onclick={startDictation}
          disabled={!!dictation.blockedTitle}
          label="Dictate"
          title={dictation.blockedTitle ?? undefined}
        />

        <ActionIconButton
          icon={CopyCheck}
          onclick={onSendAndCopy}
          disabled={!store.canSend || store.isExecuting}
          label="Send and copy"
          title="Send and copy (Ctrl+Enter)"
        />

        {#if store.isExecuting}
          <ActionIconButton icon={Square} variant="stop" label="Stop" onclick={() => store.stopExecution()} />
        {:else if store.abortRegenerateNodeId || store.isRegenerateMode}
          <ActionIconButton icon={RefreshCw} variant="primary" label="Regenerate" title="Regenerate (Enter)" onclick={sendOrRegenerate} />
        {:else}
          <ActionIconButton
            icon={ArrowUp}
            variant="primary"
            label="Send"
            title="Send (Enter)"
            disabled={!store.canSend}
            onclick={sendOrRegenerate}
          />
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .input-area {
    flex-shrink: 0;
    position: relative;
    z-index: var(--z-sticky);
    margin: -8px var(--space-8) var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-xl);
    background: var(--surface-input-bar);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    overflow: visible;
  }

  :global([data-platform="linux"]) .input-area {
    background: var(--surface-input-bar-solid);
    backdrop-filter: none;
    -webkit-backdrop-filter: none;
  }

  .input-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-4) var(--space-0);
  }

  .input-field :global(.input-editable) {
    font-size: var(--font-size-base);
    max-height: 35vh;
    overflow-y: auto;
    padding: var(--space-2) var(--space-1);
  }

  .button-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    position: relative;
    overflow: visible;
  }

  .bar-left {
    flex-shrink: 0;
    display: flex;
    gap: var(--space-3);
    align-items: center;
    position: relative;
    overflow: visible;
  }

  .token-count {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    user-select: none;
    white-space: nowrap;
    margin-right: var(--space-1);
    cursor: default;
    transition: color var(--motion-default) var(--ease-default);
  }

  .token-count[data-tone="warn"] {
    color: var(--warning);
  }

  .token-count[data-tone="danger"] {
    color: var(--danger);
  }


  .bar-right {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
</style>
