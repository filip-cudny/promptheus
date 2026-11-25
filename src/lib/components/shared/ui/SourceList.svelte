<script lang="ts">
  import type { Source } from "$lib/types/ai";

  let { sources, onopen }: { sources: Source[]; onopen: (url: string) => void } = $props();
</script>

{#if sources.length > 0}
  <div class="source-list">
    <div class="heading">Sources</div>
    <ul>
      {#each sources as source (source.url)}
        <li>
          <a
            href={source.url}
            title={source.url}
            onclick={(e) => {
              e.preventDefault();
              onopen(source.url);
            }}>{source.title}</a
          >
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .source-list {
    margin: var(--space-4) var(--space-0);
    font-size: var(--font-size-sm);
  }

  .heading {
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    margin-bottom: var(--space-2);
  }

  ul {
    margin: var(--space-0);
    padding-left: var(--space-10);
  }

  a {
    color: var(--syntax-link);
    text-decoration: underline;
    overflow-wrap: anywhere;
  }
</style>
