<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { updateHistoryEntryTitle, deleteHistoryEntry } from "$lib/services/history";
  import { focusConversationInput } from "$lib/utils/conversationFocus";
  import type { createConversationStore } from "$lib/stores/conversation.svelte";
  import TabSidebarHeader from "./components/TabSidebarHeader.svelte";
  import SidebarItemRow from "./components/SidebarItemRow.svelte";
  import SidebarMoreMenu from "./components/SidebarMoreMenu.svelte";
  import InlineRenameInput from "./components/InlineRenameInput.svelte";
  import ConfirmDialog from "$lib/components/shared/ui/ConfirmDialog.svelte";
  import { useConversationsList } from "./drivers/useConversationsList.svelte";
  import { useSidebarMutex } from "./drivers/useSidebarMutex.svelte";
  import {
    buildSidebarItems,
    itemId,
    itemTitle,
    itemIcon,
    itemTimestamp,
    type SidebarItem,
  } from "./sidebarItems";

  const PAGE_SIZE = 30;

  let {
    store,
    open,
    onClose,
  }: {
    store: ReturnType<typeof createConversationStore>;
    open: boolean;
    onClose: () => void;
  } = $props();

  const list = useConversationsList(PAGE_SIZE);
  const mutex = useSidebarMutex<SidebarItem>();

  let tabListEl: HTMLDivElement | undefined = $state();
  let unlistenList: (() => void) | undefined;

  const items = $derived(buildSidebarItems(list.conversations));

  function isItemActive(item: SidebarItem): boolean {
    return openTabOf(item)?.tab_id === store.activeTabId;
  }

  function openTabOf(item: SidebarItem) {
    return store.tabs.find((t) => t.history_entry_id === item.entry.id);
  }

  function handleItemClick(item: SidebarItem) {
    if (mutex.editingId) return;
    store.restoreFromHistory(item.entry.id);
    onClose();
    focusConversationInput();
  }

  function toggleMenu(e: MouseEvent, item: SidebarItem) {
    e.stopPropagation();
    mutex.openMenu(itemId(item));
  }

  function commitRename(item: SidebarItem, newValue: string) {
    const trimmed = newValue.trim();
    if (trimmed && trimmed !== itemTitle(item)) {
      const tab = openTabOf(item);
      if (tab) store.renameTab(tab.tab_id, trimmed);
      else updateHistoryEntryTitle(item.entry.id, trimmed).catch(() => {});
    }
    mutex.clearEditing();
  }

  async function confirmDelete() {
    const item = mutex.confirmDelete;
    if (!item) return;
    mutex.clearConfirm();
    const tab = openTabOf(item);
    if (tab) store.closeTab(tab.tab_id);
    await deleteHistoryEntry(item.entry.id).catch(() => {});
  }

  function handleScroll() {
    mutex.closeMenu();
    if (!tabListEl || !list.hasMore || list.loading) return;
    const { scrollTop, scrollHeight, clientHeight } = tabListEl;
    if (scrollHeight - scrollTop - clientHeight < 100) list.loadMore();
  }

  onMount(async () => {
    unlistenList = await list.init();
  });

  onDestroy(() => {
    unlistenList?.();
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={onClose}></div>
{/if}

<aside class="sidebar" class:open>
  <TabSidebarHeader onClose={onClose} />

  <div class="tab-list" bind:this={tabListEl} onscroll={handleScroll}>
    {#each items as item (itemId(item))}
      {@const id = itemId(item)}
      <SidebarItemRow
        active={isItemActive(item)}
        icon={itemIcon(item)}
        timestamp={itemTimestamp(item)}
        onClick={() => handleItemClick(item)}
      >
        {#snippet body()}
          {#if mutex.editingId === id}
            <InlineRenameInput
              initial={itemTitle(item)}
              onCommit={(v) => commitRename(item, v)}
              onCancel={() => mutex.clearEditing()}
            />
          {:else}
            <span class="tab-name" title={itemTitle(item)}>{itemTitle(item)}</span>
          {/if}
        {/snippet}
        {#snippet trailing()}
          <SidebarMoreMenu
            open={mutex.menuOpenId === id}
            onToggle={(e) => toggleMenu(e, item)}
            onClose={() => mutex.closeMenu()}
            onRename={() => mutex.startRename(id)}
            onDelete={() => mutex.startDelete(item)}
          />
        {/snippet}
      </SidebarItemRow>
    {/each}
  </div>
</aside>

<ConfirmDialog
  open={mutex.confirmDelete !== null}
  message="Delete this conversation?"
  confirmLabel="Delete conversation"
  onConfirm={confirmDelete}
  onCancel={() => mutex.clearConfirm()}
/>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 199;
  }

  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: 240px;
    background: var(--surface-side-drawer);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    border-right: 1px solid var(--border-strong);
    z-index: var(--z-drawer);
    display: flex;
    flex-direction: column;
    transform: translateX(-100%);
    transition: transform var(--motion-slow) var(--ease-default);
    will-change: transform;
  }

  .sidebar.open {
    transform: translateX(0);
  }

  :global([data-platform="linux"]) .sidebar {
    background: var(--surface-side-drawer-solid);
    backdrop-filter: none;
    -webkit-backdrop-filter: none;
  }

  .tab-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
