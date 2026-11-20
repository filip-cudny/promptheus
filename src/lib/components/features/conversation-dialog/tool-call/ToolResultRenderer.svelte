<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { parseToolResult } from "$lib/utils/toolResultParser";
  import { isXmlLike } from "$lib/utils/xmlParser";
  import type { HintStatus } from "$lib/types/toolResult";
  import MarkdownRenderer from "$lib/components/shared/ui/MarkdownRenderer.svelte";
  import SearchResultsRenderer from "./SearchResultsRenderer.svelte";
  import JsonNode from "./JsonNode.svelte";
  import { saveSvg } from "$lib/services/fileSave";

  let {
    result,
    isPartial = false,
  }: {
    result: string | null;
    isPartial?: boolean;
  } = $props();

  let parsed = $derived(isPartial ? null : parseToolResult(result));

  const STATUS_COLORS: Record<HintStatus, { bg: string; fg: string }> = {
    success: { bg: "var(--success-bg-soft)", fg: "var(--success-text)" },
    error: { bg: "var(--danger-bg-soft)", fg: "var(--danger-text)" },
    empty: { bg: "var(--warning-bg-soft)", fg: "var(--warning-text)" },
    partial: { bg: "var(--accent-bg-soft)", fg: "var(--accent-text)" },
  };

  const MARKDOWN_PATTERN = /(?:^#{1,6}\s|^\s*[-*]\s|\*\*|```|\[.+\]\()/m;

  function isMarkdownLike(text: string): boolean {
    return MARKDOWN_PATTERN.test(text);
  }

  function formatMetadataValue(value: unknown): string {
    if (typeof value === "number") {
      if (value > 1000 && Number.isInteger(value)) return `${(value / 1000).toFixed(1)}s`;
      return String(value);
    }
    return String(value);
  }

  function extractEnvelopeData(data: unknown): {
    formattedContext: string | null;
    metadata: Record<string, unknown> | null;
    otherData: Record<string, unknown> | null;
  } {
    if (typeof data !== "object" || data === null) {
      return { formattedContext: null, metadata: null, otherData: null };
    }
    const obj = data as Record<string, unknown>;
    const formattedContext =
      typeof obj.formattedContext === "string" ? obj.formattedContext : null;
    const metadata =
      typeof obj.metadata === "object" && obj.metadata !== null
        ? (obj.metadata as Record<string, unknown>)
        : null;

    const rest: Record<string, unknown> = {};
    let hasRest = false;
    for (const [key, val] of Object.entries(obj)) {
      if (key !== "formattedContext" && key !== "metadata") {
        rest[key] = val;
        hasRest = true;
      }
    }

    return {
      formattedContext,
      metadata,
      otherData: hasRest ? rest : null,
    };
  }
</script>

{#if isPartial && result}
  <span class="partial-text">{result}</span>
{:else if parsed}
  {#if parsed.kind === "envelope"}
    {@const colors = STATUS_COLORS[parsed.hint.status]}
    {@const envelope = extractEnvelopeData(parsed.data)}
    <div class="envelope-result">
      <div class="envelope-header">
        <span class="status-pill" style:background={colors.bg} style:color={colors.fg}>
          {parsed.hint.status}
        </span>
        <span class="hint-summary">{parsed.hint.summary}</span>
      </div>

      {#if envelope.formattedContext}
        {#if isXmlLike(envelope.formattedContext)}
          <SearchResultsRenderer xml={envelope.formattedContext} />
        {:else}
          <pre class="formatted-context">{envelope.formattedContext}</pre>
        {/if}
      {/if}

      {#if envelope.metadata}
        <div class="metadata-grid">
          {#each Object.entries(envelope.metadata) as [key, val]}
            <span class="meta-key">{key}</span>
            <span class="meta-value">{formatMetadataValue(val)}</span>
          {/each}
        </div>
      {/if}

      {#if envelope.otherData}
        <div class="envelope-data">
          <JsonNode value={envelope.otherData} />
        </div>
      {/if}

      {#if parsed.hint.diagnostics}
        <div class="diagnostics">
          <span class="section-label">Diagnostics</span>
          <div class="metadata-grid">
            {#each Object.entries(parsed.hint.diagnostics) as [key, val]}
              <span class="meta-key">{key}</span>
              <span class="meta-value">{val}</span>
            {/each}
          </div>
        </div>
      {/if}

      {#if parsed.hint.nextActions.length > 0}
        <div class="next-actions">
          <span class="section-label">Suggested</span>
          {#each parsed.hint.nextActions as action}
            <div class="action-item">
              <span class="action-tool">{action.tool}</span>
              <span class="action-why">{action.why}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {:else if parsed.kind === "json"}
    <div class="json-viewer">
      <JsonNode value={parsed.value} />
    </div>
  {:else if parsed.kind === "text"}
    {#if isMarkdownLike(parsed.text)}
      <MarkdownRenderer content={parsed.text} isStreaming={false} onopen={openUrl} onsavesvg={saveSvg} />
    {:else}
      <pre class="plain-text">{parsed.text}</pre>
    {/if}
  {/if}
{/if}

<style>
  .partial-text {
    font-size: var(--font-size-md);
    color: var(--text-disabled);
  }

  .envelope-result {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .envelope-header {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    font-size: var(--font-size-xs);
    font-weight: 700;
    padding: 1px var(--space-3);
    border-radius: var(--radius-sm);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
    flex-shrink: 0;
  }

  .hint-summary {
    font-size: var(--font-size-md);
    color: var(--text-secondary);
  }

  .formatted-context {
    font-size: var(--font-size-sm);
    line-height: var(--line-height-normal);
    color: var(--text-muted);
    white-space: pre-wrap;
    word-break: break-word;
    margin: var(--space-0);
    max-height: 250px;
    overflow-y: auto;
  }

  .metadata-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-3) var(--space-8);
    font-size: var(--font-size-sm);
    align-items: baseline;
  }

  .meta-key {
    color: var(--text-muted);
    font-weight: var(--font-weight-regular);
    letter-spacing: 0.1px;
  }

  .meta-value {
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    word-break: break-word;
  }

  .section-label {
    display: block;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--text-disabled);
    text-transform: uppercase;
    letter-spacing: var(--tracking-label);
    margin-bottom: var(--space-2);
  }

  .diagnostics,
  .next-actions {
    padding-top: var(--space-2);
    border-top: 1px solid var(--border-faint);
  }

  .action-item {
    display: flex;
    gap: var(--space-3);
    font-size: var(--font-size-sm);
    padding: 1px var(--space-0);
  }

  .action-tool {
    color: var(--accent);
    font-weight: var(--font-weight-medium);
    flex-shrink: 0;
  }

  .action-why {
    color: var(--text-disabled);
  }

  .envelope-data {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    line-height: 1.4;
  }

  .json-viewer {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    line-height: 1.4;
    max-height: 300px;
    overflow-y: auto;
  }

  .plain-text {
    font-size: var(--font-size-md);
    line-height: var(--line-height-normal);
    color: var(--text-muted);
    white-space: pre-wrap;
    word-break: break-word;
    margin: var(--space-0);
  }
</style>
