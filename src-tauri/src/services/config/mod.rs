pub mod catalog;
mod defaults;
mod loader;
mod migrator;
mod path;
mod prompts;
#[cfg(test)]
mod tests;

pub use prompts::{PromptKind, PromptStore};

use std::path::{Path, PathBuf};

use crate::models::settings::{
    EnvVar, KeymapGroup, ModelConfig, NotificationSettings, Settings, SpeechToTextConfig,
};
use crate::services::secrets;
use crate::services::hotkeys::{matches_current_os, translate_shortcut};

pub(crate) use path::resolve_config_relative;

use crate::services::fs_util::write_atomic;

pub(crate) use defaults::validate;
use defaults::ensure_surface_defaults;
use loader::initialize_defaults;
use migrator::{
    migrate_explicit_capabilities, migrate_model_params, parse_and_migrate,
    replace_legacy_system_default, sanitize_capabilities, InlinePromptWrite, LegacyFlatMdRename,
};

pub use loader::load_env;

const SHORTCUT_ACTIONS: [&str; 6] = [
    "open_context_menu",
    "speech_to_text_toggle",
    "execute_active_prompt",
    "set_context_value",
    "append_context_value",
    "clear_context",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Settings file not found: {0}")]
    FileNotFound(String),
    #[error("{message}{}", key.as_ref().map(|k| format!(" (setting: {k})")).unwrap_or_default())]
    InvalidSettings { key: Option<String>, message: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
}

impl ConfigError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidSettings { key: None, message: message.into() }
    }

    pub fn invalid_at(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidSettings { key: Some(key.into()), message: message.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Chat,
    QuickActions,
    TitleGeneration,
    SpeechToText,
    SettingsAgent,
}

const MAX_PREFERRED_NAME_LEN: usize = 60;

#[derive(Debug, Clone)]
pub struct ConfigService {
    settings: Settings,
    config_dir: PathBuf,
    baseline: Vec<u8>,
}

#[derive(Debug)]
pub struct Mutated<R> {
    pub value: R,
    pub reloaded: bool,
}

impl ConfigService {
    pub(crate) fn detached(settings: Settings, config_dir: &Path) -> Self {
        ConfigService {
            settings,
            config_dir: config_dir.to_path_buf(),
            baseline: Vec::new(),
        }
    }

    pub fn load(config_dir: &Path) -> Result<Self, ConfigError> {
        load_env(config_dir);

        let settings_path = config_dir.join("settings.json");
        if !settings_path.exists() {
            initialize_defaults(config_dir)?;
        }

        let content = std::fs::read_to_string(&settings_path)?;
        let settings = build_settings(config_dir, &content)?;

        let mut service = ConfigService {
            settings,
            config_dir: config_dir.to_path_buf(),
            baseline: Vec::new(),
        };

        service.write()?;

        Ok(service)
    }

    #[cfg(test)]
    fn save(&self) -> Result<(), ConfigError> {
        let json = serde_json::to_string_pretty(&self.settings)?;
        let settings_path = self.config_dir.join("settings.json");
        std::fs::write(&settings_path, json)?;
        Ok(())
    }

    fn write(&mut self) -> Result<(), ConfigError> {
        let json = serde_json::to_string_pretty(&self.settings)?;
        let settings_path = self.config_dir.join("settings.json");
        std::fs::write(&settings_path, &json)?;
        self.baseline = json.into_bytes();
        Ok(())
    }

    pub fn reload_if_changed(&mut self) -> Result<bool, ConfigError> {
        let settings_path = self.config_dir.join("settings.json");
        if std::fs::read(&settings_path)? == self.baseline {
            return Ok(false);
        }
        self.reload()?;
        log::info!("settings.json changed outside the app and was reloaded");
        Ok(true)
    }

    pub fn mutate<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Result<Mutated<R>, ConfigError> {
        self.try_mutate(|service| Ok(f(service)))
    }

    pub fn try_mutate<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<R, ConfigError>,
    ) -> Result<Mutated<R>, ConfigError> {
        let stale_baseline = self.baseline.clone();
        let reloaded = self.reload_if_changed()?;
        let before = self.settings.clone();
        let outcome = f(self).and_then(|value| {
            validate(&self.settings)?;
            catalog::check_changed(&before, &self.settings)?;
            Ok(value)
        });
        match outcome {
            Ok(value) => {
                self.write()?;
                Ok(Mutated { value, reloaded })
            }
            Err(e) => {
                self.settings = before;
                if reloaded {
                    self.baseline = stale_baseline;
                }
                if let ConfigError::InvalidSettings { key, .. } = &e {
                    log::debug!("settings write rejected (setting: {key:?})");
                }
                Err(e)
            }
        }
    }

    pub fn reload(&mut self) -> Result<(), ConfigError> {
        let settings_path = self.config_dir.join("settings.json");
        if !settings_path.exists() {
            return Err(ConfigError::FileNotFound(
                settings_path.display().to_string(),
            ));
        }

        let bytes = std::fs::read(&settings_path)?;
        let content = std::str::from_utf8(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.settings = build_settings(&self.config_dir, content)?;
        self.baseline = bytes;
        log::debug!("settings reloaded");

        Ok(())
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn replace_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn update_setting(
        &mut self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ConfigError> {
        let wrong_type = || ConfigError::invalid_at(key, "value has the wrong type");
        let invalid = |e: serde_json::Error| ConfigError::invalid_at(key, e.to_string());
        let settings = &mut self.settings;
        match key {
            "show_tray_icon" => settings.show_tray_icon = value.as_bool().ok_or_else(wrong_type)?,
            "launch_at_startup" => {
                settings.launch_at_startup = value.as_bool().ok_or_else(wrong_type)?
            }
            "theme" => settings.theme = value.as_str().ok_or_else(wrong_type)?.to_string(),
            "autosave_debounce_ms" => {
                settings.autosave_debounce_ms = value.as_u64().ok_or_else(wrong_type)? as u32
            }
            "history_retention_days" => {
                settings.history_retention_days =
                    value.as_u64().ok_or_else(wrong_type)?.min(3650) as u32
            }
            "recent_apps_count" => {
                settings.recent_apps_count = value.as_u64().ok_or_else(wrong_type)? as usize
            }
            "menu_digit_select_delay_ms" => {
                settings.menu_digit_select_delay_ms = value.as_u64().ok_or_else(wrong_type)? as u32
            }
            "preferred_name" => {
                let trimmed = value.as_str().ok_or_else(wrong_type)?.trim();
                settings.preferred_name = trimmed.chars().take(MAX_PREFERRED_NAME_LEN).collect();
            }
            "appearance" => settings.appearance = serde_json::from_value(value).map_err(invalid)?,
            "menu_section_order" => {
                settings.menu_section_order = serde_json::from_value(value).map_err(invalid)?
            }
            "mcp_servers" => settings.mcp_servers = serde_json::from_value(value).map_err(invalid)?,
            "webview_providers" => {
                settings.webview_providers = serde_json::from_value(value).map_err(invalid)?
            }
            "env_vars" => settings.env_vars = serde_json::from_value(value).map_err(invalid)?,
            _ => return Err(ConfigError::invalid_at(key, "unknown setting")),
        }
        Ok(())
    }

    pub fn add_shortcut(&mut self, action: &str, shortcut: &str) -> Result<(), ConfigError> {
        self.add_shortcut_for_os(std::env::consts::OS, action, shortcut)
    }

    pub fn remove_shortcut(&mut self, action: &str, shortcut: &str) -> Result<(), ConfigError> {
        self.remove_shortcut_for_os(std::env::consts::OS, action, shortcut)
    }

    pub fn add_shortcut_for_os(
        &mut self,
        os: &str,
        action: &str,
        shortcut: &str,
    ) -> Result<(), ConfigError> {
        let key = format!("keymaps.{action}");
        if !SHORTCUT_ACTIONS.contains(&action) {
            return Err(ConfigError::invalid_at(key, "unknown shortcut action"));
        }
        if translate_shortcut(shortcut, os).is_none() {
            return Err(ConfigError::invalid_at(key, format!("{shortcut} is not a valid shortcut")));
        }
        let keymaps = &mut self.settings.keymaps;
        let bound_actions: Vec<&str> = keymaps
            .iter()
            .filter(|g| matches_current_os(&g.context, os))
            .flat_map(|g| g.bindings.iter())
            .filter(|(bound, _)| bound.eq_ignore_ascii_case(shortcut))
            .map(|(_, bound_action)| bound_action.as_str())
            .collect();
        if let Some(other) = bound_actions.iter().find(|bound_action| **bound_action != action) {
            return Err(ConfigError::invalid_at(
                key,
                format!("{shortcut} is already used by {other}"),
            ));
        }
        if !bound_actions.is_empty() {
            return Ok(());
        }
        let index = match keymaps.iter().position(|g| matches_current_os(&g.context, os)) {
            Some(index) => index,
            None => {
                keymaps.push(KeymapGroup {
                    context: format!("os == {os}"),
                    bindings: Default::default(),
                });
                keymaps.len() - 1
            }
        };
        keymaps[index].bindings.insert(shortcut.to_string(), action.to_string());
        Ok(())
    }

    pub fn remove_shortcut_for_os(
        &mut self,
        os: &str,
        action: &str,
        shortcut: &str,
    ) -> Result<(), ConfigError> {
        let key = format!("keymaps.{action}");
        if !SHORTCUT_ACTIONS.contains(&action) {
            return Err(ConfigError::invalid_at(key, "unknown shortcut action"));
        }
        let mut removed = false;
        for group in self.settings.keymaps.iter_mut().filter(|g| matches_current_os(&g.context, os)) {
            let before = group.bindings.len();
            group.bindings.retain(|bound, bound_action| {
                !(bound.eq_ignore_ascii_case(shortcut) && bound_action.as_str() == action)
            });
            removed |= group.bindings.len() != before;
        }
        if !removed {
            return Err(ConfigError::invalid_at(key, format!("{shortcut} is not bound to {action}")));
        }
        Ok(())
    }

    pub fn update_surface_model(&mut self, surface: SurfaceKind, model_id: Option<String>) {
        match surface {
            SurfaceKind::Chat => self.settings.surfaces.chat.generation.model_id = model_id,
            SurfaceKind::QuickActions => {
                self.settings.surfaces.quick_actions.generation.model_id = model_id
            }
            SurfaceKind::TitleGeneration => {
                self.settings.surfaces.title_generation.generation.model_id = model_id
            }
            SurfaceKind::SpeechToText => self.settings.surfaces.speech_to_text.model_id = model_id,
            SurfaceKind::SettingsAgent => {
                self.settings.surfaces.settings_agent.generation.model_id = model_id
            }
        }
    }

    pub fn update_surface_parameter(
        &mut self,
        surface: SurfaceKind,
        key: &str,
        value: serde_json::Value,
    ) {
        let params = match surface {
            SurfaceKind::Chat => &mut self.settings.surfaces.chat.generation.parameters,
            SurfaceKind::QuickActions => {
                &mut self.settings.surfaces.quick_actions.generation.parameters
            }
            SurfaceKind::SettingsAgent => {
                &mut self.settings.surfaces.settings_agent.generation.parameters
            }
            SurfaceKind::TitleGeneration => {
                log::warn!("update_surface_parameter: title_generation has no parameters field");
                return;
            }
            SurfaceKind::SpeechToText => {
                log::warn!("update_surface_parameter: speech_to_text has no parameters field");
                return;
            }
        };

        let is_null = value.is_null();
        match key {
            "temperature" => params.temperature = if is_null { None } else { value.as_f64() },
            "max_tokens" => {
                params.max_tokens = if is_null {
                    None
                } else {
                    value.as_u64().map(|v| v as u32)
                }
            }
            "top_p" => params.top_p = if is_null { None } else { value.as_f64() },
            "frequency_penalty" => {
                params.frequency_penalty = if is_null { None } else { value.as_f64() }
            }
            "presence_penalty" => {
                params.presence_penalty = if is_null { None } else { value.as_f64() }
            }
            "reasoning_effort" => {
                params.reasoning_effort = if is_null {
                    None
                } else {
                    value.as_str().map(String::from)
                };
            }
            _ => {
                if is_null {
                    params.extra.remove(key);
                } else {
                    params.extra.insert(key.to_string(), value);
                }
            }
        }
    }

    pub fn update_surface_enabled_tools(&mut self, surface: SurfaceKind, tools: Vec<String>) {
        match surface {
            SurfaceKind::Chat => self.settings.surfaces.chat.generation.enabled_tools = tools,
            SurfaceKind::SettingsAgent => {
                self.settings.surfaces.settings_agent.generation.enabled_tools = tools
            }
            SurfaceKind::QuickActions | SurfaceKind::TitleGeneration | SurfaceKind::SpeechToText => {
                log::warn!("update_surface_enabled_tools: {surface:?} has no tools");
            }
        }
    }

    pub fn set_surface_tool(&mut self, surface: SurfaceKind, tool: &str, enabled: bool) {
        let tools = match surface {
            SurfaceKind::Chat => &mut self.settings.surfaces.chat.generation.enabled_tools,
            SurfaceKind::SettingsAgent => {
                &mut self.settings.surfaces.settings_agent.generation.enabled_tools
            }
            SurfaceKind::QuickActions | SurfaceKind::TitleGeneration | SurfaceKind::SpeechToText => {
                log::warn!("set_surface_tool: {surface:?} has no tools");
                return;
            }
        };
        if enabled {
            if !tools.iter().any(|name| name == tool) {
                tools.push(tool.to_string());
            }
        } else {
            tools.retain(|name| name != tool);
        }
    }

    pub fn update_speech_to_text(&mut self, config: SpeechToTextConfig) {
        self.settings.surfaces.speech_to_text = config;
    }

    pub fn add_model(&mut self, config: ModelConfig) {
        self.settings.models.push(config);
    }

    pub fn update_model(&mut self, model_id: &str, config: ModelConfig) {
        if let Some(existing) = self.settings.models.iter_mut().find(|m| m.id == model_id) {
            *existing = config;
        } else {
            self.settings.models.push(config);
        }
    }

    pub fn delete_model(&mut self, model_id: &str) {
        self.settings.models.retain(|m| m.id != model_id);
        let s = &mut self.settings.surfaces;
        if s.chat.generation.model_id.as_deref() == Some(model_id) {
            s.chat.generation.model_id = None;
        }
        if s.quick_actions.generation.model_id.as_deref() == Some(model_id) {
            s.quick_actions.generation.model_id = None;
        }
        if s.title_generation.generation.model_id.as_deref() == Some(model_id) {
            s.title_generation.generation.model_id = None;
        }
        if s.speech_to_text.model_id.as_deref() == Some(model_id) {
            s.speech_to_text.model_id = None;
        }
        if s.settings_agent.generation.model_id.as_deref() == Some(model_id) {
            s.settings_agent.generation.model_id = None;
        }
    }

    pub fn update_notifications(&mut self, config: NotificationSettings) {
        self.settings.notifications = config;
    }

    pub fn update_history_retention(&mut self, days: u32) {
        self.settings.history_retention_days = days.min(3650);
    }

    pub fn resolve_stt_model(&self) -> Option<&ModelConfig> {
        let id = self.settings.surfaces.speech_to_text.model_id.as_deref()?;
        self.settings
            .models
            .iter()
            .find(|m| m.id == id && m.is_stt())
    }

    pub fn update_skills_order(&mut self, order: Vec<String>) {
        self.settings.skills_order = order;
    }

    pub fn prompt_path(&self, kind: PromptKind) -> Option<&str> {
        match kind {
            PromptKind::System => Some(&self.settings.prompt_base.system),
            PromptKind::AboutYou => Some(&self.settings.prompt_base.about_you),
            PromptKind::Environment => Some(&self.settings.prompt_base.environment),
            PromptKind::InputFormat => Some(&self.settings.prompt_base.input_format),
            PromptKind::TitleGeneration => Some(&self.settings.surfaces.title_generation.prompt),
            PromptKind::SpeechToText => self.settings.surfaces.speech_to_text.prompt.as_deref(),
        }
    }

    pub fn read_prompt(&self, kind: PromptKind) -> String {
        let Some(path) = self.prompt_path(kind) else {
            return String::new();
        };
        PromptStore::new(&self.config_dir)
            .read(path)
            .unwrap_or_default()
    }

    pub fn read_prompt_optional(&self, kind: PromptKind) -> Option<String> {
        let path = self.prompt_path(kind)?;
        let content = PromptStore::new(&self.config_dir).read(path).ok()?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub fn write_prompt(&self, kind: PromptKind, content: &str) -> Result<(), ConfigError> {
        let Some(path) = self.prompt_path(kind) else {
            return Err(ConfigError::invalid(format!(
                "no path configured for prompt kind {:?}",
                kind
            )));
        };
        let path = path.to_string();
        PromptStore::new(&self.config_dir).write(&path, content)
    }

    pub fn input_format_guide(&self) -> String {
        self.read_prompt(PromptKind::InputFormat)
    }

    pub fn about_you(&self) -> String {
        self.read_prompt(PromptKind::AboutYou)
    }

    pub fn preferred_name(&self) -> &str {
        &self.settings.preferred_name
    }

    pub fn environment_section_template(&self) -> String {
        self.read_prompt(PromptKind::Environment)
    }

    pub fn system_prompt(&self) -> String {
        self.read_prompt(PromptKind::System)
    }

    pub fn title_generation_prompt(&self) -> String {
        self.read_prompt(PromptKind::TitleGeneration)
    }

    pub fn stt_prompt(&self) -> Option<String> {
        self.read_prompt_optional(PromptKind::SpeechToText)
    }

    pub fn stt_keyterms(&self) -> Vec<String> {
        let Some(raw) = self.settings.surfaces.speech_to_text.keyterms_file.as_deref() else {
            return Vec::new();
        };
        let path = match resolve_config_relative(raw, &self.config_dir) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("invalid speech_to_text.keyterms_file '{raw}': {e}");
                return Vec::new();
            }
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        parse_keyterms(&content)
    }

    pub fn stt_keyterms_path_or_default(&self) -> &str {
        self.settings
            .surfaces
            .speech_to_text
            .keyterms_file
            .as_deref()
            .unwrap_or(DEFAULT_STT_KEYTERMS_PATH)
    }

    pub fn read_stt_keyterms_file(&self) -> Result<KeytermsDoc, ConfigError> {
        let path = self.stt_keyterms_path_or_default().to_string();
        let resolved = resolve_config_relative(&path, &self.config_dir)?;
        let content = std::fs::read_to_string(&resolved).unwrap_or_default();
        let exists = resolved.exists();
        let term_count = parse_keyterms(&content).len();
        Ok(KeytermsDoc {
            path,
            content,
            exists,
            term_count,
        })
    }

    pub(crate) fn adopt_keyterms_path(&mut self) {
        if self.settings.surfaces.speech_to_text.keyterms_file.is_none() {
            self.settings.surfaces.speech_to_text.keyterms_file =
                Some(DEFAULT_STT_KEYTERMS_PATH.to_string());
        }
    }

    pub fn write_stt_keyterms_file(
        &mut self,
        content: &str,
    ) -> Result<KeytermsDoc, ConfigError> {
        let path = self.stt_keyterms_path_or_default().to_string();
        let resolved = resolve_config_relative(&path, &self.config_dir)?;
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(&resolved, content.as_bytes())?;
        self.adopt_keyterms_path();
        let term_count = parse_keyterms(content).len();
        Ok(KeytermsDoc {
            path,
            content: content.to_string(),
            exists: true,
            term_count,
        })
    }
}

fn build_settings(config_dir: &Path, content: &str) -> Result<Settings, ConfigError> {
    let outcome = parse_and_migrate(content)?;
    let mut settings = outcome.settings;

    apply_inline_prompts(config_dir, &outcome.inline_prompts)?;
    apply_legacy_flat_md_renames(config_dir, &outcome.legacy_flat_md_renames);
    rewrite_legacy_default_paths(config_dir, &mut settings);
    ensure_prompt_defaults(config_dir, &settings);
    replace_legacy_system_default(config_dir, &settings.prompt_base.system);

    apply_in_memory_steps(&mut settings, outcome.seed_env_vars);

    validate(&settings)?;
    Ok(settings)
}

pub fn check_settings_document(content: &str) -> Result<(), ConfigError> {
    let outcome = parse_and_migrate(content)?;
    let mut settings = outcome.settings;
    apply_in_memory_steps(&mut settings, outcome.seed_env_vars);
    validate(&settings)
}

fn apply_in_memory_steps(settings: &mut Settings, seed: bool) {
    migrate_model_params(settings);
    sanitize_capabilities(settings);
    migrate_explicit_capabilities(settings);
    ensure_surface_defaults(settings);

    if seed {
        seed_env_vars(settings);
    }
}

fn seed_env_vars(settings: &mut Settings) {
    let referenced = crate::services::missing_env::collect_missing(settings, |_| false);
    let (valid, invalid): (Vec<_>, Vec<_>) = referenced
        .into_iter()
        .partition(|reference| secrets::validate_name(&reference.name));
    for reference in &invalid {
        log::warn!(
            "skipped seeding env_vars entry: '{}' is not a valid variable name",
            reference.name
        );
    }
    settings.env_vars = valid
        .into_iter()
        .map(|reference| EnvVar {
            secret: reference
                .used_in
                .iter()
                .any(|usage| usage.key.ends_with(".api_key") || usage.key.contains(".env.")),
            name: reference.name,
            description: String::new(),
            renamed_from: Vec::new(),
        })
        .collect();
    log::info!(
        "seeded env_vars from existing references: {:?}",
        settings.env_vars.iter().map(|var| var.name.as_str()).collect::<Vec<_>>()
    );
}

const DEFAULT_STT_KEYTERMS_PATH: &str = "data/keyterms.txt";

#[derive(Debug, Clone, serde::Serialize)]
pub struct KeytermsDoc {
    pub path: String,
    pub content: String,
    pub exists: bool,
    pub term_count: usize,
}

fn parse_keyterms(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn apply_inline_prompts(
    config_dir: &Path,
    inline_prompts: &[InlinePromptWrite],
) -> Result<(), ConfigError> {
    if inline_prompts.is_empty() {
        return Ok(());
    }
    let store = PromptStore::new(config_dir);
    for entry in inline_prompts {
        let target = match store.resolve(&entry.path) {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "skipping inline prompt write to '{}': {e}",
                    entry.path
                );
                continue;
            }
        };
        if target.exists() {
            log::info!(
                "preserving existing prompt file at '{}', migrated inline content discarded",
                entry.path
            );
            continue;
        }
        store.write(&entry.path, &entry.content)?;
        log::info!("migrated inline prompt to '{}'", entry.path);
    }
    Ok(())
}

fn rewrite_legacy_default_paths(config_dir: &Path, settings: &mut Settings) {
    let rewrites: [(&mut String, &[&str], &str); 3] = [
        (
            &mut settings.prompt_base.about_you,
            &["about_me.md", "prompts/base/about_me.md"],
            "prompts/base/about_you.md",
        ),
        (
            &mut settings.prompt_base.environment,
            &["environment_section.md"],
            "prompts/base/environment.md",
        ),
        (
            &mut settings.prompt_base.input_format,
            &["input_format_guide.md"],
            "prompts/base/input_format.md",
        ),
    ];
    for (field, legacy_paths, new_path) in rewrites {
        if legacy_paths.iter().any(|p| field == *p) && config_dir.join(new_path).exists() {
            log::info!(
                "rewriting legacy default prompt path '{}' -> '{}'",
                field,
                new_path
            );
            *field = new_path.to_string();
        }
    }
}

fn apply_legacy_flat_md_renames(config_dir: &Path, renames: &[LegacyFlatMdRename]) {
    for rename in renames {
        let old = config_dir.join(rename.old_relative);
        let new = config_dir.join(rename.new_relative);
        if !old.exists() || new.exists() {
            continue;
        }
        if let Some(parent) = new.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!(
                    "failed to create parent dir for legacy rename '{}': {e}",
                    rename.new_relative
                );
                continue;
            }
        }
        match std::fs::rename(&old, &new) {
            Ok(_) => log::info!(
                "migrated legacy prompt file '{}' -> '{}'",
                rename.old_relative,
                rename.new_relative
            ),
            Err(e) => log::warn!(
                "failed to rename legacy prompt file '{}' -> '{}': {e}",
                rename.old_relative,
                rename.new_relative
            ),
        }
    }
}

fn ensure_prompt_defaults(config_dir: &Path, settings: &Settings) {
    let store = PromptStore::new(config_dir);
    let entries: [(PromptKind, Option<&str>); 6] = [
        (PromptKind::System, Some(settings.prompt_base.system.as_str())),
        (PromptKind::AboutYou, Some(settings.prompt_base.about_you.as_str())),
        (
            PromptKind::Environment,
            Some(settings.prompt_base.environment.as_str()),
        ),
        (
            PromptKind::InputFormat,
            Some(settings.prompt_base.input_format.as_str()),
        ),
        (
            PromptKind::TitleGeneration,
            Some(settings.surfaces.title_generation.prompt.as_str()),
        ),
        (
            PromptKind::SpeechToText,
            settings.surfaces.speech_to_text.prompt.as_deref(),
        ),
    ];
    for (kind, raw_path) in entries {
        let path = raw_path.unwrap_or_else(|| kind.default_path());
        if let Err(e) = store.ensure_default(kind, path) {
            log::warn!(
                "failed to ensure default prompt for {:?} at '{}': {e}",
                kind,
                path
            );
        }
    }
}
