import type { Channel } from "@tauri-apps/api/core";
import catalog from "virtual:screenshots/catalog";
import type { Fixture } from "../mocks";

export const label = "settings-dialog";

const parameters = {
  temperature: null,
  max_tokens: null,
  top_p: null,
  frequency_penalty: null,
  presence_penalty: null,
  reasoning_effort: null,
};

function model(id: string, display_name: string, modelName: string, type: string, provider: string, api_key: string) {
  return {
    id,
    model: modelName,
    display_name,
    type,
    provider,
    group: null,
    api_key,
    base_url: null,
    parameters: null,
    context_window_size: null,
    api_mode: null,
    capabilities: null,
    store: false,
  };
}

export const appSettings = {
  show_tray_icon: true,
  launch_at_startup: true,
  theme: "dark",
  menu_section_order: ["chat", "skills", "speech", "lastInteraction", "context", "settings"],
  appearance: { logo: "color", icons: "color" },
  notifications: {
    events: {
      prompt_execution_success: true,
      prompt_execution_cancel: true,
      speech_recording_start: true,
      speech_recording_stop: true,
      speech_transcription_success: true,
      speech_transcription_retry: true,
      context_set: true,
      context_append: true,
      context_cleared: true,
    },
    recording_reminder: { enabled: true, silence_after_secs: 60, silence_window_secs: 8, max_interval_secs: 180 },
  },
  autosave_debounce_ms: 1000,
  history_retention_days: 0,
  models: [
    model("model-gpt", "GPT-5", "gpt-5", "text", "openai", "${OPENAI_API_KEY}"),
    model("model-sonnet", "Claude Sonnet", "claude-sonnet", "text", "anthropic", "${ANTHROPIC_API_KEY}"),
    model("model-gemini", "Gemini Flash", "gemini-flash", "text", "gemini", "${GEMINI_API_KEY}"),
    model("model-whisper", "Whisper", "whisper-1", "stt", "openai", "${OPENAI_API_KEY}"),
  ],
  preferred_name: "Alex",
  prompt_base: {
    system: "prompts/base/system.md",
    about_you: "prompts/base/about_you.md",
    environment: "prompts/base/environment.md",
    input_format: "prompts/base/input_format.md",
  },
  surfaces: {
    chat: { generation: { model_id: "model-gpt", parameters, enabled_tools: [] } },
    quick_actions: { generation: { model_id: "model-sonnet", parameters } },
    title_generation: { generation: { model_id: "model-gemini" }, prompt: "prompts/surfaces/title_generation.md" },
    settings_agent: { generation: { model_id: "model-gpt", parameters, enabled_tools: [] } },
    speech_to_text: {
      model_id: "model-whisper",
      language: "en",
      keyterms_file: null,
      no_verbatim: true,
      prompt: null,
      auto_retry_attempts: 3,
      audio_retention_hours: 24,
      keep_audio_on_success: false,
      show_recording_widget: true,
      recording_widget_at_cursor: true,
      recording_widget_anchor: "above_right",
      claude_target: "webview",
      stop_chat_target: "chat",
    },
  },
  keymaps: [{ context: "global", bindings: { open_context_menu: "Ctrl+Alt+Space" } }],
  recent_apps_count: 4,
  menu_digit_select_delay_ms: 200,
  skills_order: [],
  webview_providers: [
    { id: "chatgpt", name: "ChatGPT", url: "https://chatgpt.com" },
    { id: "claude", name: "Claude", url: "https://claude.ai" },
  ],
  env_vars: [
    { name: "OPENAI_API_KEY", description: "OpenAI API key", secret: true, renamed_from: [] },
    { name: "ANTHROPIC_API_KEY", description: "Anthropic API key", secret: true, renamed_from: [] },
    { name: "GEMINI_API_KEY", description: "Google Gemini API key", secret: true, renamed_from: [] },
  ],
};

const agentReply = [
  "Quick actions use **Claude Sonnet**; the chat and this agent use **GPT-5**.",
  "",
  "To switch, pick another model in the *Quick actions* row of **Assignments**. Say \"use GPT-5 for quick actions\" and I prepare the change for you to apply.",
].join("\n");

export const globals = { __settingsInitialSection: "models", __settingsAgentPanel: "widened" };

export const fixture: Fixture = {
  get_settings: appSettings,
  get_settings_catalog: catalog,
  get_settings_fallback: null,
  get_setup_status: { active: false, step: 5 },
  get_missing_env_refs: [],
  get_model_capabilities: { reasoning: { kind: "effort", allowed: ["low", "medium", "high"] } },
  list_skills: [],
  list_skills_full: [],
  settings_agent_reset: null,
  get_ui_state: null,
  get_dictation_busy: null,
  settings_agent_send: ({ onEvent }) => {
    const channel = onEvent as Channel<unknown>;
    channel.onmessage({
      event: "done",
      data: { full_text: agentReply, full_thinking: null, prompt_tokens: null, completion_tokens: null },
    });
  },
};

export async function show() {
  const { getSettingsAgent } = await import("$lib/stores/settingsAgent.svelte");
  await getSettingsAgent().send("Which model do my quick actions use?");
}

