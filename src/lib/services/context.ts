import { invoke } from "@tauri-apps/api/core";
import type { ContextItem } from "$lib/types/context";

export async function getContextItems(): Promise<ContextItem[]> {
  return invoke("get_context_items");
}

export async function getContextText(): Promise<string | null> {
  return invoke("get_context_text");
}

export async function getLabelledContext(): Promise<string | null> {
  return invoke("get_labelled_context");
}

export async function hasContext(): Promise<boolean> {
  return invoke("has_context");
}

export async function appendContext(value: string): Promise<void> {
  return invoke("append_context", { value });
}

export async function clearContext(): Promise<void> {
  return invoke("clear_context");
}

export async function removeContextItem(index: number): Promise<boolean> {
  return invoke("remove_context_item", { index });
}

export async function appendContextImage(
  data: string,
  mediaType: string,
): Promise<void> {
  return invoke("append_context_image", { data, mediaType });
}

export async function appendContextFromClipboard(): Promise<void> {
  return invoke("append_context_from_clipboard");
}
