<script lang="ts">
  import Modal from "$lib/components/shared/ui/Modal.svelte";
  import Button from "$lib/components/shared/ui/Button.svelte";
  import type { ImportConflictMode } from "$lib/types";

  let {
    existingSlug,
    onClose,
    onResolve,
  }: {
    existingSlug: string;
    onClose: () => void;
    onResolve: (mode: ImportConflictMode) => void;
  } = $props();
</script>

<Modal title="Skill already exists" width={520} {onClose}>
  <p>
    A skill named <code>/{existingSlug}</code> already exists. Choose how to import:
  </p>

  <footer>
    <Button onclick={onClose}>Cancel</Button>
    <Button
      variant="primary"
      onclick={() => {
        onResolve("rename_suffix");
        onClose();
      }}
    >
      Import as <code>{existingSlug}-imported</code>
    </Button>
    <Button
      variant="danger"
      onclick={() => {
        onResolve("overwrite");
        onClose();
      }}
    >
      Overwrite skill
    </Button>
  </footer>
</Modal>

<style>
  p {
    margin: 0;
    font-size: var(--font-size-md);
    color: var(--text-secondary);
  }

  code {
    font-family: var(--font-mono);
    padding: 0 4px;
    background: var(--surface-overlay-faint);
    border-radius: var(--radius-sm);
    color: var(--accent-text);
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }
</style>
