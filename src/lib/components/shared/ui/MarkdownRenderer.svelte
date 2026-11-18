<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import morphdom from "morphdom";
  import { renderMarkdown, extractCodeBlocks } from "$lib/utils/markdown";
  import { icon, icons } from "$lib/utils/icons";
  import { COPY_FEEDBACK_MS } from "$lib/utils/copyFeedback.svelte";

  let {
    content,
    isStreaming = false,
    onopen,
    onsavesvg,
  }: {
    content: string;
    isStreaming: boolean;
    onopen?: (url: string) => void;
    onsavesvg?: (svg: string) => void;
  } = $props();

  const DRAIN_FRACTION = 0.15;
  const RENDER_EVERY_N_FRAMES = 2;

  let displayedLength = $state(untrack(() => (isStreaming ? 0 : content.length)));
  let animFrameId: number | null = null;
  let wasStreaming = untrack(() => isStreaming);
  let frameCount = 0;
  let lastRenderedHtml = "";

  let displayedText = $derived(
    isStreaming ? content : content.slice(0, displayedLength),
  );
  let isFullyRevealed = $derived(
    !isStreaming && displayedLength >= content.length,
  );

  let renderedHtml = $derived(renderMarkdown(displayedText));

  let codeBlocks = $derived(
    isFullyRevealed ? extractCodeBlocks(displayedText) : [],
  );

  let markdownContainer: HTMLDivElement | undefined = $state();
  const morphWrapper = document.createElement("div");

  let mermaidModule: typeof import("mermaid") | null = null;

  async function ensureMermaid() {
    if (mermaidModule) return mermaidModule;
    mermaidModule = await import("mermaid");
    mermaidModule.default.initialize({
      startOnLoad: false,
      theme: "dark",
      fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
    });
    return mermaidModule;
  }

  async function renderMermaidBlocks() {
    if (!markdownContainer) return;
    const blocks = markdownContainer.querySelectorAll<HTMLElement>(".mermaid-block:not(.mermaid-rendered)");
    if (blocks.length === 0) return;

    const mod = await ensureMermaid();
    for (const block of blocks) {
      const encoded = block.dataset.mermaidSource;
      if (!encoded) continue;
      const source = decodeURIComponent(atob(encoded));
      const id = `mermaid-${block.dataset.mermaidIndex}-${Date.now()}`;
      try {
        const { svg } = await mod.default.render(id, source);
        block.innerHTML = svg;
        block.classList.add("mermaid-rendered");
      } catch {
        block.classList.add("mermaid-error");
      }
    }
  }

  $effect(() => {
    if (!markdownContainer) return;
    const html = renderedHtml;
    if (html === lastRenderedHtml) return;
    lastRenderedHtml = html;
    morphWrapper.innerHTML = html;
    morphdom(markdownContainer, morphWrapper, { childrenOnly: true });
    renderMermaidBlocks();
  });

  function animate() {
    const remaining = content.length - displayedLength;

    if (remaining <= 0) {
      animFrameId = null;
      return;
    }

    frameCount++;
    if (frameCount % RENDER_EVERY_N_FRAMES === 0) {
      displayedLength += Math.max(1, Math.ceil(remaining * DRAIN_FRACTION));
    }

    animFrameId = requestAnimationFrame(animate);
  }

  function startAnimation() {
    if (animFrameId !== null) return;
    animFrameId = requestAnimationFrame(animate);
  }

  $effect(() => {
    if (isStreaming) {
      wasStreaming = true;
      displayedLength = content.length;
    } else if (content.length > displayedLength) {
      if (wasStreaming) {
        startAnimation();
      } else {
        displayedLength = content.length;
      }
    }
  });

  onDestroy(() => {
    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }
  });

  function handleMermaidToggle(toggleBtn: HTMLElement) {
    const idx = toggleBtn.dataset.mermaidToggle;
    if (!idx || !markdownContainer) return;

    const block = markdownContainer.querySelector<HTMLElement>(`.mermaid-block[data-mermaid-index="${idx}"]`);
    if (!block) return;

    const codeIcon = icon(icons.Code);
    const eyeIcon = icon(icons.Eye);

    const isRaw = block.classList.contains("mermaid-raw");

    if (isRaw) {
      const savedSvg = block.dataset.mermaidSvg;
      if (savedSvg) {
        block.innerHTML = decodeURIComponent(atob(savedSvg));
        block.classList.remove("mermaid-raw");
        toggleBtn.innerHTML = codeIcon;
      }
    } else if (block.classList.contains("mermaid-rendered")) {
      block.dataset.mermaidSvg = btoa(encodeURIComponent(block.innerHTML));
      const encoded = block.dataset.mermaidSource;
      if (!encoded) return;
      const source = decodeURIComponent(atob(encoded));
      block.textContent = source;
      block.classList.add("mermaid-raw");
      toggleBtn.innerHTML = eyeIcon;
    }
  }

  function getMermaidSvg(idx: string): string | null {
    if (!markdownContainer) return null;
    const block = markdownContainer.querySelector<HTMLElement>(`.mermaid-block[data-mermaid-index="${idx}"]`);
    if (!block) return null;

    if (block.classList.contains("mermaid-raw")) {
      const saved = block.dataset.mermaidSvg;
      return saved ? decodeURIComponent(atob(saved)) : null;
    }
    if (block.classList.contains("mermaid-rendered")) {
      return block.innerHTML;
    }
    return null;
  }

  const feedbackTimers = new WeakMap<HTMLElement, ReturnType<typeof setTimeout>>();

  function flashCheck(button: HTMLElement, restoreIcon: string) {
    clearTimeout(feedbackTimers.get(button));
    button.innerHTML = icon(icons.Check);
    feedbackTimers.set(
      button,
      setTimeout(() => {
        button.innerHTML = restoreIcon;
      }, COPY_FEEDBACK_MS),
    );
  }

  function closeAllMermaidMenus() {
    if (!markdownContainer) return;
    for (const panel of markdownContainer.querySelectorAll<HTMLElement>(".mermaid-menu.open")) {
      panel.classList.remove("open");
    }
  }

  function handleClick(e: MouseEvent) {
    const target = e.target as HTMLElement;

    const anchor = target.closest("a") as HTMLAnchorElement | null;
    if (anchor?.href) {
      e.preventDefault();
      onopen?.(anchor.href);
      return;
    }

    const toggleBtn = target.closest("[data-mermaid-toggle]") as HTMLElement | null;
    if (toggleBtn) {
      closeAllMermaidMenus();
      handleMermaidToggle(toggleBtn);
      return;
    }

    const menuBtn = target.closest("[data-mermaid-menu]") as HTMLElement | null;
    if (menuBtn && markdownContainer) {
      const idx = menuBtn.dataset.mermaidMenu;
      const panel = markdownContainer.querySelector<HTMLElement>(`.mermaid-menu[data-mermaid-menu-panel="${idx}"]`);
      if (panel) {
        const wasOpen = panel.classList.contains("open");
        closeAllMermaidMenus();
        if (!wasOpen) panel.classList.add("open");
      }
      return;
    }

    const copySvgBtn = target.closest("[data-mermaid-copy-svg]") as HTMLElement | null;
    if (copySvgBtn) {
      const idx = copySvgBtn.dataset.mermaidCopySvg!;
      const svg = getMermaidSvg(idx);
      if (svg) {
        navigator.clipboard.writeText(svg);
        const dotsBtn = markdownContainer?.querySelector<HTMLElement>(`[data-mermaid-menu="${idx}"]`);
        if (dotsBtn) flashCheck(dotsBtn, icon(icons.EllipsisVertical));
      }
      closeAllMermaidMenus();
      return;
    }

    const saveSvgBtn = target.closest("[data-mermaid-save-svg]") as HTMLElement | null;
    if (saveSvgBtn) {
      const idx = saveSvgBtn.dataset.mermaidSaveSvg!;
      const svg = getMermaidSvg(idx);
      if (svg) {
        onsavesvg?.(svg);
      }
      closeAllMermaidMenus();
      return;
    }

    closeAllMermaidMenus();

    const copyBtn = target.closest("[data-copy-index]") as HTMLElement | null;
    if (!copyBtn) return;

    const index = parseInt(copyBtn.dataset.copyIndex ?? "", 10);
    if (Number.isNaN(index) || index >= codeBlocks.length) return;

    navigator.clipboard.writeText(codeBlocks[index]);
    flashCheck(copyBtn, icon(icons.Copy));
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="markdown-renderer" bind:this={markdownContainer} onclick={handleClick}></div>

<style>
  .markdown-renderer {
    font-size: var(--font-size-lg);
    line-height: var(--line-height-relaxed);
    color: var(--text-primary);
    word-wrap: break-word;
    overflow-wrap: break-word;
    user-select: text;
    -webkit-user-select: text;
  }

  .markdown-renderer :global(h1),
  .markdown-renderer :global(h2),
  .markdown-renderer :global(h3) {
    margin: var(--space-6) var(--space-0) var(--space-3);
    color: var(--syntax-fg);
  }

  .markdown-renderer :global(h1) {
    font-size: 1.4em;
  }

  .markdown-renderer :global(h2) {
    font-size: 1.2em;
  }

  .markdown-renderer :global(h3) {
    font-size: 1.05em;
  }

  .markdown-renderer :global(p) {
    margin: var(--space-3) var(--space-0);
  }

  .markdown-renderer :global(a) {
    color: var(--syntax-link);
  }

  .markdown-renderer :global(ul),
  .markdown-renderer :global(ol) {
    padding-left: var(--space-10);
    margin: var(--space-3) var(--space-0);
  }

  .markdown-renderer :global(blockquote) {
    border-left: 3px solid var(--border-strong);
    padding-left: 12px;
    margin: var(--space-4) var(--space-0);
    color: var(--text-secondary);
  }

  .markdown-renderer :global(.table-wrapper) {
    overflow-x: auto;
    max-width: 100%;
    border: 1px solid var(--border-default);
    border-radius: 6px;
    margin: var(--space-3) var(--space-0);
  }

  .markdown-renderer :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-base);
  }

  .markdown-renderer :global(th) {
    text-align: left;
    font-weight: var(--font-weight-semibold);
    color: var(--text-secondary);
    background: var(--surface-overlay-faint);
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-default);
  }

  .markdown-renderer :global(td) {
    padding: 6px 10px;
    border-top: 1px solid var(--border-faint);
    vertical-align: top;
  }

  .markdown-renderer :global(th:first-child),
  .markdown-renderer :global(td:first-child) {
    white-space: nowrap;
  }

  .markdown-renderer :global(code) {
    background: var(--surface-overlay);
    padding: 2px 5px;
    border-radius: var(--radius-sm);
    font-size: 0.9em;
    font-family: var(--font-mono);
  }

  .markdown-renderer :global(.code-block) {
    position: relative;
    margin: var(--space-4) var(--space-0);
    border-radius: var(--radius-lg);
    overflow: hidden;
    border: 1px solid var(--border-faint);
    background: var(--surface-sunken);
  }

  .markdown-renderer :global(.code-block:hover) {
    border-color: var(--border-default);
  }

  .markdown-renderer :global(.code-block-header) {
    position: absolute;
    top: var(--space-2);
    right: var(--space-3);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    z-index: 1;
    pointer-events: none;
  }

  .markdown-renderer :global(.code-block-header > *) {
    pointer-events: auto;
  }

  .markdown-renderer :global(.code-lang) {
    color: var(--text-muted);
    background: var(--surface-overlay);
    padding: 1px var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-family: var(--font-mono);
    letter-spacing: var(--tracking-label);
    text-transform: uppercase;
    line-height: 1.5;
  }

  .markdown-renderer :global(.code-lang:empty) {
    display: none;
  }

  .markdown-renderer :global(.copy-btn) {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: var(--surface-overlay);
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--motion-fast) var(--ease-default),
      background var(--motion-fast) var(--ease-default),
      color var(--motion-fast) var(--ease-default);
  }

  .markdown-renderer :global(.code-block:hover .copy-btn) {
    opacity: 1;
  }

  .markdown-renderer :global(.copy-btn:hover) {
    background: var(--surface-overlay-strong);
    color: var(--text-primary);
  }

  .markdown-renderer :global(pre) {
    margin: var(--space-0);
    padding: var(--space-6) var(--space-6) var(--space-6) var(--space-6);
    background: var(--surface-sunken);
    overflow-x: auto;
  }

  .markdown-renderer :global(pre code) {
    background: none;
    padding: var(--space-0);
    border-radius: 0;
    font-size: var(--font-size-base);
  }

  .markdown-renderer :global(.mermaid-wrapper) {
    position: relative;
    margin: var(--space-4) var(--space-0);
    border-radius: var(--radius-lg);
    overflow: hidden;
    border: 1px solid var(--border-faint);
    background: var(--surface-sunken);
  }

  .markdown-renderer :global(.mermaid-actions) {
    display: flex;
    gap: var(--space-1);
  }

  .markdown-renderer :global(.mermaid-block) {
    padding: var(--space-8);
    background: var(--surface-sunken);
    overflow-x: auto;
    display: flex;
    justify-content: center;
  }

  .markdown-renderer :global(.mermaid-block:not(.mermaid-rendered)),
  .markdown-renderer :global(.mermaid-block.mermaid-raw) {
    white-space: pre;
    font-family: var(--font-mono);
    font-size: var(--font-size-base);
    color: var(--text-muted);
    justify-content: flex-start;
  }

  .markdown-renderer :global(.mermaid-block.mermaid-error) {
    color: var(--syntax-error);
  }

  .markdown-renderer :global(.mermaid-block svg) {
    max-width: 100%;
    height: auto;
  }

  .markdown-renderer :global(.mermaid-menu-anchor) {
    position: relative;
  }

  .markdown-renderer :global(.mermaid-menu) {
    display: none;
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 4px;
    min-width: 140px;
    background: var(--surface-elevated);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-lg);
    padding: var(--space-2) var(--space-0);
    z-index: var(--z-sticky);
    box-shadow: var(--shadow-md);
  }

  .markdown-renderer :global(.mermaid-menu.open) {
    display: block;
  }

  .markdown-renderer :global(.mermaid-menu-item) {
    display: block;
    width: 100%;
    padding: var(--space-3) var(--space-6);
    border: none;
    background: none;
    color: var(--text-secondary);
    font-size: var(--font-size-md);
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }

  .markdown-renderer :global(.mermaid-menu-item:hover) {
    background: var(--surface-overlay);
  }
</style>
