import { onContextChanged } from "$lib/services/events";
import type { ConversationImage } from "$lib/types/conversation";
import type { ContextLoadSave } from "./useContextLoadSave.svelte";

interface EditorFields {
  read: () => { text: string; images: ConversationImage[] };
  write: (loaded: { text: string; images: ConversationImage[] }) => void;
}

export function useOutsideContextChange(ctx: ContextLoadSave, fields: EditorFields) {
  let dirty = $state(false);
  let changedOutside = $state(false);

  async function reload() {
    fields.write(await ctx.load());
    dirty = false;
  }

  function handleContextChanged() {
    if (ctx.saving) return;
    if (dirty) {
      changedOutside = true;
    } else {
      reload();
    }
  }

  async function saveFields() {
    dirty = false;
    changedOutside = false;
    const { text, images } = fields.read();
    if (await ctx.save(text, images)) {
      await reload();
    } else {
      dirty = true;
    }
  }

  async function save() {
    if (changedOutside) return;
    await saveFields();
  }

  async function reloadFromBar() {
    changedOutside = false;
    dirty = false;
    await reload();
  }

  async function start(): Promise<() => void> {
    const unlisten = await onContextChanged(handleContextChanged);
    reload();
    return unlisten;
  }

  return {
    get changedOutside() {
      return changedOutside;
    },
    markEdited() {
      dirty = true;
    },
    start,
    save,
    saveMine: saveFields,
    reload: reloadFromBar,
  };
}
