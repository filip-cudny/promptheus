<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import { MessageSquare } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import type { MenuItem } from "$lib/types/menu";
  import {
    extractContextItems,
    extractLastInteractionData,
    groupBySection,
  } from "./itemExtractors";
  import ContextSection from "./ContextSection.svelte";
  import LastInteractionSection from "./LastInteractionSection.svelte";
  import FloatingPanel from "$lib/components/shared/ui/FloatingPanel.svelte";
  import { error as logError } from "@tauri-apps/plugin-log";
  import { focusOrOpenChat, openConversationDialog } from "$lib/services/conversationDialog";
  import { newTab } from "$lib/services/tabs";
  import { openImagePreview } from "$lib/services/windowPreviews";
  import {
    clearContext,
    getContextText,
    appendContextFromClipboard,
    removeContextItem,
  } from "$lib/services/context";
  import { captureAvailable as fetchCaptureAvailable } from "$lib/services/capture";
  import { openContextEditor } from "$lib/services/contextEditor";
  import {
    copyHistoryEntryPart,
    copyLastInteraction,
    getHistoryEntry,
    getRecentQuickActions,
    type LastInteractionChip,
    type RecentQuickAction,
  } from "$lib/services/history";
  import { retryTranscription } from "$lib/services/speech";
  import { openHistoryDialog } from "$lib/services/historyDialog";
  import { openSettingsWindow } from "$lib/services/settingsDialog";
  import ProviderMenuList from "$lib/components/shared/widgets/ProviderMenuList.svelte";
  import { isExecuting, getExecutingSkillId } from "$lib/stores/execution.svelte";
  import * as menu from "$lib/stores/contextMenu.svelte";
  import MenuShell from "./MenuShell.svelte";
  import MenuEmptyState from "./components/MenuEmptyState.svelte";
  import MenuSeparator from "./components/MenuSeparator.svelte";
  import MenuItemRow from "./components/MenuItemRow.svelte";
  import SettingsToggleRow from "./components/SettingsToggleRow.svelte";
  import FooterHint from "./components/FooterHint.svelte";
  import ProviderMenuFooterHint from "./components/ProviderMenuFooterHint.svelte";
  import SkillActionMenu from "./components/SkillActionMenu.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import { useFloatingPanelMutex } from "./drivers/useFloatingPanelMutex.svelte";
  import { useMenuKeyboard } from "./drivers/useMenuKeyboard.svelte";
  import { useMenuBlurClose } from "./drivers/useMenuBlurClose.svelte";
  import { useMenuPositioning } from "./drivers/useMenuPositioning.svelte";
  import { fetchSkillMetadata, buildSkillMetaEntries } from "$lib/services/skillMetadata.svelte";
  import { createWebviewProvidersStore } from "$lib/stores/webviewProviders.svelte";
  import { useModelsMenuData } from "$lib/stores/modelsMenuData.svelte";

  const RECENT_LIMIT = 100;

  const panels = useFloatingPanelMutex({
    onSettingsClose: menu.resumeClose,
    onActionMenuClose: menu.resumeClose,
  });

  const positioning = useMenuPositioning({
    getMenuEl: () => menuEl,
    isVisible: () => menu.isVisible(),
    getWorkArea: menu.getWorkArea,
  });

  const keyboard = useMenuKeyboard({
    isVisible: () => menuVisible,
    hasOpenPanel: () => panels.hasAny,
    closePanels: panels.closeAll,
    closeMenu: menu.closeMenu,
    moveSelection: menu.moveSelection,
    executeSelected: activateSelected,
    handleNumberInput: menu.handleNumberInput,
  });

  const blurClose = useMenuBlurClose({
    isInBlurGrace: menu.isInBlurGrace,
    isSuppressed: menu.isSuppressed,
    isCapturing: () => menu.getCapture() !== null,
    resumeClose: menu.resumeClose,
    closeMenu: menu.closeMenu,
  });

  const providersStore = createWebviewProvidersStore();
  const models = useModelsMenuData(() => menu.getItems());

  function isRecordingThisSkill(item: MenuItem): boolean {
    if (!menu.isRecording()) return false;
    const data = item.data as { skill_id: string } | null;
    return data?.skill_id === menu.getRecordingSkillId();
  }

  function isExecutingSkill(item: MenuItem): boolean {
    if (!isExecuting()) return false;
    const data = item.data as { skill_id: string } | null;
    return data?.skill_id === getExecutingSkillId();
  }

  let captureAvailable = $state(false);
  let menuEl: HTMLDivElement | undefined = $state();
  let chatRowEl: HTMLElement | undefined = $state();
  let settingsToggleEl: HTMLElement | undefined = $state();
  let unlistenProviders: (() => void) | undefined;

  function activateSelected(shiftPressed: boolean) {
    const item = menu.getItems()[menu.getSelectedIndex()];
    if (item?.item_type === "chat") return openChat();
    if (item?.item_type === "settings_section") return panels.openSettings(settingsToggleEl);
    return menu.executeSelected(shiftPressed);
  }

  async function openChat() {
    if (menu.isRecording()) return;
    await menu.closeMenu(false);
    await focusOrOpenChat();
  }

  async function pickChatProvider(providerId: string) {
    panels.closeChatProviders();
    await menu.closeMenu(false);
    try {
      await newTab(providerId);
    } catch (err) {
      console.error("newTab failed", err);
    }
  }

  function openActionMenu(e: MouseEvent, item: MenuItem, executingThis: boolean) {
    e.preventDefault();
    e.stopPropagation();
    if (executingThis) return;
    panels.openActionMenu(item.id, e.currentTarget as HTMLElement);
    if (panels.actionMenuId === item.id) {
      const skillId = (item.data as { skill_id?: string } | null)?.skill_id;
      if (skillId) void fetchSkillMetadata(skillId);
    }
  }

  let recentListOpen = $state(false);
  let previousRecentListOpen = false;
  let handledOpenTrigger = menu.getOpenTrigger();

  $effect(() => {
    const openTrigger = menu.getOpenTrigger();
    if (menuVisible && menuItems.length > 0) {
      untrack(() => panels.closeAll());
      const isNewOpen = openTrigger !== handledOpenTrigger;
      handledOpenTrigger = openTrigger;
      if (!isNewOpen && untrack(() => recentListOpen)) {
        void positioning.resizeKeepingTop();
      } else {
        void positioning.triggerReposition();
      }
    }
  });

  $effect(() => {
    const open = recentListOpen;
    if (open === previousRecentListOpen) return;
    previousRecentListOpen = open;
    untrack(() => {
      if (menu.isVisible()) void positioning.resizeKeepingTop();
    });
  });

  let sections = $derived(groupBySection(menu.getItems()));

  let menuVisible = $derived(menu.isVisible());
  $effect(() => {
    if (!menuVisible) {
      panels.closeAll();
      recentListOpen = false;
    }
  });

  let menuItems = $derived(menu.getItems());
  let allSkillItems = $derived(menu.getAllSkillItems());
  let currentSelectedIndex = $derived(menu.getSelectedIndex());

  $effect(() => {
    if (menuVisible && menuEl) {
      const _idx = currentSelectedIndex;
      const selected = menuEl.querySelector(".menu-item-row.selected");
      selected?.scrollIntoView({ block: "nearest" });
    }
  });

  function handleItemClick(index: number, e: MouseEvent) {
    menu.executeItem(index, e.shiftKey);
  }

  async function handleContextCopyAll() {
    const text = await getContextText();
    if (text) await navigator.clipboard.writeText(text);
  }

  async function handleOpenImagePreview(data: string, mediaType: string) {
    menu.suppressClose();
    await openImagePreview(data, mediaType);
  }

  async function handleCopyChip(chip: LastInteractionChip) {
    await copyLastInteraction(chip);
  }

  async function handleOpenHistory() {
    await menu.closeMenu(false);
    await openHistoryDialog();
  }

  function handleFetchRecent() {
    return getRecentQuickActions(RECENT_LIMIT);
  }

  async function handleOpenRecentInChat(row: RecentQuickAction) {
    await menu.closeMenu(false);
    try {
      if (row.entry_type === "text") {
        await openConversationDialog(row.skill_id ?? "", row.skill_name ?? "", row.id);
        return;
      }
      const entry = await getHistoryEntry(row.id);
      if (entry?.output_preview) {
        await openConversationDialog("", "", undefined, entry.output_preview, false, false, null, "chat");
      }
    } catch (err) {
      logError(`[last-interaction] open in chat failed: ${err}`);
    }
  }

  async function handleRetryTranscription(entryId: string) {
    await retryTranscription(entryId);
    await menu.refreshItems();
  }

  onMount(async () => {
    await menu.init();
    captureAvailable = await fetchCaptureAvailable();
    unlistenProviders = await providersStore.init();
    await blurClose.init();
  });

  onDestroy(() => {
    blurClose.destroy();
    menu.destroy();
    unlistenProviders?.();
  });
</script>

<svelte:window onkeydown={keyboard.onkeydown} onkeyup={keyboard.onkeyup} />

<MenuShell bind:ref={menuEl} onmousemove={positioning.enableHover}>
  {#if menuItems.length === 0}
    <MenuEmptyState />
  {:else}
    {#each sections as section, sectionIdx}
      {#if sectionIdx > 0}
        <MenuSeparator />
      {/if}
      {#if section.sectionId === "chat"}
        {@const chatDisabled = menu.isRecording()}
        {@const chatIndex = section.items[0]?.globalIndex}
        <MenuItemRow
          label="Chat"
          selected={chatIndex === currentSelectedIndex}
          disabled={chatDisabled}
          hoverHighlight
          bind:rowEl={chatRowEl}
          onhover={() => { if (positioning.hoverEnabled) menu.setSelectedIndex(-1); }}
          oncontextmenu={(e) => {
            e.preventDefault();
            if (chatDisabled) return;
            e.stopPropagation();
            panels.openChatProviders();
          }}
          onclick={openChat}
        >
          {#snippet icon()}<MessageSquare size={ICON_SIZE.md} />{/snippet}
        </MenuItemRow>
        <FloatingPanel
          visible={panels.chatProvidersOpen}
          anchorEl={chatRowEl}
          flush
          onclose={panels.closeChatProviders}
        >
          <ProviderMenuList
            providers={providersStore.providerEntries}
            expand
            onSelect={(id) => { void pickChatProvider(id); }}
          />
          <ProviderMenuFooterHint />
        </FloatingPanel>
      {/if}
      {#if section.sectionId === "skills"}
        <div data-section="skills-anchor"></div>
      {/if}
      {#if section.sectionId === "settings"}
        <SettingsToggleRow
          expanded={panels.settingsOpen}
          bind:anchorEl={settingsToggleEl}
          onhover={() => { if (positioning.hoverEnabled) menu.setSelectedIndex(-1); }}
          onclick={() => panels.openSettings(settingsToggleEl)}
          onOpenSettings={async () => {
            panels.closeSettings();
            await menu.closeMenu(false);
            await openSettingsWindow();
          }}
        />
        <FloatingPanel
          visible={panels.settingsOpen}
          anchorEl={panels.settingsAnchor}
          onclose={panels.closeSettings}
        >
          <SettingsPanel
            models={models.modelsData?.models ?? []}
            sttModels={models.modelsData?.stt_models ?? []}
            defaultModelId={models.defaultModelId}
            reasoningEffort={models.reasoningEffort}
            sttModelId={models.sttModelId}
            quickActionCapabilities={models.quickActionCapabilities}
            preventDismiss={{ suppress: menu.suppressClose, resume: menu.resumeClose }}
            onModelSelect={models.setDefaultModel}
            onReasoningSelect={models.setReasoningEffort}
            onSttSelect={models.setSttModel}
            onHover={() => { if (positioning.hoverEnabled) menu.setSelectedIndex(-1); }}
          />
        </FloatingPanel>
      {/if}
      {#each section.sectionId === "chat" || section.sectionId === "settings" ? [] : section.items as { item, globalIndex }}
        {@const contextItems = extractContextItems(item)}
        {@const lastInteractionData = extractLastInteractionData(item)}
        {#if contextItems}
          <ContextSection
            items={contextItems}
            onPaste={appendContextFromClipboard}
            onScreenshot={menu.captureScreenshot}
            onSelectText={menu.toggleSelection}
            selecting={menu.getCapture() === "selection"}
            {captureAvailable}
            onOpenEditor={openContextEditor}
            onCopyAll={handleContextCopyAll}
            onClear={clearContext}
            onRemoveItem={removeContextItem}
            onOpenImagePreview={handleOpenImagePreview}
          />
        {:else if lastInteractionData !== null}
          <LastInteractionSection
            data={lastInteractionData}
            bind:listOpen={recentListOpen}
            onCopyChip={handleCopyChip}
            onOpenHistory={handleOpenHistory}
            onRetryTranscription={handleRetryTranscription}
            onFetchRecent={handleFetchRecent}
            onCopyPart={copyHistoryEntryPart}
            onOpenInChat={handleOpenRecentInChat}
          />
        {:else}
          {@const executingThis = item.item_type === "skill" && isExecutingSkill(item)}
          {@const recordingThis = item.item_type === "skill" && isRecordingThisSkill(item)}
          {@const micDisabled = executingThis || (!item.enabled && !recordingThis)}
          {@const skillIndex = item.item_type === "skill" ? allSkillItems.indexOf(item) : -1}
          {@const promptNumber = skillIndex >= 0 ? skillIndex + 1 : null}
          <MenuItemRow
            selected={globalIndex === currentSelectedIndex}
            disabled={!item.enabled}
            executing={executingThis}
            recording={recordingThis}
            iconName={item.icon}
            promptNumber={promptNumber}
            label={item.label}
            onhover={() => { if (positioning.hoverEnabled && item.enabled) menu.setSelectedIndex(globalIndex); }}
            oncontextmenu={item.item_type === "skill"
              ? (e) => openActionMenu(e, item, executingThis)
              : undefined}
            onclick={(e) => handleItemClick(globalIndex, e)}
          />
          {#if item.item_type === "skill"}
            {@const skillId = (item.data as { skill_id?: string } | null)?.skill_id ?? ""}
            {@const metaEntries = buildSkillMetaEntries(skillId, models.modelNames)}
            <FloatingPanel
              visible={panels.actionMenuId === item.id}
              anchorEl={panels.actionMenuAnchor}
              flush
              onclose={panels.closeActionMenu}
            >
              <SkillActionMenu
                recording={recordingThis}
                {micDisabled}
                runDisabled={executingThis || recordingThis || !item.enabled}
                tooltip={item.tooltip}
                {metaEntries}
                onOpenInDialog={() => {
                  panels.closeActionMenu();
                  void menu.openDialogForItem(globalIndex);
                }}
                onRunInDialog={() => {
                  panels.closeActionMenu();
                  void menu.executeItem(globalIndex, true);
                }}
                onAlternativeExecute={() => {
                  panels.closeActionMenu();
                  void menu.startAlternativeExecution(globalIndex);
                }}
              />
            </FloatingPanel>
          {/if}
        {/if}
      {/each}
    {/each}
    {#if menuItems.some((i) => i.item_type === "skill")}
      <FooterHint shiftHeld={keyboard.shiftHeld} />
    {/if}
  {/if}
</MenuShell>
