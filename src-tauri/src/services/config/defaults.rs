use crate::models::settings::Settings;
use crate::services::secrets;

use super::ConfigError;

pub(super) fn ensure_surface_defaults(settings: &mut Settings) {
    let fallback = settings
        .models
        .iter()
        .find(|m| m.is_text())
        .map(|m| m.id.clone());

    let chat_model = settings.surfaces.chat.generation.model_id.clone();
    let chat_model = chat_model.filter(|id| settings.models.iter().any(|m| &m.id == id));
    if chat_model.is_none() {
        settings.surfaces.chat.generation.model_id = fallback.clone();
    } else {
        settings.surfaces.chat.generation.model_id = chat_model;
    }
}

pub(super) fn ensure_generation(
    surface: &mut serde_json::Map<String, serde_json::Value>,
) -> &mut serde_json::Map<String, serde_json::Value> {
    let entry = surface
        .entry("generation".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    entry.as_object_mut().expect("generation must be object")
}

pub(super) fn ensure_parameters(
    gen: &mut serde_json::Map<String, serde_json::Value>,
) -> &mut serde_json::Map<String, serde_json::Value> {
    let entry = gen
        .entry("parameters".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    entry.as_object_mut().expect("parameters must be object")
}

pub(crate) fn validate(settings: &Settings) -> Result<(), ConfigError> {
    let mut seen_ids = std::collections::HashSet::new();
    for model in &settings.models {
        if model.id.is_empty() {
            return Err(ConfigError::invalid("Each model must have an 'id' field"));
        }

        if !seen_ids.insert(&model.id) {
            return Err(ConfigError::invalid(format!(
                "Duplicate model ID: '{}'",
                model.id
            )));
        }

        let display = &model.display_name;
        let key = |field: &str| format!("models[{}].{field}", model.id);

        if model.model.is_empty() {
            return Err(ConfigError::invalid_at(
                key("model"),
                format!("Model '{display}' field 'model' cannot be empty"),
            ));
        }

        if model.display_name.is_empty() {
            return Err(ConfigError::invalid_at(
                key("display_name"),
                format!("Model '{}' missing required field: display_name", model.id),
            ));
        }

        if model.api_key.as_deref().unwrap_or("").is_empty() {
            return Err(ConfigError::invalid_at(
                key("api_key"),
                format!(
                    "Model '{display}' requires 'api_key' (use \"${{ENV_VAR}}\" for env-based keys)"
                ),
            ));
        }

        if let Some(ref url) = model.base_url {
            if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(ConfigError::invalid_at(
                    key("base_url"),
                    format!("Model '{display}' base_url must start with http:// or https://"),
                ));
            }
        }
    }

    if settings.autosave_debounce_ms > 10000 {
        return Err(ConfigError::invalid_at(
            "autosave_debounce_ms",
            "autosave_debounce_ms must be between 0 and 10000 milliseconds",
        ));
    }

    if settings.history_retention_days > 3650 {
        return Err(ConfigError::invalid_at(
            "history_retention_days",
            "history_retention_days must be between 0 and 3650 days (0 = keep forever)",
        ));
    }

    if let Some(ref stt_id) = settings.surfaces.speech_to_text.model_id {
        let key = "surfaces.speech_to_text.model_id";
        match settings.models.iter().find(|m| &m.id == stt_id) {
            None => {
                return Err(ConfigError::invalid_at(
                    key,
                    format!("speech_to_text.model_id references unknown model id: '{stt_id}'"),
                ));
            }
            Some(m) if !m.is_stt() => {
                return Err(ConfigError::invalid_at(
                    key,
                    format!("speech_to_text.model_id '{stt_id}' must have type='stt'"),
                ));
            }
            _ => {}
        }
    }

    validate_webview_providers(settings)?;
    validate_mcp_servers(settings)?;
    validate_env_vars(settings)?;

    Ok(())
}

fn validate_mcp_servers(settings: &Settings) -> Result<(), ConfigError> {
    for (name, server) in &settings.mcp_servers {
        let valid_name = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid_name {
            return Err(ConfigError::invalid_at(
                format!("mcp_servers.{name}"),
                format!("MCP server name '{name}' must match [a-zA-Z0-9_-]+"),
            ));
        }
        if server.command.trim().is_empty() {
            return Err(ConfigError::invalid_at(
                format!("mcp_servers.{name}.command"),
                format!("MCP server '{name}' needs a command"),
            ));
        }
    }
    Ok(())
}

fn validate_env_vars(settings: &Settings) -> Result<(), ConfigError> {
    let mut seen = std::collections::HashSet::new();
    for var in &settings.env_vars {
        let key = format!("env_vars[{}].name", var.name);
        if !secrets::validate_name(&var.name) {
            return Err(ConfigError::invalid_at(
                key,
                format!(
                    "Environment variable name '{}' must start with a letter or '_' and contain only letters, digits and '_'",
                    var.name
                ),
            ));
        }
        if !seen.insert(var.name.as_str()) {
            return Err(ConfigError::invalid_at(
                key,
                format!("Duplicate environment variable name: '{}'", var.name),
            ));
        }
    }
    Ok(())
}

fn validate_webview_providers(settings: &Settings) -> Result<(), ConfigError> {
    let mut seen = std::collections::HashSet::new();
    for provider in &settings.webview_providers {
        if provider.id.is_empty() {
            return Err(ConfigError::invalid("webview_providers entry missing 'id'"));
        }
        let key = |field: &str| format!("webview_providers[{}].{field}", provider.id);
        if !provider
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ConfigError::invalid(format!(
                "webview_providers id '{}' must match [a-zA-Z0-9_-]+",
                provider.id
            )));
        }
        if !seen.insert(provider.id.clone()) {
            return Err(ConfigError::invalid(format!(
                "Duplicate webview_providers id: '{}'",
                provider.id
            )));
        }
        if provider.name.is_empty() {
            return Err(ConfigError::invalid_at(
                key("name"),
                format!("webview_providers '{}' has empty 'name'", provider.id),
            ));
        }
        match tauri::Url::parse(&provider.url) {
            Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => {}
            Ok(_) => {
                return Err(ConfigError::invalid_at(
                    key("url"),
                    format!(
                        "webview_providers '{}' url must use http or https scheme",
                        provider.id
                    ),
                ));
            }
            Err(e) => {
                return Err(ConfigError::invalid_at(
                    key("url"),
                    format!("webview_providers '{}' has invalid url: {e}", provider.id),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::{
        EnvVar, ModelConfig, ModelType, Provider, Settings, WebviewProvider,
    };

    fn text_model(id: &str) -> ModelConfig {
        ModelConfig {
            id: id.into(),
            model: "gpt-4".into(),
            display_name: "T".into(),
            model_type: ModelType::Text,
            provider: Some(Provider::default()),
            group: None,
            api_key: Some("${KEY}".into()),
            base_url: None,
            parameters: None,
            context_window_size: None,
            api_mode: None,
            capabilities: None,
            store: true,
        }
    }

    #[test]
    fn ensure_surface_defaults_leaves_quick_actions_and_title_generation_empty() {
        let mut settings = Settings {
            models: vec![text_model("m1")],
            ..Default::default()
        };
        ensure_surface_defaults(&mut settings);
        assert_eq!(
            settings.surfaces.chat.generation.model_id.as_deref(),
            Some("m1")
        );
        assert_eq!(settings.surfaces.quick_actions.generation.model_id, None);
        assert_eq!(settings.surfaces.title_generation.generation.model_id, None);
    }

    #[test]
    fn ensure_surface_defaults_keeps_explicit_quick_actions_and_title_generation() {
        let mut settings = Settings {
            models: vec![text_model("m1"), text_model("m2"), text_model("m3")],
            ..Default::default()
        };
        settings.surfaces.chat.generation.model_id = Some("m1".into());
        settings.surfaces.quick_actions.generation.model_id = Some("m2".into());
        settings.surfaces.title_generation.generation.model_id = Some("m3".into());
        ensure_surface_defaults(&mut settings);
        assert_eq!(
            settings.surfaces.quick_actions.generation.model_id.as_deref(),
            Some("m2")
        );
        assert_eq!(
            settings.surfaces.title_generation.generation.model_id.as_deref(),
            Some("m3")
        );
    }

    #[test]
    fn ensure_surface_defaults_drops_unknown_chat_model() {
        let mut settings = Settings {
            models: vec![text_model("real")],
            ..Default::default()
        };
        settings.surfaces.chat.generation.model_id = Some("ghost".into());
        ensure_surface_defaults(&mut settings);
        assert_eq!(
            settings.surfaces.chat.generation.model_id.as_deref(),
            Some("real")
        );
    }

    #[test]
    fn ensure_generation_creates_object_when_missing() {
        let mut surface = serde_json::Map::new();
        let gen = ensure_generation(&mut surface);
        assert!(gen.is_empty());
        assert!(surface.get("generation").unwrap().is_object());
    }

    #[test]
    fn ensure_parameters_creates_object_when_missing() {
        let mut gen = serde_json::Map::new();
        let params = ensure_parameters(&mut gen);
        assert!(params.is_empty());
        assert!(gen.get("parameters").unwrap().is_object());
    }

    #[test]
    fn validate_no_models_passes() {
        let mut settings = Settings::default();
        settings.models.clear();
        assert!(validate(&settings).is_ok());
    }

    #[test]
    fn validate_duplicate_id_fails() {
        let m = text_model("dup");
        let settings = Settings {
            models: vec![m.clone(), m],
            ..Default::default()
        };
        let err = validate(&settings).unwrap_err().to_string();
        assert!(err.contains("Duplicate model ID"), "{err}");
    }

    #[test]
    fn validate_webview_providers_invalid_scheme_fails() {
        let settings = Settings {
            models: vec![text_model("1")],
            webview_providers: vec![WebviewProvider {
                id: "ftp".into(),
                name: "Ftp".into(),
                url: "ftp://x.example".into(),
            }],
            ..Default::default()
        };
        let err = validate(&settings).unwrap_err().to_string();
        assert!(err.contains("http or https"), "{err}");
    }

    #[test]
    fn validate_webview_providers_duplicate_id_fails() {
        let settings = Settings {
            models: vec![text_model("1")],
            webview_providers: vec![
                WebviewProvider {
                    id: "claude".into(),
                    name: "Claude".into(),
                    url: "https://claude.ai/".into(),
                },
                WebviewProvider {
                    id: "claude".into(),
                    name: "Claude2".into(),
                    url: "https://claude.ai/".into(),
                },
            ],
            ..Default::default()
        };
        let err = validate(&settings).unwrap_err().to_string();
        assert!(err.contains("Duplicate webview_providers"), "{err}");
    }

    fn env_var(name: &str) -> EnvVar {
        EnvVar {
            name: name.into(),
            description: String::new(),
            secret: false,
            renamed_from: Vec::new(),
        }
    }

    fn settings_with_env_vars(names: &[&str]) -> Settings {
        Settings {
            models: vec![text_model("1")],
            env_vars: names.iter().map(|name| env_var(name)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn validate_env_vars_duplicate_name_fails() {
        let err = validate(&settings_with_env_vars(&["OPENAI_API_KEY", "OPENAI_API_KEY"]))
            .unwrap_err();
        assert!(err.to_string().contains("OPENAI_API_KEY"), "{err}");
        assert!(err.to_string().contains("Duplicate"), "{err}");
    }

    #[test]
    fn validate_env_vars_invalid_name_fails() {
        for name in ["1BAD", "has-dash"] {
            let err = validate(&settings_with_env_vars(&[name])).unwrap_err();
            assert!(err.to_string().contains(name), "{err}");
        }
    }

    #[test]
    fn validate_env_vars_valid_list_passes() {
        validate(&settings_with_env_vars(&["OPENAI_API_KEY", "_PRIVATE", "path2"])).unwrap();
    }
}
