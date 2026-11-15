use std::sync::atomic::{AtomicBool, Ordering};

use crate::models::settings::{ModelConfig, Settings};
use crate::services::config::SurfaceKind;
use crate::services::env_resolve::env_ref_names;

pub struct SetupSession {
    active: AtomicBool,
}

impl SetupSession {
    pub fn new(active: bool) -> Self {
        Self { active: AtomicBool::new(active) }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn finish(&self) {
        self.active.store(false, Ordering::Relaxed);
    }
}

pub fn key_usable(model: &ModelConfig, env_is_set: impl Fn(&str) -> bool) -> bool {
    let key = model.api_key.as_deref().unwrap_or("");
    !key.is_empty() && env_ref_names(key).iter().all(|name| env_is_set(name))
}

fn model_by_id<'a>(settings: &'a Settings, id: &Option<String>) -> Option<&'a ModelConfig> {
    let id = id.as_ref()?;
    settings.models.iter().find(|m| &m.id == id)
}

pub fn setup_needed(settings: &Settings, env_is_set: impl Fn(&str) -> bool) -> bool {
    let surfaces = &settings.surfaces;
    let effective = surfaces
        .settings_agent
        .generation
        .model_id
        .clone()
        .or_else(|| surfaces.chat.generation.model_id.clone());
    !model_by_id(settings, &effective)
        .is_some_and(|model| model.is_text() && key_usable(model, &env_is_set))
}

fn needs_model(settings: &Settings, id: &Option<String>, env_is_set: &impl Fn(&str) -> bool) -> bool {
    !model_by_id(settings, id).is_some_and(|model| key_usable(model, env_is_set))
}

pub fn assign_setup_model(
    settings: &mut Settings,
    model_id: &str,
    env_is_set: impl Fn(&str) -> bool,
) -> Option<SurfaceKind> {
    let model = settings.models.iter().find(|m| m.id == model_id)?;
    let surfaces = &settings.surfaces;
    if model.is_text() && needs_model(settings, &surfaces.chat.generation.model_id, &env_is_set) {
        settings.surfaces.chat.generation.model_id = Some(model_id.to_string());
        return Some(SurfaceKind::Chat);
    }
    if model.is_stt() && needs_model(settings, &surfaces.speech_to_text.model_id, &env_is_set) {
        settings.surfaces.speech_to_text.model_id = Some(model_id.to_string());
        return Some(SurfaceKind::SpeechToText);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::ModelType;

    fn model(id: &str, model_type: ModelType, api_key: &str) -> ModelConfig {
        ModelConfig {
            id: id.to_string(),
            model: "m".to_string(),
            display_name: id.to_string(),
            model_type,
            provider: None,
            group: None,
            api_key: Some(api_key.to_string()),
            base_url: None,
            parameters: None,
            context_window_size: None,
            api_mode: None,
            capabilities: None,
            store: true,
        }
    }

    fn settings_with(models: Vec<ModelConfig>) -> Settings {
        Settings { models, ..Settings::default() }
    }

    fn only_set(name: &'static str) -> impl Fn(&str) -> bool {
        move |asked| asked == name
    }

    #[test]
    fn setup_needed_without_models() {
        assert!(setup_needed(&Settings::default(), only_set("KEY")));
    }

    #[test]
    fn setup_needed_when_the_agent_model_does_not_exist() {
        let mut settings = settings_with(vec![model("a", ModelType::Text, "${KEY}")]);
        settings.surfaces.settings_agent.generation.model_id = Some("missing".into());
        settings.surfaces.chat.generation.model_id = Some("a".into());
        assert!(setup_needed(&settings, only_set("KEY")));
    }

    #[test]
    fn setup_needed_when_the_effective_model_is_stt() {
        let mut settings = settings_with(vec![model("s", ModelType::Stt, "${KEY}")]);
        settings.surfaces.chat.generation.model_id = Some("s".into());
        assert!(setup_needed(&settings, only_set("KEY")));
    }

    #[test]
    fn setup_needed_when_the_key_variable_is_unset() {
        let mut settings = settings_with(vec![model("a", ModelType::Text, "${KEY}")]);
        settings.surfaces.chat.generation.model_id = Some("a".into());
        assert!(setup_needed(&settings, |_| false));
    }

    #[test]
    fn setup_not_needed_with_a_usable_text_model() {
        let mut settings = settings_with(vec![model("a", ModelType::Text, "${KEY}")]);
        settings.surfaces.chat.generation.model_id = Some("a".into());
        assert!(!setup_needed(&settings, only_set("KEY")));
    }

    fn assigned_chat(chat: Option<&str>, models: Vec<ModelConfig>, env: &'static str) -> (Option<SurfaceKind>, Settings) {
        let mut settings = settings_with(models);
        settings.surfaces.chat.generation.model_id = chat.map(str::to_string);
        let result = assign_setup_model(&mut settings, "new", only_set(env));
        (result, settings)
    }

    #[test]
    fn text_model_is_assigned_when_chat_is_unset() {
        let (result, settings) = assigned_chat(None, vec![model("new", ModelType::Text, "${KEY}")], "KEY");
        assert_eq!(result, Some(SurfaceKind::Chat));
        assert_eq!(settings.surfaces.chat.generation.model_id.as_deref(), Some("new"));
    }

    #[test]
    fn text_model_is_assigned_when_the_chat_key_is_unset() {
        let models = vec![model("old", ModelType::Text, "${OLD}"), model("new", ModelType::Text, "${KEY}")];
        let (result, settings) = assigned_chat(Some("old"), models, "KEY");
        assert_eq!(result, Some(SurfaceKind::Chat));
        assert_eq!(settings.surfaces.chat.generation.model_id.as_deref(), Some("new"));
    }

    #[test]
    fn text_model_leaves_a_usable_chat_model() {
        let models = vec![model("old", ModelType::Text, "${KEY}"), model("new", ModelType::Text, "${KEY}")];
        let (result, settings) = assigned_chat(Some("old"), models, "KEY");
        assert_eq!(result, None);
        assert_eq!(settings.surfaces.chat.generation.model_id.as_deref(), Some("old"));
    }

    fn assigned_speech(speech: Option<&str>, models: Vec<ModelConfig>, env: &'static str) -> (Option<SurfaceKind>, Settings) {
        let mut settings = settings_with(models);
        settings.surfaces.speech_to_text.model_id = speech.map(str::to_string);
        let result = assign_setup_model(&mut settings, "new", only_set(env));
        (result, settings)
    }

    #[test]
    fn stt_model_is_assigned_when_speech_is_unset() {
        let (result, settings) = assigned_speech(None, vec![model("new", ModelType::Stt, "${KEY}")], "KEY");
        assert_eq!(result, Some(SurfaceKind::SpeechToText));
        assert_eq!(settings.surfaces.speech_to_text.model_id.as_deref(), Some("new"));
    }

    #[test]
    fn stt_model_is_assigned_when_the_speech_key_is_unset() {
        let models = vec![model("old", ModelType::Stt, "${OLD}"), model("new", ModelType::Stt, "${KEY}")];
        let (result, settings) = assigned_speech(Some("old"), models, "KEY");
        assert_eq!(result, Some(SurfaceKind::SpeechToText));
        assert_eq!(settings.surfaces.speech_to_text.model_id.as_deref(), Some("new"));
    }

    #[test]
    fn stt_model_leaves_a_usable_speech_model() {
        let models = vec![model("old", ModelType::Stt, "${KEY}"), model("new", ModelType::Stt, "${KEY}")];
        let (result, settings) = assigned_speech(Some("old"), models, "KEY");
        assert_eq!(result, None);
        assert_eq!(settings.surfaces.speech_to_text.model_id.as_deref(), Some("old"));
    }
}
