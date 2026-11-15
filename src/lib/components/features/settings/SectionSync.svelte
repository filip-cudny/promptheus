<script lang="ts">
  import {
    AlertTriangle,
    CheckCircle2,
    CircleSlash,
    PauseCircle,
    RefreshCw,
  } from "lucide-svelte";
  import { ICON_SIZE } from "$lib/constants/ui";
  import SettingsSection from "./SettingsSection.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import {
    getSettingsSyncStatus,
    onSettingsSyncStatusChanged,
    syncSettingsNow,
    type SyncState,
    type SyncStatus,
  } from "$lib/services/settingsSync";
  import { error as logError } from "@tauri-apps/plugin-log";

  const ALREADY_SYNCING = "already syncing";

  const STATE_VIEW: Record<
    SyncState,
    { label: string; icon: typeof RefreshCw; tone: string }
  > = {
    off: { label: "Off", icon: CircleSlash, tone: "muted" },
    synced: { label: "Synced", icon: CheckCircle2, tone: "success" },
    syncing: { label: "Syncing", icon: RefreshCw, tone: "muted" },
    conflict: { label: "Conflict", icon: PauseCircle, tone: "warning" },
    error: { label: "Error", icon: AlertTriangle, tone: "danger" },
  };

  let status = $state<SyncStatus | null>(null);
  let syncError = $state<string | null>(null);

  const view = $derived(status ? STATE_VIEW[status.state] : null);
  const syncing = $derived(status?.state === "syncing");
  const lastSync = $derived(
    status?.last_sync ? new Date(status.last_sync).toLocaleString() : "never",
  );

  async function loadStatus() {
    try {
      status = await getSettingsSyncStatus();
    } catch (e) {
      logError(`get_settings_sync_status failed: ${e}`);
    }
  }

  async function syncNow() {
    syncError = null;
    try {
      status = await syncSettingsNow();
    } catch (e) {
      const message = String(e);
      if (message.includes(ALREADY_SYNCING)) return;
      logError(`sync_settings_now failed: ${message}`);
      syncError = message;
    }
  }

  $effect(() => {
    loadStatus();
    const unlisten = onSettingsSyncStatusChanged((next) => {
      status = next;
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<SettingsSection
  title="Sync"
  hint="Keeps settings, prompts and skills the same on all your machines through a git repository."
>
  {#snippet body()}
    {#if status && view}
      {@const Icon = view.icon}
      <section class="card">
        <h3>Status</h3>
        <div class="state {view.tone}">
          <Icon size={ICON_SIZE.md} />
          <span>{view.label}</span>
        </div>

        {#if status.state === "off"}
          <p class="muted">
            Sync is off because <code>settings.json</code>, <code>prompts</code> and
            <code>skills</code> in the config dir are not symlinks into a git repo.
          </p>
        {:else}
          <dl class="fields">
            <div>
              <dt>Repository</dt>
              <dd>{status.repo_path ?? "—"}</dd>
            </div>
            <div>
              <dt>Branch</dt>
              <dd>{status.branch ?? "—"}</dd>
            </div>
            <div>
              <dt>Last sync</dt>
              <dd>{lastSync}</dd>
            </div>
          </dl>

          {#if status.state === "conflict"}
            <p class="muted">
              Automatic sync is paused. Resolve the conflict in a terminal in the repo,
              then press "Sync now".
            </p>
          {:else if status.state === "error" && status.message}
            <p class="message danger">{status.message}</p>
          {/if}

          <div class="actions">
            <Button variant="ghost" icon={RefreshCw} disabled={syncing} onclick={syncNow}>
              {syncing ? "Syncing…" : "Sync now"}
            </Button>
            {#if syncError}
              <span class="message danger">{syncError}</span>
            {/if}
          </div>
        {/if}
      </section>
    {/if}
  {/snippet}
</SettingsSection>

<style>
  .card {
    background: var(--surface-base);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-lg);
    padding: var(--space-7) var(--space-8);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    max-width: 560px;
  }

  h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    margin: var(--space-0);
  }

  .state {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    font-size: var(--font-size-md);
    color: var(--text-primary);
  }

  .state.success {
    color: var(--success);
  }

  .state.warning {
    color: var(--warning);
  }

  .state.danger {
    color: var(--danger);
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    margin: var(--space-0);
  }

  .fields div {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  dt {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  dd {
    margin: var(--space-0);
    font-size: var(--font-size-base);
    color: var(--text-primary);
    word-break: break-all;
  }

  .muted,
  .message {
    margin: var(--space-0);
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .message.danger {
    color: var(--danger);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
</style>
