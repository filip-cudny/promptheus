<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { error as logError } from "@tauri-apps/plugin-log";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import TextInput from "$lib/components/shared/ui/TextInput.svelte";
  import {
    syncCloneDir,
    syncConnect,
    syncPrepare,
    type SyncEntries,
    type SyncPrepared,
    type SyncStatus,
  } from "$lib/services/settingsSync";

  const PUBLICKEY_DENIED = "Permission denied (publickey)";

  let {
    oncancel,
    onconnected,
  }: {
    oncancel: () => void;
    onconnected: (status: SyncStatus) => void;
  } = $props();

  let source = $state<"url" | "dir">("url");
  let url = $state("");
  let dir = $state("");
  let cloneDir = $state<string | null>(null);
  let checking = $state(false);
  let checkError = $state<string | null>(null);
  let prepared = $state<SyncPrepared | null>(null);
  let folder = $state("");
  let entries = $state<SyncEntries>({ settings: true, prompts: true, skills: true });
  let connecting = $state(false);
  let connectError = $state<string | null>(null);

  const sourceValue = $derived((source === "url" ? url : dir).trim());
  const foundNames = $derived(
    prepared
      ? [
          prepared.found.settings && "settings.json",
          prepared.found.prompts && "prompts/",
          prepared.found.skills && "skills/",
        ].filter((name): name is string => Boolean(name))
      : [],
  );
  const found = $derived(foundNames.length > 0);
  const anyEntry = $derived(entries.settings || entries.prompts || entries.skills);
  const urlStatus = $derived(
    prepared?.found.settings
      ? "the repository holds Promptheus settings"
      : prepared?.empty
        ? "the repository is empty"
        : "the repository holds other files",
  );

  $effect(() => {
    syncCloneDir()
      .then((path) => (cloneDir = path))
      .catch((e) => logError(`sync_clone_dir failed: ${e}`));
  });

  function resetCheck() {
    prepared = null;
    checkError = null;
    connectError = null;
  }

  async function chooseFolder() {
    try {
      const picked = await open({ directory: true });
      if (typeof picked === "string") {
        dir = picked;
        resetCheck();
      }
    } catch (e) {
      logError(`choose repository folder failed: ${e}`);
    }
  }

  async function checkAccess() {
    if (!sourceValue || checking) return;
    resetCheck();
    checking = true;
    try {
      const result = await syncPrepare({ kind: source, value: sourceValue });
      folder = result.suggested_folder;
      entries = { settings: true, prompts: true, skills: true };
      prepared = result;
    } catch (e) {
      logError(`sync_prepare (${source}) failed`);
      checkError = String(e);
    } finally {
      checking = false;
    }
  }

  async function connect() {
    if (!prepared || connecting) return;
    connectError = null;
    connecting = true;
    try {
      const status = await syncConnect({
        repo_path: prepared.repo_path,
        folder: folder.trim(),
        entries: { ...entries },
        empty: prepared.empty,
      });
      onconnected(status);
    } catch (e) {
      logError("sync_connect failed");
      connectError = String(e);
    } finally {
      connecting = false;
    }
  }
</script>

<div class="form">
  <h3>Connect a git repo</h3>

  <fieldset class="radios">
    <legend class="sr-only">Repository source</legend>
    <label class="check">
      <input type="radio" name="sync-source" value="url" bind:group={source} onchange={resetCheck} />
      <span>Clone from a URL</span>
    </label>
    <label class="check">
      <input type="radio" name="sync-source" value="dir" bind:group={source} onchange={resetCheck} />
      <span>Repository already on this disk</span>
    </label>
  </fieldset>

  {#if source === "url"}
    <div class="field">
      <label for="sync-url">Repository URL</label>
      <div class="row">
        <TextInput
          id="sync-url"
          bind:value={url}
          placeholder="git@github.com:you/settings.git"
          oninput={resetCheck}
          onkeydown={(e: KeyboardEvent) => e.key === "Enter" && checkAccess()}
        />
        <Button disabled={checking || !sourceValue} onclick={checkAccess}>
          {checking ? "Checking…" : "Check access"}
        </Button>
      </div>
      <p class="muted">
        Cloned to <code>{cloneDir ?? "—"}</code>. An empty repository works: create one without a
        README and paste its URL.
      </p>
    </div>
  {:else}
    <div class="field">
      <label for="sync-dir">Repository folder</label>
      <div class="row">
        <TextInput
          id="sync-dir"
          bind:value={dir}
          placeholder="~/dotfiles"
          oninput={resetCheck}
          onkeydown={(e: KeyboardEvent) => e.key === "Enter" && checkAccess()}
        />
        <Button disabled={checking} onclick={chooseFolder}>Choose…</Button>
        <Button disabled={checking || !sourceValue} onclick={checkAccess}>
          {checking ? "Checking…" : "Check access"}
        </Button>
      </div>
      <p class="muted">
        A git clone with a remote branch. The app commits only the synced entries; the rest of the
        repository is left alone.
      </p>
    </div>
  {/if}

  {#if checkError}
    <div class="note err">
      <span>{checkError}</span>
      {#if checkError.includes(PUBLICKEY_DENIED)}
        <span>
          Add your SSH public key to your account at the git host and load it with
          <code>ssh-add</code>; the app cannot ask for a passphrase.
        </span>
      {/if}
    </div>
  {/if}

  {#if prepared}
    <div class="note ok">Access OK{source === "url" ? `, ${urlStatus}` : ""}.</div>

    <div class="field">
      <label for="sync-folder">Folder in the repository</label>
      <TextInput id="sync-folder" bind:value={folder} />
      <p class="muted">
        {#if found}
          Found <code>{prepared.suggested_folder}/{foundNames.join(", ")}</code>.
        {:else}
          Created if missing.
        {/if}
      </p>
    </div>

    <fieldset class="field">
      <legend>What to sync</legend>
      <label class="check">
        <input type="checkbox" bind:checked={entries.settings} />
        <span>Settings <small>settings.json, with the settings agent's change history</small></span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={entries.prompts} />
        <span>Prompts <small>prompts/</small></span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={entries.skills} />
        <span>Skills <small>skills/</small></span>
      </label>
    </fieldset>

    <div class="note">
      {#if found}
        For each checked entry the repository's version wins. Your local copy goes to a backup in
        the config dir.
      {:else}
        Your current entries move into the folder and the config dir links to them. The first sync
        commits and pushes them.
      {/if}
    </div>

    {#if connectError}
      <div class="note err">{connectError}</div>
    {/if}

    <div class="row">
      <Button disabled={connecting} onclick={oncancel}>Cancel</Button>
      <Button
        variant="primary"
        disabled={connecting || !anyEntry || !folder.trim()}
        onclick={connect}
      >
        {connecting ? "Connecting…" : "Connect"}
      </Button>
    </div>
  {:else}
    <div class="row">
      <Button disabled={checking} onclick={oncancel}>Cancel</Button>
    </div>
  {/if}
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    margin: var(--space-0);
  }

  fieldset {
    border: none;
    margin: var(--space-0);
    padding: var(--space-0);
    min-width: 0;
  }

  .radios {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .field > label,
  legend {
    font-size: var(--font-size-md);
    color: var(--text-primary);
    padding: var(--space-0);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    font-size: var(--font-size-md);
    color: var(--text-primary);
    cursor: pointer;
  }

  .check input {
    margin-top: var(--space-1);
    cursor: pointer;
  }

  small {
    display: block;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .muted {
    margin: var(--space-0);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  code {
    font-family: var(--font-mono);
    color: var(--text-secondary);
    word-break: break-all;
  }

  .note {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-5);
    border-radius: var(--radius-md);
    background: var(--surface-sunken);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    word-break: break-word;
  }

  .note.ok {
    background: var(--success-bg-soft);
    color: var(--success);
  }

  .note.err {
    background: var(--danger-bg-soft);
    color: var(--danger);
  }
</style>
