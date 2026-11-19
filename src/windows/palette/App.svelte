<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { initLogo } from "$lib/stores/logo.svelte";
  import Palette from "$lib/components/features/palette/Palette.svelte";
  import { usePaletteIpc } from "$lib/components/features/palette/drivers/usePaletteIpc.svelte";

  let inputEl: HTMLInputElement | undefined = $state();

  const ipc = usePaletteIpc({
    onShown: () => inputEl?.focus(),
  });

  onMount(() => {
    initLogo();
    ipc.init();
  });
  onDestroy(() => ipc.destroy());

  $effect(() => {
    document.documentElement.dataset.corners = ipc.rounded ? "rounded" : "square";
  });
</script>

<Palette
  visible={ipc.visible}
  activeId={ipc.activeId}
  webviewProviders={ipc.webviewProviders}
  openWindows={ipc.openWindows}
  currentTitle={ipc.currentTitle}
  bind:inputRef={inputEl}
  onDismiss={(selectedId) => ipc.dismiss(selectedId)}
  onReloadActive={() => ipc.reloadActive()}
  onFocusWindow={(label) => ipc.focusWindow(label)}
/>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    margin: var(--space-0);
  }

  :global(html[data-corners="rounded"] body) {
    height: 100vh;
    clip-path: inset(0 round 0 0 10px 10px);
  }
</style>
