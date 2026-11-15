use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::settings::{default_timeout_secs, McpServerConfig, ModelConfig, Settings, WebviewProvider};
use crate::models::skill::SkillFrontmatter;
use crate::services::config::{catalog, PromptKind};

use super::plan::{parse_path, Segment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptFile {
    System,
    AboutYou,
    Environment,
    InputFormat,
    TitleGeneration,
    SpeechToText,
    SttKeyterms,
}

impl PromptFile {
    pub fn prompt_kind(self) -> Option<PromptKind> {
        match self {
            Self::System => Some(PromptKind::System),
            Self::AboutYou => Some(PromptKind::AboutYou),
            Self::Environment => Some(PromptKind::Environment),
            Self::InputFormat => Some(PromptKind::InputFormat),
            Self::TitleGeneration => Some(PromptKind::TitleGeneration),
            Self::SpeechToText => Some(PromptKind::SpeechToText),
            Self::SttKeyterms => None,
        }
    }

    fn label(self) -> &'static str {
        self.prompt_kind()
            .map_or("Speech-to-text keyterms", |kind| kind.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerOperationConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub tool_timeouts: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,
}

impl McpServerOperationConfig {
    pub fn into_server(self, current: Option<&McpServerConfig>) -> McpServerConfig {
        McpServerConfig {
            command: self.command,
            args: self.args,
            env: self.env,
            timeout_secs: self.timeout_secs,
            tool_timeouts: self.tool_timeouts,
            disabled: self.disabled.unwrap_or_else(|| current.is_some_and(|c| c.disabled)),
            disabled_tools: self
                .disabled_tools
                .unwrap_or_else(|| current.map(|c| c.disabled_tools.clone()).unwrap_or_default()),
        }
    }
}

#[cfg(test)]
impl From<McpServerConfig> for McpServerOperationConfig {
    fn from(config: McpServerConfig) -> Self {
        Self {
            command: config.command,
            args: config.args,
            env: config.env,
            timeout_secs: config.timeout_secs,
            tool_timeouts: config.tool_timeouts,
            disabled: Some(config.disabled),
            disabled_tools: Some(config.disabled_tools),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    SetSetting { key: String, value: serde_json::Value },
    AddModel { model: ModelConfig },
    UpdateModel { id: String, model: ModelConfig },
    DeleteModel { id: String },
    CreateSkill { slug: String, frontmatter: SkillFrontmatter, body: String, sample_input: String },
    UpdateSkill { slug: String, frontmatter: SkillFrontmatter, body: String, sample_input: String },
    DeleteSkill { slug: String },
    ReorderSkills { order: Vec<String> },
    WritePrompt { kind: PromptFile, content: String },
    AddMcpServer { name: String, config: McpServerOperationConfig },
    UpdateMcpServer { name: String, config: McpServerOperationConfig },
    DeleteMcpServer { name: String },
    AddWebviewProvider { provider: WebviewProvider },
    UpdateWebviewProvider { id: String, provider: WebviewProvider },
    DeleteWebviewProvider { id: String },
    AddShortcut { action: String, shortcut: String },
    RemoveShortcut { action: String, shortcut: String },
    AddEnvVar {
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        secret: bool,
    },
    UpdateEnvVar {
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        secret: bool,
    },
    RenameEnvVar { name: String, new_name: String },
    DeleteEnvVar { name: String },
    SetEnvValue { name: String },
}

impl Operation {
    pub const NAMES: [&'static str; 22] = [
        "set_setting",
        "add_model",
        "update_model",
        "delete_model",
        "create_skill",
        "update_skill",
        "delete_skill",
        "reorder_skills",
        "write_prompt",
        "add_mcp_server",
        "update_mcp_server",
        "delete_mcp_server",
        "add_webview_provider",
        "update_webview_provider",
        "delete_webview_provider",
        "add_shortcut",
        "remove_shortcut",
        "add_env_var",
        "update_env_var",
        "rename_env_var",
        "delete_env_var",
        "set_env_value",
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::SetSetting { .. } => "set_setting",
            Self::AddModel { .. } => "add_model",
            Self::UpdateModel { .. } => "update_model",
            Self::DeleteModel { .. } => "delete_model",
            Self::CreateSkill { .. } => "create_skill",
            Self::UpdateSkill { .. } => "update_skill",
            Self::DeleteSkill { .. } => "delete_skill",
            Self::ReorderSkills { .. } => "reorder_skills",
            Self::WritePrompt { .. } => "write_prompt",
            Self::AddMcpServer { .. } => "add_mcp_server",
            Self::UpdateMcpServer { .. } => "update_mcp_server",
            Self::DeleteMcpServer { .. } => "delete_mcp_server",
            Self::AddWebviewProvider { .. } => "add_webview_provider",
            Self::UpdateWebviewProvider { .. } => "update_webview_provider",
            Self::DeleteWebviewProvider { .. } => "delete_webview_provider",
            Self::AddShortcut { .. } => "add_shortcut",
            Self::RemoveShortcut { .. } => "remove_shortcut",
            Self::AddEnvVar { .. } => "add_env_var",
            Self::UpdateEnvVar { .. } => "update_env_var",
            Self::RenameEnvVar { .. } => "rename_env_var",
            Self::DeleteEnvVar { .. } => "delete_env_var",
            Self::SetEnvValue { .. } => "set_env_value",
        }
    }

    pub fn label(&self, settings: &Settings) -> String {
        match self {
            Self::SetSetting { key, .. } => setting_label(settings, key),
            Self::AddModel { model } => format!("Add model {}", model.display_name),
            Self::UpdateModel { model, .. } => format!("Update model {}", model.display_name),
            Self::DeleteModel { id } => format!("Delete model {}", model_name(settings, id)),
            Self::CreateSkill { slug, .. } => format!("Create skill {slug}"),
            Self::UpdateSkill { slug, .. } => format!("Update skill {slug}"),
            Self::DeleteSkill { slug } => format!("Delete skill {slug}"),
            Self::ReorderSkills { .. } => "Reorder skills".to_string(),
            Self::WritePrompt { kind, .. } => format!("Write {}", kind.label()),
            Self::AddMcpServer { name, .. } => format!("Add MCP server {name}"),
            Self::UpdateMcpServer { name, .. } => format!("Update MCP server {name}"),
            Self::DeleteMcpServer { name } => format!("Delete MCP server {name}"),
            Self::AddWebviewProvider { provider } => format!("Add web provider {}", provider.name),
            Self::UpdateWebviewProvider { provider, .. } => {
                format!("Update web provider {}", provider.name)
            }
            Self::DeleteWebviewProvider { id } => {
                format!("Delete web provider {}", provider_name(settings, id))
            }
            Self::AddShortcut { action, shortcut } => {
                format!("Add shortcut {shortcut} to {}", action_label(action))
            }
            Self::RemoveShortcut { action, shortcut } => {
                format!("Remove shortcut {shortcut} from {}", action_label(action))
            }
            Self::AddEnvVar { name, .. } => format!("Add variable {name}"),
            Self::UpdateEnvVar { name, secret: false, .. } if is_secret_variable(settings, name) => {
                format!("Update variable {name} (value becomes visible to the agent)")
            }
            Self::UpdateEnvVar { name, .. } => format!("Update variable {name}"),
            Self::RenameEnvVar { name, new_name } => format!("Rename variable {name} to {new_name}"),
            Self::DeleteEnvVar { name } => format!("Delete variable {name}"),
            Self::SetEnvValue { name } => format!("Set value of {name}"),
        }
    }
}

fn is_secret_variable(settings: &Settings, name: &str) -> bool {
    settings.env_vars.iter().any(|var| var.name == name && var.secret)
}

fn model_name<'a>(settings: &'a Settings, id: &'a str) -> &'a str {
    settings
        .models
        .iter()
        .find(|model| model.id == id)
        .map_or(id, |model| model.display_name.as_str())
}

fn provider_name<'a>(settings: &'a Settings, id: &'a str) -> &'a str {
    settings
        .webview_providers
        .iter()
        .find(|provider| provider.id == id)
        .map_or(id, |provider| provider.name.as_str())
}

fn action_label(action: &str) -> &str {
    let key = format!("keymaps.{action}");
    catalog::ENTRIES
        .iter()
        .find(|entry| entry.key == key)
        .map_or(action, |entry| entry.label)
}

fn setting_label(settings: &Settings, key: &str) -> String {
    let catalog_key = catalog::catalog_path(key);
    let Some(entry) = catalog::ENTRIES.iter().find(|entry| entry.key == catalog_key) else {
        return key.to_string();
    };
    if catalog_key == key {
        return entry.label.to_string();
    }
    let item = match parse_path(key).as_slice() {
        [Segment::Key(list), Segment::Item(id), ..] if list == "models" => {
            model_name(settings, id).to_string()
        }
        [Segment::Key(list), Segment::Item(id), ..] if list == "webview_providers" => {
            provider_name(settings, id).to_string()
        }
        _ => return key.to_string(),
    };
    format!("{item}: {}", entry.label)
}
