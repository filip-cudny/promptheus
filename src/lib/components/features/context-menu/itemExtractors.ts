import type { MenuItem } from "$lib/types/menu";
import type { ContextItem } from "$lib/types/context";

/** Display text only, capped at 200 characters; the clipboard value is resolved in Rust. */
export interface LastInteractionChipData {
  preview: string;
}

export type TranscriptionStatus = "ok" | "failed" | "retrying";

export interface TranscriptionChipData {
  status: TranscriptionStatus;
  entry_id: string;
  preview: string | null;
  error: string | null;
  cancelled: boolean;
  has_audio: boolean;
  expires_at: string | null;
}

export interface LastInteractionData {
  input: LastInteractionChipData | null;
  output: LastInteractionChipData | null;
  transcription: TranscriptionChipData | null;
}

export type SectionGroup = {
  sectionId: string;
  startIndex: number;
  items: { item: MenuItem; globalIndex: number }[];
};

export function extractContextItems(item: MenuItem): ContextItem[] | null {
  if (item.item_type !== "context") return null;
  const data = (item.data ?? {}) as { items?: ContextItem[] };
  return data.items ?? [];
}

export function extractLastInteractionData(item: MenuItem): LastInteractionData | null {
  if (item.item_type !== "last_interaction") return null;
  return (item.data ?? null) as LastInteractionData | null;
}

export function groupBySection(items: MenuItem[]): SectionGroup[] {
  const groups: SectionGroup[] = [];
  let currentSection: SectionGroup | null = null;

  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    const sectionId = item.section_id ?? "default";

    if (!currentSection || currentSection.sectionId !== sectionId) {
      currentSection = { sectionId, startIndex: i, items: [] };
      groups.push(currentSection);
    }

    currentSection.items.push({ item, globalIndex: i });
  }

  return groups.filter((g) => g.sectionId !== "models");
}
