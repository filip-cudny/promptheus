import type { Fixture } from "../mocks";
import { chatEntry } from "./conversation";

export const label = "history-dialog";

type Entry = Record<string, unknown>;

function quickAction(
  id: string,
  timestamp: string,
  skill: [string, string] | null,
  input_preview: string,
  output_preview: string,
): Entry {
  return {
    ...chatEntry,
    id,
    timestamp,
    created_at: timestamp,
    updated_at: timestamp,
    input_preview,
    output_preview,
    entry_type: skill ? "text" : "speech",
    skill_id: skill?.[0] ?? null,
    skill_name: skill?.[1] ?? null,
    is_multi_turn: false,
    conversation_data: null,
    quick_action: true,
    title: null,
  };
}

const entries: Entry[] = [
  quickAction(
    "entry-speech-2",
    "2026-03-12T11:20:00Z",
    null,
    "Add a search field to the settings page before the Friday demo.",
    "Add a search field to the settings page before the Friday demo.",
  ),
  chatEntry,
  quickAction(
    "entry-grammar-1",
    "2026-03-11T16:45:00Z",
    ["fix-grammar", "Fix grammar"],
    "Can you search the shared drive for last years invoices and send me them?",
    "Can you search the shared drive for last year's invoices and send them to me?",
  ),
  quickAction(
    "entry-summary-1",
    "2026-03-11T10:05:00Z",
    ["summarize", "Summarize"],
    "Interview notes, round two: most people open the customer list and scroll instead of using filters.",
    "- Most users search by customer name first and never open the filter panel.\n- Saved searches are the most requested feature.",
  ),
  quickAction(
    "entry-explain-1",
    "2026-03-10T14:30:00Z",
    ["explain-code", "Explain code"],
    "const page = await api.customers.list({ name, limit: 20 });",
    "Fetches the first 20 customers whose name matches, the same request the search box sends.",
  ),
];

const fields = ["title", "skill_name", "input_preview", "output_preview"] as const;

function matchesFor(entry: Entry, text: string) {
  return fields.flatMap((field) => {
    const value = String(entry[field] ?? "").toLowerCase();
    const indices: number[] = [];
    for (let at = value.indexOf(text); text && at >= 0; at = value.indexOf(text, at + text.length)) {
      for (let i = at; i < at + text.length; i++) indices.push(i);
    }
    return indices.length ? [{ field, indices }] : [];
  });
}

function search({ query: request }: Record<string, unknown>) {
  const text = String((request as { query: string }).query).toLowerCase();
  const results = entries
    .map((entry) => ({ entry, matches: matchesFor(entry, text), score: 1 }))
    .filter((result) => !text || result.matches.length);
  return { results, total: results.length };
}

export const fixture: Fixture = {
  search_history: search,
  list_history_skills: [
    { skill_id: "fix-grammar", skill_name: "Fix grammar", count: 1 },
    { skill_id: "summarize", skill_name: "Summarize", count: 1 },
    { skill_id: "explain-code", skill_name: "Explain code", count: 1 },
  ],
  get_ui_state: null,
  get_audio_clip_info: { has_audio: false, expires_at: null, duration_secs: null, bytes: null, is_retrying: false },
};
