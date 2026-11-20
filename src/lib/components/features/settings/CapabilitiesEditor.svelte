<script lang="ts">
  import { ListChecks } from "lucide-svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import type { ModelCapabilities, ReasoningMode } from "$lib/types";
  import {
    REASONING_LEVELS,
    REASONING_LEVEL_LABELS,
    type ReasoningLevel,
  } from "$lib/constants/models";

  let {
    capabilities,
    onChange,
  }: {
    capabilities: ModelCapabilities | null;
    onChange: (next: ModelCapabilities | null) => void;
  } = $props();

  type Kind = ReasoningMode["kind"];

  const KIND_OPTIONS: { value: Kind; label: string }[] = [
    { value: "unsupported", label: "Unsupported" },
    { value: "effort", label: "Effort" },
    { value: "budget_tokens", label: "Budget tokens" },
    { value: "toggle", label: "Toggle" },
  ];

  const OPENAI_PRESET: ReasoningLevel[] = [
    "none",
    "minimal",
    "low",
    "medium",
    "high",
    "xhigh",
  ];

  let kind = $derived<Kind>(capabilities?.reasoning.kind ?? "unsupported");

  let allowedSet = $derived.by<Set<ReasoningLevel>>(() => {
    if (capabilities?.reasoning.kind !== "effort") return new Set();
    return new Set(capabilities.reasoning.allowed);
  });

  let budgetMin = $derived<number>(
    capabilities?.reasoning.kind === "budget_tokens"
      ? capabilities.reasoning.min
      : 1024,
  );

  let budgetMax = $derived<number>(
    capabilities?.reasoning.kind === "budget_tokens"
      ? capabilities.reasoning.max
      : 64000,
  );

  let allowedEmpty = $derived(kind === "effort" && allowedSet.size === 0);

  function selectKind(next: Kind) {
    if (next === kind) return;
    switch (next) {
      case "unsupported":
        onChange({ reasoning: { kind: "unsupported" } });
        return;
      case "effort":
        onChange({
          reasoning: {
            kind: "effort",
            allowed: [...OPENAI_PRESET],
          },
        });
        return;
      case "budget_tokens":
        onChange({
          reasoning: { kind: "budget_tokens", min: 1024, max: 64000 },
        });
        return;
      case "toggle":
        onChange({ reasoning: { kind: "toggle" } });
        return;
    }
  }

  function toggleAllowed(level: ReasoningLevel) {
    if (kind !== "effort") return;
    const next = new Set(allowedSet);
    if (next.has(level)) next.delete(level);
    else next.add(level);
    const ordered = REASONING_LEVELS.filter((l) => next.has(l));
    onChange({ reasoning: { kind: "effort", allowed: ordered } });
  }

  function applyOpenAiPreset() {
    onChange({
      reasoning: { kind: "effort", allowed: [...OPENAI_PRESET] },
    });
  }

  function setBudgetMin(value: number) {
    if (kind !== "budget_tokens") return;
    const min = Number.isFinite(value) && value > 0 ? Math.floor(value) : 1;
    const max = Math.max(min, budgetMax);
    onChange({ reasoning: { kind: "budget_tokens", min, max } });
  }

  function setBudgetMax(value: number) {
    if (kind !== "budget_tokens") return;
    const max = Number.isFinite(value) && value > 0 ? Math.floor(value) : 1;
    const min = Math.min(budgetMin, max);
    onChange({ reasoning: { kind: "budget_tokens", min, max } });
  }

  function resetToInferred() {
    onChange(null);
  }
</script>

<div class="caps">
  <div class="header">
    <span class="label">Reasoning</span>
    {#if capabilities !== null}
      <button class="reset-link" onclick={resetToInferred} title="Use inferred default from provider/model">
        Reset to inferred
      </button>
    {/if}
  </div>

  <div class="kind-row">
    {#each KIND_OPTIONS as opt}
      <label class="kind-option" class:active={kind === opt.value}>
        <input
          type="radio"
          name="reasoning-kind"
          checked={kind === opt.value}
          onchange={() => selectKind(opt.value)}
        />
        <span>{opt.label}</span>
      </label>
    {/each}
  </div>

  {#if kind === "effort"}
    <div class="sub">
      <span class="sub-label">Allowed levels</span>
      <div class="checks">
        {#each REASONING_LEVELS as level}
          <label class="check">
            <input
              type="checkbox"
              checked={allowedSet.has(level)}
              onchange={() => toggleAllowed(level)}
            />
            <span>{REASONING_LEVEL_LABELS[level]}</span>
          </label>
        {/each}
      </div>
      {#if allowedEmpty}
        <p class="hint warn">
          Select at least one level, or click below to apply the OpenAI preset.
        </p>
      {/if}
      <div>
        <Button icon={ListChecks} onclick={applyOpenAiPreset}>Apply OpenAI preset</Button>
      </div>
    </div>
  {:else if kind === "budget_tokens"}
    <div class="sub">
      <div class="budget-row">
        <label>
          <span class="sub-label">Min tokens</span>
          <input
            type="number"
            min="1"
            value={budgetMin}
            oninput={(e) => setBudgetMin(Number((e.target as HTMLInputElement).value))}
          />
        </label>
        <label>
          <span class="sub-label">Max tokens</span>
          <input
            type="number"
            min="1"
            value={budgetMax}
            oninput={(e) => setBudgetMax(Number((e.target as HTMLInputElement).value))}
          />
        </label>
      </div>
    </div>
  {/if}
</div>

<style>
  .caps {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .label {
    font-size: var(--font-size-md);
    color: var(--text-secondary);
    font-weight: var(--font-weight-semibold);
  }

  .reset-link {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--font-size-sm);
    cursor: pointer;
    text-decoration: underline;
    padding: 0;
  }

  .reset-link:hover {
    color: var(--text-primary);
  }

  .kind-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .kind-option {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    background: var(--surface-sunken);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-size: var(--font-size-md);
    cursor: pointer;
  }

  .kind-option.active {
    border-color: var(--accent-border);
    color: var(--accent-text);
    background: var(--accent-bg-soft);
  }

  .kind-option input {
    accent-color: var(--accent);
  }

  .sub {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-left: var(--space-3);
    border-left: 2px solid var(--border-faint);
  }

  .sub-label {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3) var(--space-5);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-primary);
    font-size: var(--font-size-md);
    cursor: pointer;
  }

  .budget-row {
    display: flex;
    gap: var(--space-5);
  }

  .budget-row label {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    flex: 1;
  }

  .budget-row input {
    padding: var(--space-3) var(--space-4);
    background: var(--surface-sunken);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
  }

  .hint {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    margin: 0;
  }

  .hint.warn {
    color: var(--warning);
  }
</style>
