<details class="card">
  <summary>How sync works</summary>
  <ol>
    <li>At startup, every 5 minutes and on <b>Sync now</b> the app runs <code>git fetch</code>.</li>
    <li>
      It commits only the synced entries (settings.json, prompts/, skills/ and the change
      history next to settings.json) as <code>chore(sync): update from &lt;host&gt;</code>.
      Other files in the repository are never committed.
    </li>
    <li>It rebases on the remote branch, then pushes.</li>
    <li>
      On a conflict automatic sync pauses until you resolve it in a terminal in the repository
      and press <b>Sync now</b>.
    </li>
  </ol>
  <ul>
    <li>API keys (<code>.env</code>), window state, conversation history and logs are never synced.</li>
    <li>
      Git must reach the remote without a prompt: an SSH key loaded in ssh-agent, or a credential
      helper for HTTPS.
    </li>
  </ul>
</details>

<style>
  .card {
    background: var(--surface-base);
    border: 1px solid var(--border-hard);
    border-radius: var(--radius-lg);
    padding: var(--space-7) var(--space-8);
    max-width: 560px;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  summary {
    cursor: pointer;
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    color: var(--text-primary);
  }

  ol,
  ul {
    margin: var(--space-5) var(--space-0) var(--space-0);
    padding-left: var(--space-8);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  b {
    color: var(--text-primary);
    font-weight: var(--font-weight-medium);
  }

  code {
    font-family: var(--font-mono);
    color: var(--text-secondary);
  }
</style>
