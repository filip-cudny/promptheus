import type { Fixture } from "../mocks";
import { appSettings } from "./settings";

export const label = "conversation-dialog";

function skill(name: string, display_name: string, description: string) {
  return { name, display_name, description, offered_in: null };
}

const skills = [
  skill("fix-grammar", "Fix grammar", "Corrects grammar and spelling, keeping the tone."),
  skill("summarize", "Summarize", "Condenses the text into a few bullet points."),
  skill("explain-code", "Explain code", "Explains what a code snippet does, step by step."),
];

const skillsFull = skills.map((s) => ({ ...s, model: null, parameters: null, body: "", file_path: `skills/${s.name}.md` }));

const turns = [
  [
    "/explain-code\ninput.oninput = (e) => search(e.target.value);",
    "The handler calls `search` on **every keystroke**: typing \"weather\" sends seven requests, and a slow early answer can overwrite a newer one.",
  ],
  [
    "make it wait until I stop typing",
    [
      "Debounce the handler, so only the last keystroke within 300 ms runs the search:",
      "",
      "```js",
      "function debounce(fn, ms = 300) {",
      "  let timer;",
      "  return (...args) => {",
      "    clearTimeout(timer);",
      "    timer = setTimeout(() => fn(...args), ms);",
      "  };",
      "}",
      "",
      "input.oninput = debounce((e) => search(e.target.value));",
      "```",
      "",
      "Typing \"weather\" now sends one request, 300 ms after the last key, so no older answer is left to overwrite it.",
    ].join("\n"),
  ],
];

function node(id: string, parent: string | null, role: string, content: string, child: string | null, minute: number) {
  return {
    node_id: id,
    parent_id: parent,
    role,
    content,
    text_attachments: [],
    timestamp: `2026-03-12T09:${String(minute).padStart(2, "0")}:00Z`,
    children: child ? [child] : [],
    updates: [],
    prompt_tokens: role === "assistant" ? 412 : null,
    completion_tokens: role === "assistant" ? 186 : null,
    thinking: null,
    thinking_duration: null,
    query_duration: role === "assistant" ? 3.4 : null,
    error: null,
    cancelled: false,
    tool_calls: [],
    applied_skills: [],
  };
}

const ids = turns.flatMap((_, i) => [`user-${i}`, `assistant-${i}`]);
const nodes = turns.flatMap(([question, answer], i) => [
  node(`user-${i}`, ids[2 * i - 1] ?? null, "user", question, `assistant-${i}`, 2 * i),
  node(`assistant-${i}`, `user-${i}`, "assistant", answer, ids[2 * i + 2] ?? null, 2 * i + 1),
]);

export const chatTitle = "Debouncing search input";

export const chatEntry = {
  id: "entry-chat-1",
  timestamp: "2026-03-12T09:03:00Z",
  input_preview: turns[0][0],
  entry_type: "text",
  output_preview: turns[1][1].split("\n")[0],
  skill_id: null,
  success: true,
  error: null,
  is_multi_turn: true,
  skill_name: null,
  conversation_data: {
    context_text: "",
    nodes,
    root_node_id: ids[0],
    current_path: ids,
    resolved_environment_section: null,
    node_images: {},
    context_images: [],
    model_id: "model-sonnet",
    reasoning_effort: null,
  },
  created_at: "2026-03-12T09:00:00Z",
  updated_at: "2026-03-12T09:03:00Z",
  quick_action: false,
  title: chatTitle,
};

export const fixture: Fixture = {
  get_settings: appSettings,
  list_skills: skills,
  list_skills_full: skillsFull,
  get_chat_corners: false,
  get_dialog_init_params: {
    skill_id: "",
    skill_name: "",
    skill_model: null,
    history_entry_id: chatEntry.id,
    last_interaction_only: false,
    initial_input: null,
    auto_send_input: false,
    new_chat: false,
    invocation_source: null,
    copy_token: null,
  },
  get_history_entry: chatEntry,
  get_conversations: [chatEntry],
  reconnect_to_execution: null,
  count_conversation_tokens: 598,
  get_model_capabilities: { reasoning: { kind: "effort", allowed: ["low", "medium", "high"] } },
  list_mcp_tools: [],
  get_dictation_busy: null,
  has_context: false,
  get_tabs: { tabs: [], active_id: null },
  report_native_tabs: null,
  "plugin:window|is_maximized": false,
};
