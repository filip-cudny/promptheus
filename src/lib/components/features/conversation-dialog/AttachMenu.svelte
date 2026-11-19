<script lang="ts">
  import { Plus, FileText } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import type { ComponentType, SvelteComponent } from "svelte";
  import type { IconProps } from "lucide-svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import FloatingPanel from "$lib/components/shared/ui/FloatingPanel.svelte";
  import MenuList from "$lib/components/shared/ui/MenuList.svelte";
  import MenuItem from "$lib/components/shared/ui/MenuItem.svelte";
  import ScanSquareIcon from "$lib/icons/ScanSquareIcon.svelte";
  import TextSelectIcon from "$lib/icons/TextSelectIcon.svelte";
  import { captureAvailable } from "$lib/services/capture";

  type LucideIcon = ComponentType<SvelteComponent<IconProps>>;

  let {
    onSelectContext,
    onScreenArea,
    onSelectedText,
    contextDisabled = false,
    availableTools = [],
    onToggleTool,
  }: {
    onSelectContext: () => void;
    onScreenArea: () => void;
    onSelectedText: () => void;
    contextDisabled?: boolean;
    availableTools?: { id: string; label: string; icon?: LucideIcon; active: boolean }[];
    onToggleTool?: (toolId: string, enabled: boolean) => void;
  } = $props();

  let menuOpen = $state(false);
  let captureEnabled = $state(false);
  let availabilityRequested = false;
  let buttonEl: HTMLSpanElement | undefined = $state();

  function toggleMenu() {
    menuOpen = !menuOpen;
    if (!menuOpen || availabilityRequested) return;
    availabilityRequested = true;
    captureAvailable().then((available) => (captureEnabled = available));
  }

  function handleCaptureClick(handler: () => void) {
    menuOpen = false;
    handler();
  }

  function handleContextClick() {
    if (contextDisabled) return;
    menuOpen = false;
    onSelectContext();
  }
</script>

<div class="attach-menu">
  <span class="attach-btn-wrap" bind:this={buttonEl}>
    <ActionIconButton
      icon={Plus}
      onclick={toggleMenu}
      label="Add attachment"
    />
  </span>

  <FloatingPanel
    visible={menuOpen}
    anchorEl={buttonEl}
    position="above"
    fitContent
    onclose={() => (menuOpen = false)}
  >
    <MenuList role="menu">
      {#if captureEnabled}
        <MenuItem label="Screen area" onclick={() => handleCaptureClick(onScreenArea)}>
          {#snippet icon()}
            <ScanSquareIcon size={ICON_SIZE.md} />
          {/snippet}
        </MenuItem>
        <MenuItem label="Selected text" onclick={() => handleCaptureClick(onSelectedText)}>
          {#snippet icon()}
            <TextSelectIcon size={ICON_SIZE.md} />
          {/snippet}
        </MenuItem>
      {/if}
      <MenuItem label="Context" disabled={contextDisabled} onclick={handleContextClick}>
        {#snippet icon()}
          <FileText size={ICON_SIZE.md} />
        {/snippet}
      </MenuItem>
      {#if availableTools.length > 0}
        <div class="menu-list-separator"></div>
        {#each availableTools as tool (tool.id)}
          <MenuItem
            label={tool.label}
            active={tool.active}
            onclick={() => { onToggleTool?.(tool.id, !tool.active); menuOpen = false; }}
          >
            {#snippet icon()}
              {#if tool.icon}
                {@const Icon = tool.icon}
                <Icon size={ICON_SIZE.md} />
              {/if}
            {/snippet}
          </MenuItem>
        {/each}
      {/if}
    </MenuList>
  </FloatingPanel>
</div>

<style>
  .attach-menu {
    flex-shrink: 0;
    align-self: flex-end;
  }

  .attach-btn-wrap {
    display: flex;
  }
</style>
