import { emit } from "@tauri-apps/api/event";
import type { Fixture } from "../mocks";
import { appSettings } from "./settings";

export const label = "context-menu";

function item(id: string, label: string, item_type: string, section_id: string, extra: Record<string, unknown> = {}) {
  return {
    id,
    label,
    item_type,
    data: null,
    enabled: true,
    separator_after: false,
    tooltip: null,
    icon: null,
    section_id,
    ...extra,
  };
}

function prompt(id: string, label: string, tooltip: string) {
  return item(id, label, "skill", "skills", { data: { skill_id: id, skill_name: label }, tooltip });
}

const menuItems = [
  item("__chat__", "Chat", "chat", "chat", { separator_after: true }),
  prompt("fix-grammar", "Fix grammar", "Corrects grammar and spelling, keeping the tone."),
  prompt("to-english", "Translate to English", "Translates the text into natural English."),
  prompt("summarize", "Summarize", "Condenses the text into a few bullet points."),
  { ...prompt("explain-code", "Explain code", "Explains what a code snippet does, step by step."), separator_after: true },
  item("system_speech_to_text", "Speech to text", "speech", "speech", {
    data: { type: "speech_to_text" },
    icon: "mic",
    separator_after: true,
  }),
  item("last_interaction_section", "Last interaction", "last_interaction", "lastInteraction", {
    separator_after: true,
    data: {
      input: { preview: "Our team are meeting tomorrow for discuss the roadmap." },
      output: { preview: "Our team is meeting tomorrow to discuss the roadmap." },
      transcription: {
        status: "ok",
        entry_id: "entry-speech-1",
        preview: "Remind me to send the slides before the call.",
        error: null,
        has_audio: false,
        expires_at: null,
      },
      last_text_entry: { id: "entry-text-1", skill_id: "fix-grammar", skill_name: "Fix grammar" },
    },
  }),
  item("environment_section", "Context", "context", "context", {
    separator_after: true,
    data: {
      items: [{ item_type: "text", number: 1, content: "Release notes draft for version 2.4: faster startup, new themes." }],
    },
  }),
  item("settings_section", "Settings", "settings_section", "settings"),
];

export const fixture: Fixture = {
  get_settings: appSettings,
  get_context_menu_items: menuItems,
  get_recording_state: { is_recording: false, is_transcribing: false, action_id: null },
  get_webview_providers: appSettings.webview_providers,
  clamp_menu_origin: [0, 0],
  "plugin:window|set_size": null,
  "plugin:window|set_position": null,
  show_context_menu_panel: null,
  focus_context_menu: null,
};

export async function show() {
  await emit("show-context-menu", {
    cursor_x: 0,
    cursor_y: 0,
    work_x: 0,
    work_y: 0,
    work_width: 320,
    work_height: 400,
    shell_placement: false,
  });
}
