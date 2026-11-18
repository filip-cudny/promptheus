<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { createConversationStore } from "$lib/stores/conversation.svelte";
  import { hasContext } from "$lib/services/context";
  import { getSettings } from "$lib/services/settings";
  import { getContextWindowSize } from "$lib/utils/contextWindow";
  import { getSkillsStore } from "$lib/stores/skills.svelte";
  import ChatPalette from "$lib/components/features/conversation-dialog/ChatPalette.svelte";
  import ChatTopButtons from "$lib/components/features/conversation-dialog/ChatTopButtons.svelte";
  import ContextSection from "$lib/components/features/conversation-dialog/ContextSection.svelte";
  import ConversationArea from "$lib/components/features/conversation-dialog/ConversationArea.svelte";
  import InputArea from "$lib/components/features/conversation-dialog/InputArea.svelte";
  import TabSidebar from "$lib/components/features/conversation-dialog/TabSidebar.svelte";
  import { useConversationDialogIpc } from "$lib/components/features/conversation-dialog/drivers/useConversationDialogIpc.svelte";
  import { getChatCorners, reloadActiveInHost } from "$lib/services/shellToolbar";
  import { getTabs, reportNativeTabs, type NativeTabReport } from "$lib/services/tabs";
  import { SHORTCUTS, createHostShortcutHandler, matches } from "$lib/shortcuts";
  import { focusConversationInput } from "$lib/utils/conversationFocus";
  import { attachEdgeResizeCursor } from "$lib/utils/edgeResizeCursor";
  import { initTheme } from "$lib/stores/theme.svelte";
  import { initLogo } from "$lib/stores/logo.svelte";
  import type { ModelConfig } from "$lib/types";
  import type { InvocationSource } from "$lib/types/history";

  interface DialogInitParams {
    skill_id: string;
    skill_name: string;
    skill_model: string | null;
    history_entry_id: string | null;
    initial_input: string | null;
    auto_send_input: boolean;
    new_chat: boolean;
    invocation_source: InvocationSource | null;
    copy_token: string | null;
  }

  const skillsStore = getSkillsStore();
  const HOST_LABEL = getCurrentWindow().label;
  const SELF_TARGET = getCurrentWebview().label;

  const store = createConversationStore("", "");

  let rounded = $state(false);
  let unlistenCorners: UnlistenFn | null = null;

  $effect(() => {
    document.documentElement.classList.toggle("rounded", rounded);
  });

  let sidebarOpen = $state(false);
  let contextVisible = $state(false);
  let contextDisabled = $state(false);
  let contextInitialCollapsed = $state(false);
  let models = $state<ModelConfig[]>([]);
  let defaultModelId = $state<string | null>(null);

  let chatPaletteOpen = $state(false);
  let prevChatPaletteOpen = false;

  $effect(() => {
    const isOpen = chatPaletteOpen;
    if (prevChatPaletteOpen && !isOpen) {
      focusConversationInput();
    }
    prevChatPaletteOpen = isOpen;
  });

  let contextWindowSize = $derived.by(() => {
    const activeModelId = store.modelId ?? defaultModelId;
    const activeModel = models.find((m) => m.id === activeModelId);
    if (!activeModel) return 0;
    return getContextWindowSize(activeModel.model, activeModel.context_window_size);
  });

  let nativeViewVisible = $state(true);
  let reportingReady = $state(false);
  let activeChangedCount = $state(0);
  let lastReportKey = "";

  $effect(() => {
    if (!reportingReady) return;
    void activeChangedCount;
    const tabs: NativeTabReport[] = store.tabs.map((t) => ({
      tab_id: t.tab_id,
      title: t.tab_name ?? "New chat",
      streaming: t.is_executing,
      listed: t.shown || !store.isTabClean(t),
    }));
    const activeTabId = store.activeTabId;
    const key = JSON.stringify([tabs, activeTabId]);
    if (key === lastReportKey) return;
    lastReportKey = key;
    reportNativeTabs(tabs, activeTabId).catch((err) => {
      console.error("report_native_tabs failed", err);
    });
  });

  $effect(() => {
    if (reportingReady && nativeViewVisible) store.markActiveTabShown();
  });

  const handleHostShortcut = createHostShortcutHandler(HOST_LABEL);

  async function handleGlobalKeydown(e: KeyboardEvent) {
    if (matches(e, SHORTCUTS.reloadActive)) {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (chatPaletteOpen) chatPaletteOpen = false;
      await reloadActiveProvider();
      return;
    }

    if (chatPaletteOpen) return;

    if (handleHostShortcut(e)) return;

    if (matches(e, SHORTCUTS.openChatPalette)) {
      e.preventDefault();
      e.stopPropagation();
      chatPaletteOpen = true;
      return;
    }

    if (e.key === "Escape" && store.isExecuting) {
      e.preventDefault();
      store.abortExecution();
    }
  }

  function handleWindowFocus() {
    requestAnimationFrame(() => {
      if (chatPaletteOpen) return;
      const sel = window.getSelection();
      if (sel && sel.toString().length > 0) return;
      const active = document.activeElement;
      if (active && active !== document.body && active !== document.documentElement) return;
      focusConversationInput();
    });
  }

  function handleChatPaletteNewChat() {
    chatPaletteOpen = false;
    store.addTab();
  }

  async function handleChatPaletteOpenConversation(entryId: string) {
    chatPaletteOpen = false;
    await store.restoreFromHistory(entryId);
  }

  async function reloadActiveProvider() {
    try {
      await reloadActiveInHost();
    } catch (err) {
      console.error("reload_active_in_host failed", err);
    }
  }

  async function autoShowContextIfNeeded() {
    if (contextDisabled || contextVisible) return;
    if (await hasContext()) {
      contextInitialCollapsed = true;
      contextVisible = true;
    }
  }

  async function handleVoiceInput(
    skillId: string,
    text: string,
    skillModel: string | null,
    autoSend: boolean,
    invocationSource: InvocationSource | null,
    copyToken: string | null,
  ) {
    const currentTab = store.tabs.find((t) => t.tab_id === store.activeTabId);
    if (currentTab && currentTab.tree.current_path.length > 0) {
      store.addTab();
    }
    store.setActiveTabInvocationSource(invocationSource ?? "alternative_execute");
    if (skillModel) store.setActiveTabModelId(skillModel);
    const inputText = skillId ? `/${skillId} ${text}` : text;
    store.updateInputText(inputText);
    if (!autoSend) return;
    if (!contextDisabled) {
      try {
        await store.loadGlobalContextIntoActiveTab();
      } catch (err) {
        console.error("load global context failed", err);
      }
    }
    const sending = store.sendMessage();
    await autoShowContextIfNeeded();
    const outcome = await sending;
    if (!copyToken || !outcome.success || !outcome.tabId || !outcome.nodeId) return;
    const tab = store.tabs.find((t) => t.tab_id === outcome.tabId);
    const node = tab?.tree.nodes.get(outcome.nodeId);
    if (!node || node.cancelled) return;
    try {
      const copied = await invoke<boolean>("copy_chat_answer_if_unchanged", {
        token: copyToken,
        text: outcome.result,
      });
      if (copied) store.markAutoCopied(outcome.nodeId);
    } catch (err) {
      console.error("conditional answer copy failed", err);
    }
  }

  async function applyInitParams(p: DialogInitParams) {
    if (p.new_chat) {
      store.addTab();
    } else if (p.history_entry_id) {
      await store.restoreFromHistory(p.history_entry_id);
    } else if (p.initial_input) {
      await handleVoiceInput(p.skill_id, p.initial_input, p.skill_model, p.auto_send_input, p.invocation_source, p.copy_token);
    } else if (p.skill_id) {
      store.openForSkill(p.skill_id, p.skill_name, p.skill_model);
    }
  }

  async function loadModelInfo() {
    try {
      const settings = await getSettings();
      models = settings.models.filter((m) => m.type === "text");
      defaultModelId = settings.surfaces.chat.generation.model_id ?? null;
    } catch {}
  }

  const ipc = useConversationDialogIpc({
    selfTarget: SELF_TARGET,
    onRestoreHistory: (entryId) => {
      store.restoreFromHistory(entryId);
    },
    onVoiceInput: (text, skillId, skillModel, autoSend, source, copyToken) =>
      handleVoiceInput(skillId, text, skillModel, autoSend, source, copyToken),
    onOpenForSkill: (skillId, skillName, skillModel) =>
      store.openForSkill(skillId, skillName, skillModel),
    onNewConversation: () => store.addTab(),
    onActivateTab: (tabId) => store.switchTab(tabId),
    onCloseTab: (tabId, nextTabId) => store.closeTabWithSuccessor(tabId, nextTabId),
    onActiveProviderChanged: (providerId) => {
      nativeViewVisible = providerId === null;
      lastReportKey = "";
      activeChangedCount++;
    },
    onActiveProviderCleared: () => focusConversationInput(),
    onMenuReloadActive: () => {
      if (chatPaletteOpen) chatPaletteOpen = false;
      reloadActiveProvider();
    },
    onContextChanged: () => autoShowContextIfNeeded(),
  });

  let detachEdgeResizeCursor: (() => void) | null = null;

  onMount(async () => {
    initTheme();
    initLogo();
    window.addEventListener("keydown", handleGlobalKeydown);
    window.addEventListener("focus", handleWindowFocus);
    detachEdgeResizeCursor = attachEdgeResizeCursor("content");
    unlistenCorners = await listen<{ rounded: boolean }>(
      "shell:corners",
      (ev) => {
        rounded = ev.payload.rounded;
      },
      { target: SELF_TARGET },
    );
    try {
      rounded = await getChatCorners();
    } catch (err) {
      console.error("get_chat_corners failed", err);
    }
    skillsStore.init();
    await store.initFromSettings();
    loadModelInfo();

    const reconnected = await store.tryReconnect();

    const initParams = await invoke<DialogInitParams | null>("get_dialog_init_params");
    if (!reconnected && initParams) {
      await applyInitParams(initParams);
    }

    await ipc.init();
    await autoShowContextIfNeeded();
    try {
      const snapshot = await getTabs();
      const active = snapshot.tabs.find((t) => t.id === snapshot.active_id);
      nativeViewVisible = snapshot.active_id === null || active?.kind === "native";
    } catch (err) {
      console.error("get_tabs failed", err);
    }
    reportingReady = true;
  });

  function handleContextAutoShow() {
    if (!contextDisabled) {
      contextInitialCollapsed = !contextVisible;
      contextVisible = true;
    }
  }

  function toggleContext() {
    if (contextVisible) closeContext();
    else contextVisible = true;
  }

  function closeContext() {
    contextVisible = false;
    store.updateContextText("");
    store.updateContextImages([]);
  }

  async function handleSendAndCopy() {
    const currentWindow = getCurrentWindow();
    const { success, result } = await store.sendMessage();
    if (success && result) {
      await navigator.clipboard.writeText(result);
      await currentWindow.close();
    }
  }

  onDestroy(() => {
    window.removeEventListener("keydown", handleGlobalKeydown);
    window.removeEventListener("focus", handleWindowFocus);
    if (detachEdgeResizeCursor) {
      detachEdgeResizeCursor();
      detachEdgeResizeCursor = null;
    }
    unlistenCorners?.();
    unlistenCorners = null;
    ipc.destroy();
    store.destroy();
  });
</script>

<div class="dialog-shell" class:sidebar-open={sidebarOpen}>
  <ChatTopButtons
    {sidebarOpen}
    onToggleSidebar={() => (sidebarOpen = !sidebarOpen)}
    onNewChat={() => store.addTab()}
  />
  {#if contextVisible}
    <ContextSection
      {store}
      {contextDisabled}
      initialCollapsed={contextInitialCollapsed}
      onHasContent={handleContextAutoShow}
      onClose={closeContext}
    />
  {/if}
  <ConversationArea {store} belowContextBar={contextVisible} />
  <InputArea
    {store}
    {models}
    {contextDisabled}
    {contextWindowSize}
    {defaultModelId}
    onSendAndCopy={handleSendAndCopy}
    onToggleContext={toggleContext}
  />
  <TabSidebar {store} open={sidebarOpen} onClose={() => (sidebarOpen = false)} />
</div>

<ChatPalette
  open={chatPaletteOpen}
  onClose={() => (chatPaletteOpen = false)}
  onNewChat={handleChatPaletteNewChat}
  onOpenConversation={handleChatPaletteOpenConversation}
/>

<style>
  :global(html),
  :global(body) {
    background: var(--surface-base);
  }

  :global(html.rounded),
  :global(html.rounded body) {
    background: transparent;
  }

  :global(html.rounded body) {
    height: 100vh;
    margin: 0;
    clip-path: inset(0 round 0 0 10px 10px);
  }

  :global(html.rounded body::after) {
    content: "";
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-top: none;
    border-radius: 0 0 10px 10px;
    pointer-events: none;
    z-index: 2147483647;
  }

  /* --chat-top-buttons-end restates the geometry of ChatTopButtons (left offset 6px, 28px buttons, gap) and the
     240px TabSidebar width, so the context bar's content starts after the buttons. It is wrong once either
     component changes those values; then define them once here as custom properties read by all three. */
  .dialog-shell {
    --chat-top-buttons-end: calc(6px + 28px + var(--space-2) + 28px);
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--surface-base);
    color: var(--text-primary);
    font-family: var(--font-sans);
    font-size: var(--font-size-base);
    overflow: hidden;
    position: relative;
  }

  .dialog-shell.sidebar-open {
    --chat-top-buttons-end: calc(240px + 6px + var(--space-2) + 28px);
  }
</style>
