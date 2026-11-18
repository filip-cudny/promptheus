import { invoke } from "@tauri-apps/api/core";
import type { SearchQuery, SearchResponse } from "$lib/types/historySearch";
import type {
  HistoryEntry,
  HistoryEntryType,
  InvocationSource,
  ImagePayload,
  SerializedConversationNode,
  HistoryStorageStats,
} from "$lib/types";

export async function searchHistory(query: SearchQuery): Promise<SearchResponse> {
  return invoke("search_history", { query });
}

export async function getConversations(
  offset: number,
  limit: number,
): Promise<HistoryEntry[]> {
  return invoke("get_conversations", { offset, limit });
}

export async function getHistoryEntry(
  entryId: string,
): Promise<HistoryEntry | null> {
  return invoke("get_history_entry", { entryId });
}

export async function addConversationEntry(params: {
  contextText: string;
  skillId: string | null;
  skillName: string | null;
  success: boolean;
  error: string | null;
  nodes: SerializedConversationNode[];
  rootNodeId: string | null;
  currentPath: string[];
  tabId: string | null;
  images: ImagePayload[];
  modelId: string | null;
  reasoningEffort: string | null;
  invocationSource: InvocationSource;
}): Promise<string> {
  return invoke("add_conversation_entry", params);
}

export async function updateConversationEntry(params: {
  entryId: string;
  contextText: string;
  nodes: SerializedConversationNode[];
  rootNodeId: string | null;
  currentPath: string[];
  images: ImagePayload[];
  modelId: string | null;
  reasoningEffort: string | null;
}): Promise<void> {
  return invoke("update_conversation_entry", params);
}

export async function updateHistoryEntryTitle(
  entryId: string,
  title: string,
): Promise<void> {
  return invoke("update_history_entry_title", { entryId, title });
}

export async function deleteHistoryEntry(
  entryId: string,
): Promise<void> {
  return invoke("delete_history_entry", { entryId });
}

export async function getHistoryStorageStats(): Promise<HistoryStorageStats> {
  return invoke("get_history_storage_stats");
}

export async function compactHistoryDatabase(): Promise<HistoryStorageStats> {
  return invoke("compact_history_database");
}

export type LastInteractionChip = "input" | "output" | "transcription";

/**
 * Copies the full text behind a "Last interaction" chip. The value is resolved
 * in Rust, since the menu only holds previews.
 */
export async function copyLastInteraction(chip: LastInteractionChip): Promise<void> {
  return invoke("copy_last_interaction", { chip });
}

export interface RecentQuickAction {
  id: string;
  entry_type: HistoryEntryType;
  skill_id: string | null;
  skill_name: string | null;
  timestamp: string;
  input_preview: string;
  output_preview: string | null;
  status: "ok" | "failed" | "retrying";
  has_audio: boolean;
}

export async function getRecentQuickActions(limit: number): Promise<RecentQuickAction[]> {
  return invoke("get_recent_quick_actions", { limit });
}

/** Resolves to false when nothing was copied (the Output of a failed row). */
export async function copyHistoryEntryPart(
  entryId: string,
  part: "input" | "output",
): Promise<boolean> {
  return invoke<boolean>("copy_history_entry_part", { entryId, part });
}

/** Copies a history entry's full response (or its prompt, when there is none). */
export async function copyHistoryEntryContent(entryId: string): Promise<void> {
  return invoke("copy_history_entry_content", { entryId });
}
