import { error as logError } from "@tauri-apps/plugin-log";
import {
  appendContext,
  appendContextImage,
  clearContext,
  getContextItems,
} from "$lib/services/context";
import { parseContextSource, serializeContextSource } from "$lib/utils/contextSource";
import type { ConversationImage } from "$lib/types/conversation";

export function useContextLoadSave() {
  let saving = $state(false);
  let errorMessage = $state("");

  async function load(): Promise<{ text: string; images: ConversationImage[] }> {
    const textItems: string[] = [];
    const images: ConversationImage[] = [];
    const items = await getContextItems();
    for (const item of items) {
      if (item.item_type === "text") {
        textItems.push(item.content);
      } else if (item.item_type === "image") {
        images.push({ data: item.data, media_type: item.media_type, number: item.number });
      }
    }
    return { text: serializeContextSource(textItems), images };
  }

  async function save(text: string, images: ConversationImage[]): Promise<boolean> {
    saving = true;
    errorMessage = "";
    try {
      await clearContext();
      for (const img of images) await appendContextImage(img.data, img.media_type);
      for (const item of parseContextSource(text)) await appendContext(item);
      return true;
    } catch (e) {
      errorMessage = e instanceof Error ? e.message : String(e);
      logError(`Failed to save context: ${errorMessage}`);
      return false;
    } finally {
      saving = false;
    }
  }

  return {
    get saving() {
      return saving;
    },
    get errorMessage() {
      return errorMessage;
    },
    load,
    save,
  };
}

export type ContextLoadSave = ReturnType<typeof useContextLoadSave>;
