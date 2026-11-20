<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { Plus, Trash2, X } from "lucide-svelte";
  import { error as logError, warn as logWarn } from "@tauri-apps/plugin-log";
  import { ICON_SIZE } from "$lib/constants/ui";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import FormRow from "$lib/components/shared/ui/FormRow.svelte";
  import Chip from "$lib/components/shared/ui/Chip.svelte";
  import Switch from "$lib/components/shared/ui/Switch.svelte";
  import NumberInput from "$lib/components/shared/ui/NumberInput.svelte";
  import TextInput from "$lib/components/shared/ui/TextInput.svelte";
  import { useSaveTracker } from "$lib/stores/saveTracker.svelte";
  import { getSettingsStore } from "$lib/stores/settings.svelte";
  import { getSettingsNavigation } from "$lib/stores/settingsNavigation.svelte";
  import { onSettingsChanged } from "$lib/services/events";
  import { applyEnvVarOperation, getEnvVars, updateSetting } from "$lib/services/settings";
  import {
    listMcpServers,
    listMcpTools,
    type McpServerConfig,
    type McpServerState,
    type McpToolInfo,
  } from "$lib/services/mcp";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingField from "./SettingField.svelte";
  import InsertVariableButton from "./InsertVariableButton.svelte";
  import { classifyReference, parseEnvRef } from "$lib/utils/envVarOptions";
  import { serverChip } from "$lib/utils/mcpServerChip";
  import { toolUses, type ToolUse } from "$lib/utils/toolUses";
  import { SURFACE_LABELS } from "$lib/constants/surfaces";
  import type { EnvVarStatus } from "$lib/types";

  const ROW_PREFIX = /^row \d+: /;
  const TOOL_LIST_KEYS: Record<ToolUse, string> = {
    chat: "surfaces.chat.generation.enabled_tools",
    settings_agent: "surfaces.settings_agent.generation.enabled_tools",
  };

  type Pair<V> = { key: number; name: string; value: V };
  type Row = {
    key: number;
    name: string;
    command: string;
    args: string;
    env: Pair<string>[];
    timeout: number;
    toolTimeouts: Pair<number>[];
    disabled: boolean;
    disabledTools: string[];
  };

  const store = getSettingsStore();
  const navigation = getSettingsNavigation();
  const tracker = useSaveTracker({
    debounceMs: store.settings?.autosave_debounce_ms ?? 1000,
  });

  let nextKey = 0;
  const configured = untrack(
    () =>
      ((store.settings as unknown as { mcp_servers?: Record<string, McpServerConfig> })
        ?.mcp_servers ?? {}),
  );

  function toPairs<V>(map: Record<string, V>): Pair<V>[] {
    return Object.entries(map).map(([name, value]) => ({ key: nextKey++, name, value }));
  }

  let rows = $state<Row[]>(
    Object.entries(configured)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([name, c]) => ({
        key: nextKey++,
        name,
        command: c.command,
        args: c.args.join("\n"),
        env: toPairs(c.env),
        timeout: c.timeout_secs,
        toolTimeouts: toPairs(c.tool_timeouts),
        disabled: c.disabled ?? false,
        disabledTools: c.disabled_tools ?? [],
      })),
  );
  let selectedKey = $state<number | null>(untrack(() => rows[0]?.key ?? null));
  let removedName = $state<string | null>(null);
  let confirmDelete = $state(false);
  let states = $state<McpServerState[]>([]);
  let mcpTools = $state<McpToolInfo[]>([]);
  let commandRow = $state<HTMLElement | undefined>();
  let argsEl = $state<HTMLTextAreaElement | undefined>();
  let valueCells = $state<Record<number, HTMLElement | undefined>>({});
  let envVars = $state<EnvVarStatus[]>([]);
  let envLoaded = $state(false);
  let envErrors = $state<Record<number, string>>({});
  let unlisten: (() => void) | null = null;

  const selected = $derived(rows.find((r) => r.key === selectedKey) ?? null);
  const stateOf = $derived(new Map(states.map((s) => [s.name, s])));
  const removed = $derived(
    states.filter((s) => s.state !== "off" && !rows.some((r) => r.name === s.name)),
  );
  const selectedRemoved = $derived(
    selected ? null : (removed.find((s) => s.name === removedName) ?? null),
  );
  const envNames = $derived(envVars.map((v) => v.name));
  const duplicateName = $derived(
    selected !== null && rows.some((r) => r !== selected && r.name === selected.name),
  );

  async function refreshEnv() {
    try {
      envVars = await getEnvVars();
      envLoaded = true;
    } catch (e) {
      logWarn("Failed to read get_env_vars: " + e);
    }
  }

  function referenceState(value: string): string | null {
    if (!envLoaded) return null;
    const reference = classifyReference(value, envNames);
    if (reference.kind === "unlisted") return "not listed";
    if (reference.kind !== "listed") return null;
    return envVars.find((v) => v.name === reference.name)?.set ? "set" : "missing";
  }

  async function addToList(pair: Pair<string>, name: string) {
    delete envErrors[pair.key];
    try {
      await applyEnvVarOperation({ op: "add_env_var", name, description: "", secret: true });
    } catch (e) {
      envErrors[pair.key] = String(e).replace(ROW_PREFIX, "");
      return;
    }
    await refreshEnv();
  }

  function setEnvValue(pair: Pair<string>, value: string) {
    pair.value = value;
    edited();
  }

  onMount(async () => {
    window.addEventListener("focus", refreshEnv);
    listMcpServers()
      .then((s) => (states = s))
      .catch((e) => logError(`list_mcp_servers failed: ${e}`));
    listMcpTools()
      .then((t) => (mcpTools = t))
      .catch((e) => logError(`list_mcp_tools failed: ${e}`));
    unlisten = await onSettingsChanged(refreshEnv);
    await refreshEnv();
  });

  onDestroy(() => {
    window.removeEventListener("focus", refreshEnv);
    unlisten?.();
    void tracker.flush();
    tracker.destroy();
  });

  $effect(() => {
    const item = navigation.selectedItem;
    if (item?.list !== "mcp_servers") return;
    const row = untrack(() => rows.find((r) => r.name === item.id));
    if (row) select(row.key);
  });

  function chip(row: Row) {
    return serverChip(stateOf.get(row.name)?.state ?? null, row.disabled);
  }

  function setServerDisabled(row: Row, disabled: boolean) {
    row.disabled = disabled;
    void tracker.flush(persist);
  }

  function setToolOff(row: Row, tool: string, off: boolean) {
    const without = row.disabledTools.filter((t) => t !== tool);
    row.disabledTools = off ? [...without, tool] : without;
    void tracker.flush(persist);
  }

  function persist() {
    const names = rows.map((r) => r.name);
    if (new Set(names).size !== names.length) return Promise.resolve();
    const map: Record<string, McpServerConfig> = {};
    for (const r of rows) {
      map[r.name] = {
        command: r.command,
        args: r.args.split("\n").filter((a) => a !== ""),
        env: Object.fromEntries(r.env.filter((p) => p.name).map((p) => [p.name, p.value])),
        timeout_secs: r.timeout,
        tool_timeouts: Object.fromEntries(
          r.toolTimeouts.filter((p) => p.name).map((p) => [p.name, p.value]),
        ),
        disabled: r.disabled,
        disabled_tools: r.disabledTools,
      };
    }
    return updateSetting("mcp_servers", map);
  }

  function edited() {
    tracker.scheduleSave(persist);
  }

  function select(key: number) {
    selectedKey = key;
    removedName = null;
    confirmDelete = false;
  }

  function add() {
    const row: Row = {
      key: nextKey++,
      name: "",
      command: "",
      args: "",
      env: [],
      timeout: 180,
      toolTimeouts: [],
      disabled: false,
      disabledTools: [],
    };
    rows.push(row);
    select(row.key);
  }

  function remove() {
    rows = rows.filter((r) => r.key !== selectedKey);
    selectedKey = rows[0]?.key ?? null;
    confirmDelete = false;
    void tracker.flush(persist);
  }

  function addPair<V>(list: Pair<V>[], value: V) {
    list.push({ key: nextKey++, name: "", value });
  }

  function removePair<V>(list: Pair<V>[], key: number) {
    list.splice(
      list.findIndex((p) => p.key === key),
      1,
    );
    edited();
  }

  const nameError = $derived(
    selected
      ? duplicateName
        ? "Another server already has this name."
        : store.fieldError(`mcp_servers.${selected.name}`)
      : undefined,
  );
</script>

<SettingsSection title="MCP Servers" hint="Server changes apply after restart; tool switches apply at once.">
  {#snippet body()}
    <div class="layout">
      <aside class="list">
        {#each rows as row (row.key)}
          <button class="item" class:active={row.key === selectedKey} onclick={() => select(row.key)}>
            <span class="name">{row.name || "(unnamed)"}</span>
            <span class="chip {chip(row).replace(/ /g, '-')}">{chip(row)}</span>
          </button>
        {/each}
        {#each removed as s (s.name)}
          <button
            class="item"
            class:active={selectedKey === null && removedName === s.name}
            onclick={() => {
              selectedKey = null;
              removedName = s.name;
            }}
          >
            <span class="name">{s.name}</span>
            <span class="chip removed">removed after restart</span>
          </button>
        {/each}
        <Button icon={Plus} onclick={add}>Add server</Button>
      </aside>

      <div class="editor">
        {#if selected}
          {@const state = stateOf.get(selected.name)}
          {@const serverTools = mcpTools.filter((t) => t.server === selected.name)}
          <header class="head">
            <h1>{selected.name || "(unnamed)"}</h1>
            <Switch
              checked={!selected.disabled}
              label="Server on"
              onchange={(next) => setServerDisabled(selected, !next)}
            />
          </header>

          {#if chip(selected) === "off"}
            <p class="muted">Off: tools are listed while the server runs.</p>
          {:else if state?.state === "error"}
            <pre class="start-error">{state.error}</pre>
          {:else if state?.state === "running"}
            <div class="tools">
              <h3>Tools</h3>
              {#if serverTools.length === 0}
                <p class="muted">No tools.</p>
              {:else}
                <ul>
                  {#each serverTools as tool (tool.name)}
                    {@const off = selected.disabledTools.includes(tool.name)}
                    {@const uses = store.settings ? toolUses(selected.name, tool.name, store.settings.surfaces) : []}
                    <li class="tool" class:off>
                      <Switch
                        checked={!off}
                        label={`${tool.name} tool`}
                        onchange={(next) => setToolOff(selected, tool.name, !next)}
                      />
                      <div class="tool-main">
                        <code>{tool.name}</code>
                        {#if tool.description}<p class="tool-desc">{tool.description}</p>{/if}
                        <div class="tool-uses">
                          {#if off}
                            Off — not offered in Chat or Settings agent tools.
                          {:else if uses.length === 0}
                            Used by nothing —
                            <button class="tag link" onclick={() => navigation.navigateTo(TOOL_LIST_KEYS.chat)}>
                              assign in Chat ›
                            </button>
                          {:else}
                            Used by
                            {#each uses as use (use)}
                              <button class="tag" onclick={() => navigation.navigateTo(TOOL_LIST_KEYS[use])}>
                                {SURFACE_LABELS[use]}
                              </button>
                            {/each}
                          {/if}
                        </div>
                      </div>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}

          <FormRow label="Name" error={nameError}>
            <TextInput
              value={selected.name}
              aria-label="Server name"
              oninput={(e: Event) => {
                selected.name = (e.target as HTMLInputElement).value;
                edited();
              }}
            />
          </FormRow>

          <SettingField key="mcp_servers.*.command">
            <div class="with-action" bind:this={commandRow}>
              <TextInput
                value={selected.command}
                aria-label="Command"
                oninput={(e: Event) => {
                  selected.command = (e.target as HTMLInputElement).value;
                  edited();
                }}
              />
              <InsertVariableButton
                ariaLabel="Insert variable into command"
                field={() => commandRow?.querySelector("input")}
                oninsert={(text) => {
                  selected.command = text;
                  edited();
                }}
              />
            </div>
          </SettingField>

          <SettingField key="mcp_servers.*.args">
            <div class="with-action top">
              <textarea
                bind:this={argsEl}
                rows="3"
                aria-label="Arguments, one per line"
                placeholder="One argument per line"
                value={selected.args}
                oninput={(e) => {
                  selected.args = e.currentTarget.value;
                  edited();
                }}
              ></textarea>
              <InsertVariableButton
                ariaLabel="Insert variable into arguments"
                field={() => argsEl}
                oninsert={(text) => {
                  selected.args = text;
                  edited();
                }}
              />
            </div>
          </SettingField>

          <SettingField key="mcp_servers.*.env">
            <div class="pairs">
              {#if selected.env.length > 0}
                <div class="pair env env-head" aria-hidden="true">
                  <span class="name-col">Name in server</span>
                  <span>Value</span>
                </div>
              {/if}
              {#each selected.env as pair (pair.key)}
                {@const ref = parseEnvRef(pair.value)}
                {@const refState = ref ? referenceState(pair.value) : null}
                <div class="env-item">
                  <div class="pair env">
                    <TextInput
                      value={pair.name}
                      placeholder="NAME"
                      aria-label="Variable name"
                      oninput={(e: Event) => {
                        pair.name = (e.target as HTMLInputElement).value;
                        edited();
                      }}
                    />
                    <div class="value-cell" bind:this={valueCells[pair.key]}>
                      {#if ref}
                        <span class="chip-slot">
                          <Chip>
                            <span class="chip-text">{refState ? `${ref} · ${refState}` : ref}</span>
                            {#if refState === "not listed"}
                              <button type="button" class="chip-btn" onclick={() => addToList(pair, ref)}>
                                Add to list
                              </button>
                            {/if}
                            <button
                              type="button"
                              class="chip-btn clear"
                              aria-label="Clear value"
                              title="Clear value"
                              onclick={() => setEnvValue(pair, "")}
                            >
                              <X size={ICON_SIZE.sm} />
                            </button>
                          </Chip>
                        </span>
                      {:else}
                        <TextInput
                          value={pair.value}
                          placeholder="value"
                          aria-label={pair.name ? `Value of ${pair.name}` : "Variable value"}
                          oninput={(e: Event) => setEnvValue(pair, (e.target as HTMLInputElement).value)}
                        />
                      {/if}
                      <InsertVariableButton
                        ariaLabel="Insert variable into value"
                        field={() => valueCells[pair.key]?.querySelector("input")}
                        oninsert={(text) => setEnvValue(pair, text)}
                      />
                    </div>
                    <ActionIconButton
                      icon={X}
                      label="Remove variable"
                      variant="danger"
                      onclick={() => removePair(selected.env, pair.key)}
                    />
                  </div>
                  {#if envErrors[pair.key]}<p class="env-error">{envErrors[pair.key]}</p>{/if}
                </div>
              {/each}
              <div>
                <Button icon={Plus} onclick={() => addPair(selected.env, "")}>Add variable</Button>
              </div>
            </div>
          </SettingField>

          <SettingField key="mcp_servers.*.timeout_secs">
            <NumberInput
              min={1}
              step={1}
              value={selected.timeout}
              oninput={(e: Event) => {
                const n = Math.floor(Number((e.currentTarget as HTMLInputElement).value));
                if (!(n >= 1)) return;
                selected.timeout = n;
                edited();
              }}
            />
          </SettingField>

          <SettingField key="mcp_servers.*.tool_timeouts">
            <div class="pairs">
              {#each selected.toolTimeouts as pair (pair.key)}
                <div class="pair">
                  <TextInput
                    value={pair.name}
                    placeholder="tool"
                    aria-label="Tool name"
                    oninput={(e: Event) => {
                      pair.name = (e.target as HTMLInputElement).value;
                      edited();
                    }}
                  />
                  <NumberInput
                    min={1}
                    step={1}
                    value={pair.value}
                    aria-label="Timeout seconds"
                    oninput={(e: Event) => {
                      const n = Math.floor(Number((e.currentTarget as HTMLInputElement).value));
                      if (!(n >= 1)) return;
                      pair.value = n;
                      edited();
                    }}
                  />
                  <ActionIconButton
                    icon={X}
                    label="Remove tool timeout"
                    variant="danger"
                    onclick={() => removePair(selected.toolTimeouts, pair.key)}
                  />
                </div>
              {/each}
              <div>
                <Button icon={Plus} onclick={() => addPair(selected.toolTimeouts, 180)}>Add tool timeout</Button>
              </div>
            </div>
          </SettingField>

          <section class="danger">
            {#if !confirmDelete}
              <Button variant="danger" icon={Trash2} onclick={() => (confirmDelete = true)}>Delete server</Button>
            {:else}
              <p>Delete <strong>{selected.name || "this server"}</strong>?</p>
              <div class="confirm">
                <Button onclick={() => (confirmDelete = false)}>Cancel</Button>
                <Button variant="danger" onclick={remove}>Delete server</Button>
              </div>
            {/if}
          </section>
        {:else if selectedRemoved}
          <p class="muted">
            <code>{selectedRemoved.name}</code> is no longer configured and stops after restart.
          </p>
        {:else}
          <p class="muted">Add a server or pick one to edit.</p>
        {/if}
      </div>
    </div>
  {/snippet}
</SettingsSection>

<style>
  .layout {
    display: flex;
    gap: var(--space-8);
    align-items: flex-start;
  }

  .list {
    width: 220px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
    background: transparent;
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--font-size-md);
    cursor: pointer;
    text-align: left;
  }

  .item.active {
    background: var(--surface-overlay-faint);
    border-color: var(--accent);
  }

  .chip {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  .chip.running {
    color: var(--success);
  }

  .chip.error {
    color: var(--danger);
  }

  .chip.off,
  .chip.stops-after-restart {
    color: var(--text-faint);
  }

  .editor {
    flex: 1;
    min-width: 0;
    max-width: 640px;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-5);
  }

  .head h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
    margin: var(--space-0);
    line-height: 1.2;
  }

  .start-error {
    margin: 0;
    padding: var(--space-4);
    white-space: pre-wrap;
    word-break: break-word;
    background: var(--surface-sunken);
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-md);
    color: var(--danger);
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
  }

  .tools h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .tools ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .tool {
    display: flex;
    align-items: flex-start;
    gap: var(--space-5);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border-faint);
    border-radius: var(--radius-md);
  }

  .tool.off code,
  .tool.off .tool-desc {
    color: var(--text-faint);
  }

  .tool-main {
    flex: 1;
    min-width: 0;
  }

  .tool-desc {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .tool-uses {
    margin-top: var(--space-2);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .tag {
    font: inherit;
    font-size: var(--font-size-xs);
    padding: 1px var(--space-3);
    background: var(--tag-bg);
    color: var(--text-primary);
    border: 1px solid var(--tag-border);
    border-radius: var(--radius-xl);
    line-height: 1.4;
    white-space: nowrap;
    cursor: pointer;
  }

  .tag:hover {
    text-decoration: underline;
  }

  .tag.link {
    background: transparent;
    color: var(--accent);
    border: none;
    padding: 0;
    font-size: inherit;
  }

  .muted {
    color: var(--text-muted);
    font-size: var(--font-size-md);
  }

  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    padding: var(--space-3);
    background: var(--surface-sunken);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: var(--font-size-md);
  }

  .with-action {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }

  .with-action.top {
    align-items: flex-start;
  }

  .pairs {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .pair {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }

  .pair.env > :global(.text-input),
  .env-head .name-col {
    flex: 0 0 38%;
  }

  .env-head {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  .value-cell {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }

  .chip-slot {
    flex: 1;
    min-width: 0;
    display: flex;
  }

  .chip-slot > :global(.chip) {
    max-width: 100%;
  }

  .chip-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip-btn {
    display: inline-flex;
    align-items: center;
    padding: 0;
    background: transparent;
    border: none;
    color: var(--accent);
    font: inherit;
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .chip-btn.clear {
    color: var(--text-muted);
  }

  .env-error {
    margin: var(--space-1) 0 0;
    font-size: var(--font-size-sm);
    color: var(--danger);
  }

  .danger {
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-lg);
    padding: var(--space-6);
  }

  .danger p {
    margin: 0 0 var(--space-4);
  }

  .confirm {
    display: flex;
    gap: var(--space-4);
  }
</style>
