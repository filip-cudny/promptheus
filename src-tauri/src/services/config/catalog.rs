use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use super::ConfigError;
use crate::models::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Section {
    General,
    Notifications,
    History,
    Sync,
    Models,
    PromptBase,
    Skills,
    McpServers,
    Chat,
    ContextMenu,
    Speech,
    Environment,
    Shortcuts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Bool,
    Integer { min: Option<i64>, max: Option<i64> },
    Text { max_len: Option<usize> },
    Choice { values: &'static [&'static str] },
    List,
    Map,
    Model { model_type: ModelKind },
    Shortcut,
    Composite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Text,
    Stt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applies {
    Live,
    AfterRestart,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CatalogEntry {
    pub key: &'static str,
    pub section: Section,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    pub applies: Applies,
}

const fn live(
    key: &'static str,
    section: Section,
    label: &'static str,
    description: &'static str,
    kind: Kind,
) -> CatalogEntry {
    CatalogEntry { key, section, label, description, kind, applies: Applies::Live }
}

const fn restart(
    key: &'static str,
    section: Section,
    label: &'static str,
    description: &'static str,
    kind: Kind,
) -> CatalogEntry {
    CatalogEntry { key, section, label, description, kind, applies: Applies::AfterRestart }
}

const fn int(min: Option<i64>, max: Option<i64>) -> Kind {
    Kind::Integer { min, max }
}

const TEXT: Kind = Kind::Text { max_len: None };
const SKIN: Kind = Kind::Choice { values: &["color", "mono"] };
const TEXT_MODEL: Kind = Kind::Model { model_type: ModelKind::Text };
const STT_MODEL: Kind = Kind::Model { model_type: ModelKind::Stt };

use Section::*;

pub const ENTRIES: &[CatalogEntry] = &[
    restart("show_tray_icon", General, "Show tray icon", "Shows the app icon in the system tray or menu bar.", Kind::Bool),
    live("launch_at_startup", General, "Launch at startup", "Starts the app automatically when you log in.", Kind::Bool),
    live("theme", General, "Theme", "Switches the app between the dark and the light colour theme.", Kind::Choice { values: &["dark", "light"] }),
    live("appearance.logo", General, "Logo", "Colour or monochrome variant of the app logo, including the dock icon.", SKIN),
    live("appearance.icons", General, "Icons", "Colour or monochrome variant of the icons across the app.", SKIN),
    live("notifications.events.prompt_execution_success", Notifications, "Prompt finished", "Shows a toast when a prompt finishes successfully.", Kind::Bool),
    live("notifications.events.prompt_execution_cancel", Notifications, "Prompt cancelled", "Shows a toast when a running prompt is cancelled.", Kind::Bool),
    live("notifications.events.speech_recording_start", Notifications, "Recording started", "Shows a toast when a speech recording starts.", Kind::Bool),
    live("notifications.events.speech_recording_stop", Notifications, "Recording stopped", "Shows a toast when a speech recording stops.", Kind::Bool),
    live("notifications.events.speech_transcription_success", Notifications, "Transcription finished", "Shows a toast when a transcription completes.", Kind::Bool),
    live("notifications.events.speech_transcription_retry", Notifications, "Transcription retrying", "Shows a toast when a failed transcription is retried.", Kind::Bool),
    live("notifications.events.context_set", Notifications, "Context set", "Shows a toast when the context value is set.", Kind::Bool),
    live("notifications.events.context_append", Notifications, "Context appended", "Shows a toast when text is appended to the context value.", Kind::Bool),
    live("notifications.events.context_cleared", Notifications, "Context cleared", "Shows a toast when the context value is cleared.", Kind::Bool),
    live("notifications.recording_reminder.enabled", Notifications, "Remind me while a recording is running", "Warns you while a speech recording is still running, to catch recordings started by accident or left running.", Kind::Bool),
    live("notifications.recording_reminder.silence_after_secs", Notifications, "Silence trigger after (seconds)", "Minimum recording length before a silence-based reminder can fire.", int(Some(5), None)),
    live("notifications.recording_reminder.silence_window_secs", Notifications, "Silence length (seconds)", "How long the microphone must stay quiet to count as stopped dictating.", int(Some(2), None)),
    live("notifications.recording_reminder.max_interval_secs", Notifications, "Maximum interval (seconds)", "The reminder fires regardless of silence once this much time passes since the recording started or since the last reminder.", int(Some(10), None)),
    live("history_retention_days", History, "Retention", "How many days conversations stay in the local history. 0 keeps everything forever.", int(Some(0), Some(3650))),
    live("preferred_name", PromptBase, "Preferred name", "The name the assistant uses when addressing you.", Kind::Text { max_len: Some(60) }),
    live("models[].model", Models, "Model", "Model identifier sent to the provider API.", TEXT),
    live("models[].display_name", Models, "Display name", "Name shown for the model in menus and settings.", TEXT),
    live("models[].type", Models, "Type", "Whether the model generates text or transcribes speech.", Kind::Choice { values: &["text", "stt"] }),
    live("models[].provider", Models, "Provider", "Service that hosts the model.", Kind::Choice { values: &["openai", "anthropic", "gemini", "elevenlabs"] }),
    live("models[].group", Models, "Group", "Optional group the model is listed under in menus.", TEXT),
    live("models[].api_key", Models, "API key", "Key used to call the provider, or a ${ENV_VAR} reference resolved at use.", TEXT),
    live("models[].base_url", Models, "Base URL", "Custom API endpoint for OpenAI-compatible providers. Must start with http:// or https://.", TEXT),
    live("models[].parameters", Models, "Parameters", "Generation parameters sent with every request to this model.", Kind::Composite),
    live("models[].context_window_size", Models, "Context window size", "Size of the model's context window in tokens.", int(None, None)),
    live("models[].api_mode", Models, "API mode", "Which OpenAI API the model is called through: Responses or Chat Completions.", Kind::Choice { values: &["responses", "completions"] }),
    live("models[].capabilities", Models, "Capabilities", "What the model supports, such as the way reasoning is controlled.", Kind::Composite),
    live("models[].store", Models, "Store", "Lets the provider store requests and responses where it supports this.", Kind::Bool),
    restart("mcp_servers.*.command", McpServers, "Command", "Executable that starts the MCP server.", TEXT),
    restart("mcp_servers.*.args", McpServers, "Arguments", "Command-line arguments passed to the server.", Kind::List),
    restart("mcp_servers.*.env", McpServers, "Environment", "Environment variables set for the server process.", Kind::Map),
    restart("mcp_servers.*.timeout_secs", McpServers, "Timeout (seconds)", "Default time a tool call may take before it is abandoned.", int(None, None)),
    restart("mcp_servers.*.tool_timeouts", McpServers, "Tool timeouts", "Per-tool timeouts in seconds that override the server default.", Kind::Map),
    restart("mcp_servers.*.disabled", McpServers, "Disabled", "Turned off: the server is not started at the next app start; its configuration and tool settings are kept.", Kind::Bool),
    live("mcp_servers.*.disabled_tools", McpServers, "Disabled tools", "Tools of this server that are offered nowhere and sent to no model.", Kind::List),
    live("surfaces.chat.generation.model_id", Chat, "Chat model", "Model used for chat conversations.", TEXT_MODEL),
    live("surfaces.chat.generation.parameters.reasoning_effort", Chat, "Reasoning effort", "How much reasoning the chat model spends on a reply, where the model supports it.", TEXT),
    live("surfaces.chat.generation.enabled_tools", Chat, "Enabled tools", "Tools the assistant may call in chat.", Kind::List),
    live("surfaces.title_generation.generation.model_id", Chat, "Title model", "Model used to name conversations. Falls back to the chat model when empty.", TEXT_MODEL),
    live("surfaces.settings_agent.generation.model_id", Chat, "Settings agent model", "Model used by the settings agent. Falls back to the chat model when empty.", TEXT_MODEL),
    live("surfaces.settings_agent.generation.parameters.reasoning_effort", Chat, "Settings agent reasoning effort", "How much reasoning the settings agent model spends on a reply, where the model supports it. Falls back to the chat's value when empty.", TEXT),
    live("surfaces.settings_agent.generation.enabled_tools", Chat, "Settings agent tools", "Tools the settings agent may call: MCP tools as `server.tool`, and `web_search` for the provider's built-in web search (OpenAI Responses models), e.g. to check current model ids.", Kind::List),
    live("webview_providers[].name", Chat, "Provider name", "Name of a web chat provider shown in the app.", TEXT),
    live("webview_providers[].url", Chat, "Provider URL", "Address of a web chat provider. Must use http or https.", TEXT),
    live("surfaces.quick_actions.generation.model_id", ContextMenu, "Quick actions model", "Model used for prompts run from the context menu. Falls back to the chat model when empty.", TEXT_MODEL),
    live("surfaces.quick_actions.generation.parameters.reasoning_effort", ContextMenu, "Reasoning effort", "How much reasoning the quick actions model spends on a reply, where the model supports it.", TEXT),
    live("menu_section_order", ContextMenu, "Menu section order", "Order of the sections in the context menu.", Kind::List),
    live("recent_apps_count", ContextMenu, "Recent apps", "How many recently used apps the context menu remembers.", int(Some(0), None)),
    live("menu_digit_select_delay_ms", ContextMenu, "Digit selection delay", "How long the menu waits for a second digit when a two-digit number is possible.", int(Some(100), Some(1000))),
    live("surfaces.speech_to_text.model_id", Speech, "Speech model", "Model used to transcribe recordings.", STT_MODEL),
    live("surfaces.speech_to_text.language", Speech, "Language", "Language hint sent to the transcription model.", TEXT),
    live("surfaces.speech_to_text.no_verbatim", Speech, "No verbatim", "Asks the transcription model to clean up filler words and false starts.", Kind::Bool),
    live("surfaces.speech_to_text.auto_retry_attempts", Speech, "Transcription attempts", "Total transcription attempts per recording, including the first. 1 disables background retrying.", int(Some(1), Some(10))),
    live("surfaces.speech_to_text.audio_retention_hours", Speech, "Keep audio for (hours)", "How long a recording stays on disk so a failed transcription can be replayed. 0 discards audio immediately.", int(Some(0), Some(72))),
    live("surfaces.speech_to_text.keep_audio_on_success", Speech, "Keep audio after a successful transcription", "Keeps the recording for the retention period even when transcription succeeds.", Kind::Bool),
    live("surfaces.speech_to_text.show_recording_widget", Speech, "Show recording widget", "Shows the recording pill while recording.", Kind::Bool),
    live("surfaces.speech_to_text.recording_widget_at_cursor", Speech, "Open recording widget at the cursor", "Opens the recording pill next to the mouse cursor instead of a fixed place.", Kind::Bool),
    live("surfaces.speech_to_text.recording_widget_anchor", Speech, "Recording widget anchor", "Where the recording pill sits relative to its anchor point.", Kind::Choice { values: &["above_right", "above_center", "above_left", "below_right", "below_center", "below_left"] }),
    live("surfaces.speech_to_text.claude_target", Speech, "Claude target", "Where a transcription sent to Claude goes: the Claude webview or Claude Desktop.", Kind::Choice { values: &["webview", "desktop"] }),
    live("surfaces.speech_to_text.stop_chat_target", Speech, "Stop + chat target", "Where Stop + chat sends the transcription: the app chat or Claude.", Kind::Choice { values: &["chat", "claude"] }),
    live("keymaps.open_context_menu", Shortcuts, "Open context menu", "Global shortcut that opens the context menu.", Kind::Shortcut),
    live("keymaps.execute_active_prompt", Shortcuts, "Run active prompt", "Global shortcut that runs the active prompt.", Kind::Shortcut),
    live("keymaps.set_context_value", Shortcuts, "Set context", "Global shortcut that sets the context value from the selection.", Kind::Shortcut),
    live("keymaps.append_context_value", Shortcuts, "Append to context", "Global shortcut that appends the selection to the context value.", Kind::Shortcut),
    live("keymaps.clear_context", Shortcuts, "Clear context", "Global shortcut that clears the context value.", Kind::Shortcut),
    live("keymaps.speech_to_text_toggle", Shortcuts, "Toggle speech to text", "Global shortcut that starts and stops a recording.", Kind::Shortcut),
    live("env_vars[].name", Environment, "Name", "Name of the environment variable, referenced as ${NAME} in settings.", TEXT),
    live("env_vars[].description", Environment, "Description", "What the variable is used for.", TEXT),
    live("env_vars[].secret", Environment, "Secret", "Masks the value wherever it is shown.", Kind::Bool),
    live("env_vars[].renamed_from", Environment, "Previous names", "Earlier names of the variable. Each machine copies a missing value from the first previous name that has one.", Kind::List),
];

#[cfg(test)]
pub const FILE_ONLY: &[&str] = &[
    "autosave_debounce_ms",
    "prompt_base.system",
    "prompt_base.about_you",
    "prompt_base.environment",
    "prompt_base.input_format",
    "surfaces.title_generation.prompt",
    "surfaces.speech_to_text.prompt",
    "surfaces.speech_to_text.keyterms_file",
    "skills_order",
    "keymaps[].context",
    "models[].id",
    "webview_providers[].id",
    "surfaces.chat.generation.parameters.temperature",
    "surfaces.chat.generation.parameters.max_tokens",
    "surfaces.chat.generation.parameters.top_p",
    "surfaces.chat.generation.parameters.frequency_penalty",
    "surfaces.chat.generation.parameters.presence_penalty",
    "surfaces.quick_actions.generation.parameters.temperature",
    "surfaces.quick_actions.generation.parameters.max_tokens",
    "surfaces.quick_actions.generation.parameters.top_p",
    "surfaces.quick_actions.generation.parameters.frequency_penalty",
    "surfaces.quick_actions.generation.parameters.presence_penalty",
    "surfaces.settings_agent.generation.parameters.temperature",
    "surfaces.settings_agent.generation.parameters.max_tokens",
    "surfaces.settings_agent.generation.parameters.top_p",
    "surfaces.settings_agent.generation.parameters.frequency_penalty",
    "surfaces.settings_agent.generation.parameters.presence_penalty",
];

const OPAQUE: &[&str] = &[
    "models[].parameters",
    "models[].capabilities",
    "mcp_servers.*.env",
    "mcp_servers.*.tool_timeouts",
    "keymaps[].bindings",
];

#[cfg(test)]
const KEYMAP_ACTIONS: [&str; 6] = [
    "open_context_menu",
    "speech_to_text_toggle",
    "execute_active_prompt",
    "set_context_value",
    "append_context_value",
    "clear_context",
];

pub fn catalog_path(concrete: &str) -> String {
    let mut path = String::with_capacity(concrete.len());
    let mut in_index = false;
    for c in concrete.chars() {
        match c {
            '[' => {
                in_index = true;
                path.push_str("[]");
            }
            ']' => in_index = false,
            _ if !in_index => path.push(c),
            _ => {}
        }
    }
    match path.strip_prefix("mcp_servers.") {
        Some(tail) => match tail.split_once('.') {
            Some((_, rest)) => format!("mcp_servers.*.{rest}"),
            None => "mcp_servers.*".to_string(),
        },
        None => path,
    }
}

#[cfg(test)]
fn has_entry(key: &str) -> bool {
    ENTRIES.iter().any(|e| e.key == key)
}

#[cfg(test)]
fn is_covered(leaf: &str) -> bool {
    if leaf == "keymaps[].bindings" {
        return KEYMAP_ACTIONS.iter().all(|a| has_entry(&format!("keymaps.{a}")));
    }
    has_entry(leaf) || FILE_ONLY.contains(&leaf)
}

fn collect_leaves<'a>(value: &'a Value, path: &str, out: &mut HashMap<String, &'a Value>) {
    let join = |segment: &str| {
        if path.is_empty() { segment.to_string() } else { format!("{path}.{segment}") }
    };
    match value {
        Value::Object(_) if OPAQUE.contains(&catalog_path(path).as_str()) => {
            out.insert(path.to_string(), value);
        }
        Value::Object(map) => {
            for (name, item) in map {
                collect_leaves(item, &join(name), out);
            }
        }
        Value::Array(items) if items.iter().any(Value::is_object) => {
            for (index, item) in items.iter().enumerate() {
                let name = ["id", "context", "name"]
                    .iter()
                    .find_map(|field| item.get(field).and_then(Value::as_str))
                    .map_or_else(|| index.to_string(), str::to_string);
                collect_leaves(item, &format!("{path}[{name}]"), out);
            }
        }
        _ => {
            out.insert(path.to_string(), value);
        }
    }
}

#[cfg(test)]
pub fn uncovered_leaves(value: &Value) -> Vec<String> {
    let mut leaves = HashMap::new();
    collect_leaves(value, "", &mut leaves);
    let mut uncovered: Vec<String> = leaves
        .keys()
        .map(|key| catalog_path(key))
        .filter(|leaf| !is_covered(leaf))
        .collect();
    uncovered.sort();
    uncovered.dedup();
    uncovered
}

fn check_value(key: &str, kind: Kind, value: &Value) -> Result<(), ConfigError> {
    let violation = match kind {
        Kind::Integer { min, max } => value.as_i64().and_then(|n| match (min, max) {
            (Some(lo), _) if n < lo => Some(format!("must be at least {lo}")),
            (_, Some(hi)) if n > hi => Some(format!("must be at most {hi}")),
            _ => None,
        }),
        Kind::Text { max_len: Some(limit) } => value
            .as_str()
            .filter(|text| text.chars().count() > limit)
            .map(|_| format!("must be at most {limit} characters")),
        Kind::Choice { values } => value
            .as_str()
            .filter(|text| !values.contains(text))
            .map(|_| format!("must be one of: {}", values.join(", "))),
        _ => None,
    };
    match violation {
        Some(reason) => Err(ConfigError::invalid_at(key, format!("{key} {reason}"))),
        None => Ok(()),
    }
}

pub fn check_changed(before: &Settings, after: &Settings) -> Result<(), ConfigError> {
    let (before, after) = (serde_json::to_value(before)?, serde_json::to_value(after)?);
    let (mut old_leaves, mut new_leaves) = (HashMap::new(), HashMap::new());
    collect_leaves(&before, "", &mut old_leaves);
    collect_leaves(&after, "", &mut new_leaves);
    let mut changed: Vec<_> = new_leaves
        .into_iter()
        .filter(|(key, value)| old_leaves.get(key) != Some(value))
        .collect();
    changed.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in changed {
        let path = catalog_path(&key);
        if let Some(entry) = ENTRIES.iter().find(|e| e.key == path) {
            check_value(&key, entry.kind, value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::capabilities::{Effort, ModelCapabilities, ReasoningMode};
    use crate::models::settings::{
        ApiMode, EnvVar, KeymapGroup, McpServerConfig, ModelConfig, ModelParameters, ModelType,
        Provider, Settings, WebviewProvider,
    };
    use std::collections::HashMap;

    fn full_parameters() -> ModelParameters {
        ModelParameters {
            temperature: Some(0.5),
            max_tokens: Some(100),
            top_p: Some(1.0),
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            reasoning_effort: Some("low".into()),
            ..Default::default()
        }
    }

    fn fixture() -> Settings {
        let mut settings = Settings::default();
        settings.models = vec![ModelConfig {
            id: "abc".into(),
            model: "gpt".into(),
            display_name: "GPT".into(),
            model_type: ModelType::Text,
            provider: Some(Provider::Openai),
            group: Some("g".into()),
            api_key: Some("k".into()),
            base_url: Some("https://x".into()),
            parameters: Some(full_parameters()),
            context_window_size: Some(1000),
            api_mode: Some(ApiMode::Responses),
            capabilities: Some(ModelCapabilities {
                reasoning: ReasoningMode::Effort { allowed: vec![Effort::Low] },
            }),
            store: true,
        }];
        settings.mcp_servers = HashMap::from([(
            "my-tools".to_string(),
            McpServerConfig {
                command: "c".into(),
                args: vec!["a".into()],
                env: HashMap::from([("K".to_string(), "V".to_string())]),
                timeout_secs: 5,
                tool_timeouts: HashMap::from([("t".to_string(), 3)]),
                disabled: true,
                disabled_tools: vec!["t".into()],
            },
        )]);
        settings.keymaps = vec![KeymapGroup {
            context: "global".into(),
            bindings: HashMap::from([("open_context_menu".to_string(), "cmd+k".to_string())]),
        }];
        settings.webview_providers = vec![WebviewProvider {
            id: "claude".into(),
            name: "Claude".into(),
            url: "https://claude.ai/".into(),
        }];
        settings.env_vars = vec![EnvVar {
            name: "OPENAI_API_KEY".into(),
            description: "d".into(),
            secret: true,
            renamed_from: vec!["OLD_KEY".into()],
        }];
        settings.surfaces.chat.generation.model_id = Some("abc".into());
        settings.surfaces.chat.generation.parameters = full_parameters();
        settings.surfaces.quick_actions.generation.model_id = Some("abc".into());
        settings.surfaces.quick_actions.generation.parameters = full_parameters();
        settings.surfaces.title_generation.generation.model_id = Some("abc".into());
        settings.surfaces.settings_agent.generation.model_id = Some("abc".into());
        settings.surfaces.settings_agent.generation.parameters = full_parameters();
        settings.surfaces.settings_agent.generation.enabled_tools = vec!["srv.tool".into()];
        let stt = &mut settings.surfaces.speech_to_text;
        stt.model_id = Some("abc".into());
        stt.language = Some("en".into());
        stt.keyterms_file = Some("k.md".into());
        stt.no_verbatim = Some(true);
        stt.prompt = Some("p.md".into());
        settings
    }

    #[test]
    fn fixture_has_no_uncovered_leaves() {
        let value = serde_json::to_value(fixture()).unwrap();
        assert_eq!(uncovered_leaves(&value), Vec::<String>::new());
    }

    #[test]
    fn injected_leaf_is_reported() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value["surfaces"]["chat"]["new_key"] = serde_json::json!(1);
        assert_eq!(uncovered_leaves(&value), vec!["surfaces.chat.new_key"]);
    }

    #[test]
    fn entries_are_unique_and_not_file_only() {
        let mut seen = std::collections::HashSet::new();
        for entry in ENTRIES {
            assert!(seen.insert(entry.key), "duplicate entry {}", entry.key);
            assert!(!FILE_ONLY.contains(&entry.key), "{} is also file-only", entry.key);
        }
    }

    #[test]
    fn only_keymap_entries_are_in_shortcuts_section() {
        for entry in ENTRIES {
            assert_eq!(
                entry.section == Shortcuts,
                entry.key.starts_with("keymaps."),
                "{}",
                entry.key
            );
        }
        let shortcuts = ENTRIES.iter().filter(|e| e.section == Shortcuts).count();
        assert_eq!(shortcuts, 6);
    }

    #[test]
    fn catalog_path_normalizes_concrete_paths() {
        assert_eq!(catalog_path("models[abc].model"), "models[].model");
        assert_eq!(catalog_path("mcp_servers.my-tools.env"), "mcp_servers.*.env");
        assert_eq!(catalog_path("theme"), "theme");
    }
}
