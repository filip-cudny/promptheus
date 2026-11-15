use super::*;
use crate::models::settings::{KeymapGroup, ModelConfig, Settings, ModelType, Provider, Skin};
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

fn test_model(id: &str, api_key: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        model: "test".to_string(),
        display_name: "Test".to_string(),
        model_type: ModelType::Text,
        provider: Some(Provider::default()),
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

fn test_stt_model(id: &str, api_key: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        model: "whisper-1".to_string(),
        display_name: "STT".to_string(),
        model_type: ModelType::Stt,
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

fn setup_test_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let default_json = include_str!("../../../resources/default_settings.json");
    fs::write(dir.path().join("settings.json"), default_json).unwrap();
    dir
}

#[test]
fn test_load_default_settings() {
    let dir = setup_test_dir();
    let service = ConfigService::load(dir.path()).expect("should load default settings");
    assert!(!service.settings().models.is_empty());
    assert!(service.settings().models.iter().any(|m| m.is_text()));
    assert!(service.settings().models.iter().any(|m| m.is_stt()));
    assert!(service.settings().surfaces.chat.generation.model_id.is_some());
    assert!(service.settings().surfaces.speech_to_text.model_id.is_some());
}

#[test]
fn test_resolve_stt_model_returns_stt_entry() {
    let dir = setup_test_dir();
    let service = ConfigService::load(dir.path()).expect("load");
    let stt = service.resolve_stt_model().expect("stt model should resolve");
    assert!(stt.is_stt());
    assert_eq!(
        service.settings().surfaces.speech_to_text.language.as_deref(),
        Some("pl")
    );
}

#[test]
fn test_save_and_reload() {
    let dir = setup_test_dir();
    let service = ConfigService::load(dir.path()).expect("load");
    service.save().expect("save");

    let mut service2 = ConfigService::load(dir.path()).expect("reload");
    assert_eq!(service.settings().models.len(), service2.settings().models.len());
    assert_eq!(service.settings().models[0].id, service2.settings().models[0].id);

    service2.reload().expect("reload method");
    assert_eq!(service.settings().models[0].id, service2.settings().models[0].id);
}

#[test]
fn test_env_ref_preserved_on_save() {
    let dir = setup_test_dir();
    let service = ConfigService::load(dir.path()).expect("load");
    service.save().expect("save");

    let content = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&content).unwrap();
    let models = saved["models"].as_array().unwrap();
    let text_model = models.iter().find(|m| m["type"] == "text").unwrap();
    assert_eq!(text_model["api_key"].as_str().unwrap(), "${OPENAI_API_KEY}");
    let stt_model = models.iter().find(|m| m["type"] == "stt").unwrap();
    assert_eq!(stt_model["api_key"].as_str().unwrap(), "${OPENAI_API_KEY}");
}

#[test]
fn test_delete_model_clears_references() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.add_model(test_stt_model("stt-to-delete", "${KEY}"));
    service.settings.surfaces.speech_to_text.model_id = Some("stt-to-delete".to_string());

    service.delete_model("stt-to-delete");
    assert_eq!(service.settings().surfaces.speech_to_text.model_id, None);
}

#[test]
fn test_delete_model_clears_settings_agent_model() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.add_model(test_model("agent-to-delete", "${KEY}"));
    service.settings.surfaces.settings_agent.generation.model_id = Some("agent-to-delete".to_string());

    service.delete_model("agent-to-delete");
    assert_eq!(service.settings().surfaces.settings_agent.generation.model_id, None);
}

#[test]
fn test_update_surface_enabled_tools_stores_settings_agent_list() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");

    service.update_surface_enabled_tools(SurfaceKind::SettingsAgent, vec!["srv.web_search".to_string()]);

    let surfaces = &service.settings().surfaces;
    assert_eq!(surfaces.settings_agent.generation.enabled_tools, vec!["srv.web_search"]);
    assert!(surfaces.chat.generation.enabled_tools.is_empty());
}

#[test]
fn set_surface_tool_adds_and_removes_one_name_keeping_the_rest() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.update_surface_enabled_tools(SurfaceKind::Chat, vec!["a.x".to_string(), "web_search".to_string()]);
    service.update_surface_enabled_tools(SurfaceKind::SettingsAgent, vec!["c.z".to_string()]);

    service.set_surface_tool(SurfaceKind::Chat, "b.y", true);
    assert_eq!(service.settings().surfaces.chat.generation.enabled_tools, vec!["a.x", "web_search", "b.y"]);

    service.set_surface_tool(SurfaceKind::Chat, "a.x", true);
    assert_eq!(service.settings().surfaces.chat.generation.enabled_tools, vec!["a.x", "web_search", "b.y"]);

    service.set_surface_tool(SurfaceKind::Chat, "web_search", false);
    assert_eq!(service.settings().surfaces.chat.generation.enabled_tools, vec!["a.x", "b.y"]);
    assert_eq!(service.settings().surfaces.settings_agent.generation.enabled_tools, vec!["c.z"]);
}

#[test]
fn test_direct_api_key_preserved_on_save() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.add_model(test_model("direct-model", "sk-direct-secret"));
    service.save().expect("save");

    let content = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&content).unwrap();
    let models = saved["models"].as_array().unwrap();
    let direct = models.iter().find(|m| m["id"] == "direct-model").unwrap();
    assert_eq!(direct["api_key"].as_str().unwrap(), "sk-direct-secret");
}

#[test]
fn test_mutation_methods() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");

    let new_model = test_model("new-model", "${KEY}");
    let initial_count = service.settings().models.len();
    service.add_model(new_model.clone());
    assert_eq!(service.settings().models.len(), initial_count + 1);

    let mut updated = new_model.clone();
    updated.display_name = "GPT-5 Updated".to_string();
    service.update_model("new-model", updated);
    assert_eq!(
        service.settings().models.iter().find(|m| m.id == "new-model").unwrap().display_name,
        "GPT-5 Updated"
    );

    let upsert = test_model("upsert-model", "${KEY}");
    let count_before = service.settings().models.len();
    service.update_model("upsert-model", upsert);
    assert_eq!(service.settings().models.len(), count_before + 1);

    service.delete_model("upsert-model");
    assert!(service.settings().models.iter().all(|m| m.id != "upsert-model"));

    service.update_setting("theme", serde_json::json!("light")).unwrap();
    assert_eq!(service.settings().theme, "light");
}

#[test]
fn test_first_run_creates_defaults() {
    let dir = TempDir::new().unwrap();
    let service = ConfigService::load(dir.path()).expect("should create defaults");
    assert!(dir.path().join("settings.json").exists());
    assert!(dir.path().join(".env").exists());
    assert!(!service.settings().models.is_empty());
}

#[test]
fn test_first_run_env_template_created() {
    let dir = TempDir::new().unwrap();
    let _service = ConfigService::load(dir.path()).expect("should create defaults");
    let env_content = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env_content.contains("OPENAI_API_KEY"));
}

#[test]
fn test_existing_env_not_overwritten() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path()).unwrap();
    fs::write(dir.path().join(".env"), "OPENAI_API_KEY=real_key\n").unwrap();
    let _service = ConfigService::load(dir.path()).expect("should create defaults");
    let env_content = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert_eq!(env_content, "OPENAI_API_KEY=real_key\n");
}

#[test]
fn test_migrates_legacy_schema() {
    let dir = TempDir::new().unwrap();
    let legacy = r#"{
        "default_model": "model-a",
        "quick_action_default_model": "model-b",
        "speech_to_text_model": "stt-a",
        "conversation_title_model": "model-c",
        "conversation_title_prompt": "Make a title",
        "system_prompt": "Custom system",
        "about_me": "about_me.md",
        "environment_section": "environment_section.md",
        "stt_prompt": "stt_prompt.md",
        "selected_tools": ["web_search"],
        "models": [
            {
                "id": "model-a",
                "type": "text",
                "model": "gpt-4",
                "display_name": "GPT-4",
                "provider": "openai",
                "api_key": "${OPENAI_API_KEY}",
                "parameters": { "reasoning_effort": "medium" },
                "enabled_tools": ["web_search"]
            },
            {
                "id": "model-b",
                "type": "text",
                "model": "gpt-4",
                "display_name": "Quick",
                "provider": "openai",
                "api_key": "${OPENAI_API_KEY}"
            },
            {
                "id": "model-c",
                "type": "text",
                "model": "gpt-4",
                "display_name": "Title",
                "provider": "openai",
                "api_key": "${OPENAI_API_KEY}"
            },
            {
                "id": "stt-a",
                "type": "stt",
                "model": "whisper-1",
                "display_name": "STT",
                "api_key": "${OPENAI_API_KEY}",
                "language": "pl",
                "keyterms_file": "keyterms.txt",
                "no_verbatim": true
            }
        ]
    }"#;

    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load migrated");
    let s = service.settings();

    assert_eq!(s.surfaces.chat.generation.model_id.as_deref(), Some("model-a"));
    assert_eq!(s.surfaces.chat.generation.parameters.reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(s.surfaces.chat.generation.enabled_tools, vec!["web_search"]);
    assert_eq!(s.prompt_base.system, "prompts/base/system.md");
    let migrated_system = fs::read_to_string(dir.path().join("prompts/base/system.md")).unwrap();
    assert_eq!(migrated_system, "Custom system");
    assert_eq!(s.prompt_base.about_you, "about_me.md");
    assert_eq!(s.prompt_base.environment, "environment_section.md");
    assert_eq!(s.prompt_base.input_format, "prompts/base/input_format.md");
    assert_eq!(s.surfaces.quick_actions.generation.model_id.as_deref(), Some("model-b"));
    assert_eq!(s.surfaces.title_generation.generation.model_id.as_deref(), Some("model-c"));
    assert_eq!(s.surfaces.title_generation.prompt, "prompts/surfaces/title_generation.md");
    let migrated_title = fs::read_to_string(dir.path().join("prompts/surfaces/title_generation.md")).unwrap();
    assert_eq!(migrated_title, "Make a title");
    assert_eq!(s.surfaces.speech_to_text.model_id.as_deref(), Some("stt-a"));
    assert_eq!(s.surfaces.speech_to_text.language.as_deref(), Some("pl"));
    assert_eq!(s.surfaces.speech_to_text.keyterms_file.as_deref(), Some("keyterms.txt"));
    assert_eq!(s.surfaces.speech_to_text.no_verbatim, Some(true));
    assert_eq!(s.surfaces.speech_to_text.prompt.as_deref(), Some("stt_prompt.md"));

    let text_a = s.models.iter().find(|m| m.id == "model-a").unwrap();
    assert_eq!(text_a.parameters.as_ref().unwrap().reasoning_effort, None);

    let saved = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let saved_json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert!(saved_json.get("default_model").is_none());
    assert!(saved_json.get("system_prompt").is_none());
    assert!(saved_json.get("stt_prompt").is_none());
    assert!(saved_json.get("surfaces").is_some());
}

#[test]
fn test_migrate_minimal_legacy() {
    let dir = TempDir::new().unwrap();
    let legacy = r#"{
        "default_model": "m1",
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "M1", "provider": "openai", "api_key": "${OPENAI_API_KEY}" },
            { "id": "stt-only", "type": "stt", "model": "whisper-1", "display_name": "STT", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    let s = service.settings();
    assert_eq!(s.surfaces.chat.generation.model_id.as_deref(), Some("m1"));
    assert_eq!(s.surfaces.quick_actions.generation.model_id.as_deref(), Some("m1"));
}

fn two_text_models_settings(chat_model_id: Option<&str>) -> String {
    let chat_generation = match chat_model_id {
        Some(id) => format!(r#"{{ "model_id": "{id}", "parameters": {{}}, "enabled_tools": [] }}"#),
        None => r#"{ "parameters": {}, "enabled_tools": [] }"#.to_string(),
    };
    format!(
        r#"{{
        "surfaces": {{
            "chat": {{ "generation": {chat_generation} }},
            "quick_actions": {{ "generation": {{ "model_id": null, "parameters": {{}}, "enabled_tools": [] }} }}
        }},
        "models": [
            {{ "id": "m1", "type": "text", "model": "gpt-4", "display_name": "M1", "provider": "openai", "api_key": "${{OPENAI_API_KEY}}" }},
            {{ "id": "m2", "type": "text", "model": "gpt-4", "display_name": "M2", "provider": "openai", "api_key": "${{OPENAI_API_KEY}}" }}
        ]
    }}"#
    )
}

fn written_settings(dir: &TempDir) -> serde_json::Value {
    let content = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

#[test]
fn test_load_keeps_quick_actions_and_title_generation_empty() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("settings.json"), two_text_models_settings(Some("m2"))).unwrap();

    let service = ConfigService::load(dir.path()).expect("load");
    let surfaces = &service.settings().surfaces;
    assert_eq!(surfaces.chat.generation.model_id.as_deref(), Some("m2"));
    assert_eq!(surfaces.quick_actions.generation.model_id, None);
    assert_eq!(surfaces.title_generation.generation.model_id, None);

    let written = written_settings(&dir);
    assert_eq!(written["surfaces"]["chat"]["generation"]["model_id"], "m2");
    assert!(written["surfaces"]["quick_actions"]["generation"]["model_id"].is_null());
    assert!(written["surfaces"]["title_generation"]["generation"]["model_id"].is_null());
}

#[test]
fn test_load_with_unset_chat_model_picks_first_text_model_only_for_chat() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("settings.json"), two_text_models_settings(None)).unwrap();

    let service = ConfigService::load(dir.path()).expect("load");
    let surfaces = &service.settings().surfaces;
    assert_eq!(surfaces.chat.generation.model_id.as_deref(), Some("m1"));
    assert_eq!(surfaces.quick_actions.generation.model_id, None);
    assert_eq!(surfaces.title_generation.generation.model_id, None);
}

#[test]
fn test_deleted_quick_actions_and_title_generation_model_stay_empty_after_reload() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("settings.json"), two_text_models_settings(Some("m1"))).unwrap();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.settings.surfaces.quick_actions.generation.model_id = Some("m2".to_string());
    service.settings.surfaces.title_generation.generation.model_id = Some("m2".to_string());

    service.delete_model("m2");
    service.save().expect("save");

    let reloaded = ConfigService::load(dir.path()).expect("reload");
    let surfaces = &reloaded.settings().surfaces;
    assert_eq!(surfaces.chat.generation.model_id.as_deref(), Some("m1"));
    assert_eq!(surfaces.quick_actions.generation.model_id, None);
    assert_eq!(surfaces.title_generation.generation.model_id, None);
}

#[test]
fn test_migrate_chat_prompt_fields_into_prompt_base() {
    let dir = TempDir::new().unwrap();
    let legacy = r#"{
        "surfaces": {
            "chat": {
                "generation": { "model_id": "m1", "parameters": {}, "enabled_tools": [] },
                "system_prompt": "Custom system",
                "about_me": "about_me.md",
                "environment_section": "environment_section.md"
            },
            "speech_to_text": { "model_id": "stt-1" }
        },
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" },
            { "id": "stt-1", "type": "stt", "model": "whisper-1", "display_name": "STT", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    let s = service.settings();

    assert_eq!(s.prompt_base.system, "prompts/base/system.md");
    let migrated_system = fs::read_to_string(dir.path().join("prompts/base/system.md")).unwrap();
    assert_eq!(migrated_system, "Custom system");
    assert_eq!(s.prompt_base.about_you, "about_me.md");
    assert_eq!(s.prompt_base.environment, "environment_section.md");
    assert_eq!(s.surfaces.chat.generation.model_id.as_deref(), Some("m1"));

    let saved = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let saved_json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let chat = saved_json["surfaces"]["chat"].as_object().unwrap();
    assert!(!chat.contains_key("system_prompt"));
    assert!(!chat.contains_key("about_me"));
    assert!(!chat.contains_key("environment_section"));
    let prompt_base = saved_json["prompt_base"].as_object().unwrap();
    assert_eq!(prompt_base["system"], "prompts/base/system.md");
    assert!(!prompt_base.contains_key("system_prompt"));
    assert!(!prompt_base.contains_key("about_me"));
    assert!(!prompt_base.contains_key("environment_section"));
}

#[test]
fn test_rewrite_legacy_default_path_when_new_file_present() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("prompts/base")).unwrap();
    fs::write(dir.path().join("prompts/base/about_you.md"), "real content").unwrap();
    fs::write(dir.path().join("about_me.md"), "stale template").unwrap();
    let legacy = r#"{
        "prompt_base": {
            "system": "prompts/base/system.md",
            "about_me": "about_me.md",
            "environment": "prompts/base/environment.md",
            "input_format": "prompts/base/input_format.md"
        },
        "surfaces": {
            "chat": { "generation": { "model_id": "m1" } },
            "title_generation": { "prompt": "prompts/surfaces/title_generation.md" },
            "speech_to_text": { "prompt": "prompts/surfaces/speech_to_text.md" }
        },
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    assert_eq!(
        service.settings().prompt_base.about_you,
        "prompts/base/about_you.md",
        "legacy default path should be rewritten when canonical file exists"
    );
    assert_eq!(service.read_prompt(PromptKind::AboutYou), "real content");
}

#[test]
fn test_legacy_default_path_preserved_when_no_canonical_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("about_me.md"), "user content").unwrap();
    let legacy = r#"{
        "prompt_base": { "about_me": "about_me.md" },
        "surfaces": {
            "chat": { "generation": { "model_id": "m1" } },
            "speech_to_text": {}
        },
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    assert_eq!(
        service.settings().prompt_base.about_you,
        "prompts/base/about_you.md",
        "rename should move flat about_me.md to canonical path"
    );
    assert_eq!(service.read_prompt(PromptKind::AboutYou), "user content");
}

#[test]
fn test_migrates_about_me_key_to_about_you_in_prompt_base() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("prompts/base")).unwrap();
    fs::write(
        dir.path().join("prompts/base/about_me.md"),
        "user content",
    )
    .unwrap();
    let legacy = r#"{
        "prompt_base": {
            "system": "prompts/base/system.md",
            "about_me": "prompts/base/about_me.md",
            "environment": "prompts/base/environment.md",
            "input_format": "prompts/base/input_format.md"
        },
        "surfaces": {
            "chat": { "generation": { "model_id": "m1" } },
            "title_generation": { "prompt": "prompts/surfaces/title_generation.md" },
            "speech_to_text": { "prompt": "prompts/surfaces/speech_to_text.md" }
        },
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    assert_eq!(
        service.settings().prompt_base.about_you,
        "prompts/base/about_you.md"
    );

    let saved = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let saved_json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let pb = saved_json["prompt_base"].as_object().unwrap();
    assert!(!pb.contains_key("about_me"));
    assert_eq!(pb["about_you"], "prompts/base/about_you.md");
}

#[test]
fn test_renames_legacy_nested_about_me_md_file() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("prompts/base")).unwrap();
    fs::write(
        dir.path().join("prompts/base/about_me.md"),
        "user-edited content",
    )
    .unwrap();
    let legacy = r#"{
        "prompt_base": {
            "system": "prompts/base/system.md",
            "about_me": "prompts/base/about_me.md",
            "environment": "prompts/base/environment.md",
            "input_format": "prompts/base/input_format.md"
        },
        "surfaces": {
            "chat": { "generation": { "model_id": "m1" } },
            "title_generation": { "prompt": "prompts/surfaces/title_generation.md" },
            "speech_to_text": { "prompt": "prompts/surfaces/speech_to_text.md" }
        },
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");

    assert!(!dir.path().join("prompts/base/about_me.md").exists());
    assert!(dir.path().join("prompts/base/about_you.md").exists());
    assert_eq!(
        service.read_prompt(PromptKind::AboutYou),
        "user-edited content"
    );
}

#[test]
fn test_replaces_legacy_system_prompt_default() {
    let dir = setup_test_dir();
    fs::create_dir_all(dir.path().join("prompts/base")).unwrap();
    fs::write(
        dir.path().join("prompts/base/system.md"),
        "You are a helpful assistant.\n",
    )
    .unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    let content = service.read_prompt(PromptKind::System);
    assert_ne!(content, "You are a helpful assistant.\n");
    assert!(content.contains("candid"));
}

#[test]
fn test_does_not_replace_user_edited_system_prompt() {
    let dir = setup_test_dir();
    fs::create_dir_all(dir.path().join("prompts/base")).unwrap();
    fs::write(
        dir.path().join("prompts/base/system.md"),
        "You are a helpful assistant. Answer in haiku.\n",
    )
    .unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    assert_eq!(
        service.read_prompt(PromptKind::System),
        "You are a helpful assistant. Answer in haiku.\n"
    );
}

#[test]
fn test_preferred_name_defaults_to_empty_for_legacy_settings() {
    let dir = TempDir::new().unwrap();
    let legacy = r#"{
        "prompt_base": {
            "system": "prompts/base/system.md",
            "about_you": "prompts/base/about_you.md",
            "environment": "prompts/base/environment.md",
            "input_format": "prompts/base/input_format.md"
        },
        "surfaces": {
            "chat": { "generation": { "model_id": "m1" } },
            "title_generation": { "prompt": "prompts/surfaces/title_generation.md" },
            "speech_to_text": { "prompt": "prompts/surfaces/speech_to_text.md" }
        },
        "models": [
            { "id": "m1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    assert_eq!(service.preferred_name(), "");
}

#[test]
fn test_update_setting_preferred_name_trims_and_clamps() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");

    service.update_setting(
        "preferred_name",
        serde_json::Value::String("  Filip  ".to_string()),
    )
    .unwrap();
    assert_eq!(service.preferred_name(), "Filip");

    let long = "x".repeat(200);
    service.update_setting("preferred_name", serde_json::Value::String(long)).unwrap();
    assert_eq!(service.preferred_name().chars().count(), 60);
}

#[test]
fn test_migrate_stt_only_legacy() {
    let dir = TempDir::new().unwrap();
    let legacy = r#"{
        "speech_to_text_model": "stt-1",
        "models": [
            { "id": "text-1", "type": "text", "model": "gpt-4", "display_name": "T", "provider": "openai", "api_key": "${OPENAI_API_KEY}" },
            { "id": "stt-1", "type": "stt", "model": "whisper-1", "display_name": "STT", "api_key": "${OPENAI_API_KEY}", "language": "en" }
        ]
    }"#;
    fs::write(dir.path().join("settings.json"), legacy).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    let s = service.settings();
    assert_eq!(s.surfaces.speech_to_text.model_id.as_deref(), Some("stt-1"));
    assert_eq!(s.surfaces.speech_to_text.language.as_deref(), Some("en"));
}

#[test]
fn test_stt_keyterms_reads_relative_file() {
    let dir = setup_test_dir();
    fs::write(
        dir.path().join("keyterms.txt"),
        "Promptheus\n# comment\nTauri\n\n  rmcp  \n",
    )
    .unwrap();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.settings.surfaces.speech_to_text.keyterms_file =
        Some("keyterms.txt".to_string());

    let terms = service.stt_keyterms();
    assert_eq!(terms, vec!["Promptheus", "Tauri", "rmcp"]);
}

#[test]
fn test_stt_keyterms_rejects_absolute_path() {
    let dir = setup_test_dir();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("leak.txt");
    fs::write(&outside_file, "Secret\n").unwrap();

    let mut service = ConfigService::load(dir.path()).expect("load");
    service.settings.surfaces.speech_to_text.keyterms_file =
        Some(outside_file.to_string_lossy().into_owned());

    assert!(service.stt_keyterms().is_empty());
}

#[test]
fn test_stt_keyterms_rejects_parent_dir_traversal() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service.settings.surfaces.speech_to_text.keyterms_file =
        Some("../escape.txt".to_string());

    assert!(service.stt_keyterms().is_empty());
}

#[test]
fn test_stt_keyterms_resolves_config_dir_placeholder_to_relative() {
    let dir = setup_test_dir();
    fs::write(dir.path().join("keyterms.txt"), "Alpha\n").unwrap();

    let mut service = ConfigService::load(dir.path()).expect("load");
    service.settings.surfaces.speech_to_text.keyterms_file =
        Some("${CONFIG_DIR}/keyterms.txt".to_string());

    assert!(
        service.stt_keyterms().is_empty(),
        "${{CONFIG_DIR}} expands to absolute, which is rejected for config-relative fields"
    );
}

fn assert_keys_in_order(text: &str, keys: &[&str]) {
    let positions: Vec<usize> = keys
        .iter()
        .map(|k| text.find(&format!("\"{k}\"")).unwrap_or_else(|| panic!("missing {k}")))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "not sorted: {keys:?}");
}

#[test]
fn test_load_writes_sorted_maps_byte_stable() {
    let dir = setup_test_dir();
    let mut raw: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("settings.json")).unwrap())
            .unwrap();
    raw["mcp_servers"] = serde_json::json!({
        "srv_c": {"command": "c"},
        "srv_a": {
            "command": "a",
            "env": {"ENV_C": "3", "ENV_A": "1", "ENV_B": "2"},
            "tool_timeouts": {"tool_c": 3, "tool_a": 1, "tool_b": 2}
        },
        "srv_b": {"command": "b"}
    });
    raw["keymaps"] = serde_json::json!([{
        "context": "linux",
        "bindings": {"Ctrl+C": "act_c", "Ctrl+A": "act_a", "Ctrl+B": "act_b"}
    }]);
    let mut model = serde_json::to_value(test_model("sorted-model", "k")).unwrap();
    model["parameters"] = serde_json::json!({"xk_c": 3, "xk_a": 1, "xk_b": 2});
    raw["models"].as_array_mut().unwrap().push(model);
    let path = dir.path().join("settings.json");
    fs::write(&path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();

    ConfigService::load(dir.path()).unwrap();
    let first = fs::read_to_string(&path).unwrap();
    ConfigService::load(dir.path()).unwrap();
    let second = fs::read_to_string(&path).unwrap();

    assert_eq!(first, second);
    assert_keys_in_order(&first, &["srv_a", "srv_b", "srv_c"]);
    assert_keys_in_order(&first, &["ENV_A", "ENV_B", "ENV_C"]);
    assert_keys_in_order(&first, &["tool_a", "tool_b", "tool_c"]);
    assert_keys_in_order(&first, &["Ctrl+A", "Ctrl+B", "Ctrl+C"]);
    assert_keys_in_order(&first, &["xk_a", "xk_b", "xk_c"]);
}

#[test]
fn test_unknown_top_level_key_survives_load() {
    let dir = setup_test_dir();
    let path = dir.path().join("settings.json");
    let mut raw: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    raw["zz_future_key"] = serde_json::json!({"a": 1});
    fs::write(&path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();

    ConfigService::load(dir.path()).unwrap();

    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["zz_future_key"], serde_json::json!({"a": 1}));
}

fn models_json(ids: &[&str], first_display_name: &str) -> String {
    let models: Vec<serde_json::Value> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            serde_json::json!({
                "id": id,
                "type": "text",
                "model": "gpt-4",
                "display_name": if i == 0 { first_display_name } else { "Other" },
                "provider": "openai",
                "api_key": "${OPENAI_API_KEY}"
            })
        })
        .collect();
    serde_json::json!({ "models": models }).to_string()
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn model_ids(value: &serde_json::Value) -> Vec<String> {
    value["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn mutate_reloads_external_change_before_saving() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, models_json(&["x", "y"], "Old")).unwrap();
    let mut service = ConfigService::load(dir.path()).expect("load");

    fs::write(&path, models_json(&["x", "z"], "New")).unwrap();
    let outcome = service
        .try_mutate(|c| c.update_setting("theme", serde_json::json!("dark")))
        .expect("mutate");

    assert!(outcome.reloaded);
    let saved = read_json(&path);
    assert_eq!(model_ids(&saved), vec!["x", "z"]);
    assert_eq!(saved["models"][0]["display_name"], "New");
    assert_eq!(saved["theme"], "dark");
    let in_memory: Vec<&str> = service.settings().models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(in_memory, vec!["x", "z"]);
    assert_eq!(service.settings().models[0].display_name, "New");
    assert_eq!(service.settings().theme, "dark");
}

#[test]
fn mutate_without_external_change_does_not_reload() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, models_json(&["x"], "X")).unwrap();
    let mut service = ConfigService::load(dir.path()).expect("load");

    let first = service
        .try_mutate(|c| c.update_setting("theme", serde_json::json!("dark")))
        .expect("mutate");
    assert!(!first.reloaded);
    assert_eq!(read_json(&path)["theme"], "dark");

    let mut external = read_json(&path);
    external["theme"] = serde_json::json!("light");
    fs::write(&path, serde_json::to_string_pretty(&external).unwrap()).unwrap();
    service.reload().expect("reload");

    let second = service
        .try_mutate(|c| c.update_setting("autosave_debounce_ms", serde_json::json!(2000)))
        .expect("mutate");
    assert!(!second.reloaded);
    let saved = read_json(&path);
    assert_eq!(saved["theme"], "light");
    assert_eq!(saved["autosave_debounce_ms"], 2000);
}

#[test]
fn mutate_with_invalid_file_fails_without_writing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, models_json(&["x"], "X")).unwrap();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let theme_before = service.settings().theme.clone();

    fs::write(&path, "{ not json").unwrap();
    let result = service.try_mutate(|c| c.update_setting("theme", serde_json::json!("dark")));

    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{ not json");
    assert_eq!(service.settings().theme, theme_before);
}

#[test]
fn mutate_keeps_settings_symlink() {
    let dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let target = target_dir.path().join("settings.json");
    let link = dir.path().join("settings.json");
    fs::write(&target, models_json(&["x"], "X")).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut service = ConfigService::load(dir.path()).expect("load");

    service
        .try_mutate(|c| c.update_setting("theme", serde_json::json!("dark")))
        .expect("mutate");

    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(read_json(&target)["theme"], "dark");
}

fn load_with_old_icon_keys(
    notifications: Option<bool>,
    widget: Option<bool>,
) -> (TempDir, ConfigService) {
    let dir = setup_test_dir();
    let path = dir.path().join("settings.json");
    let mut raw = read_json(&path);
    if let Some(v) = notifications {
        raw["notifications"]["monochromatic_notification_icons"] = serde_json::json!(v);
    }
    if let Some(v) = widget {
        raw["surfaces"]["speech_to_text"]["monochromatic_widget_icon"] = serde_json::json!(v);
    }
    fs::write(&path, raw.to_string()).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    (dir, service)
}

#[test]
fn appearance_migration_both_false_gives_color() {
    let (_dir, service) = load_with_old_icon_keys(Some(false), Some(false));
    assert_eq!(service.settings().appearance.icons, Skin::Color);
}

#[test]
fn appearance_migration_either_true_gives_mono() {
    for (notifications, widget) in [(true, false), (false, true)] {
        let (_dir, service) = load_with_old_icon_keys(Some(notifications), Some(widget));
        assert_eq!(service.settings().appearance.icons, Skin::Mono);
    }
}

#[test]
fn appearance_migration_absent_keys_keep_defaults() {
    let dir = setup_test_dir();
    let path = dir.path().join("settings.json");
    let mut raw = read_json(&path);
    raw.as_object_mut().unwrap().remove("appearance");
    fs::write(&path, raw.to_string()).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    assert_eq!(service.settings().appearance.logo, Skin::Color);
    assert_eq!(service.settings().appearance.icons, Skin::Mono);
}

#[test]
fn appearance_migration_removes_old_keys_and_survives_reload() {
    let (dir, _service) = load_with_old_icon_keys(Some(false), Some(false));

    let saved = read_json(&dir.path().join("settings.json"));
    assert!(saved["notifications"].get("monochromatic_notification_icons").is_none());
    assert!(saved["surfaces"]["speech_to_text"].get("monochromatic_widget_icon").is_none());
    assert_eq!(saved["appearance"]["icons"], "color");

    let reloaded = ConfigService::load(dir.path()).expect("reload");
    assert_eq!(reloaded.settings().appearance.icons, Skin::Color);
}

#[test]
fn update_setting_appearance_persists() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service
        .try_mutate(|c| c.update_setting("appearance", serde_json::json!({"logo": "mono", "icons": "color"})))
        .expect("mutate");

    let reloaded = ConfigService::load(dir.path()).expect("reload");
    assert_eq!(reloaded.settings().appearance.logo, Skin::Mono);
    assert_eq!(reloaded.settings().appearance.icons, Skin::Color);
}

#[test]
fn test_removed_settings_keys_leave_the_file_on_load() {
    let dir = TempDir::new().unwrap();
    let mut settings = Settings::default();
    let mut model = test_model("m1", "${KEY}");
    model.parameters = Some(Default::default());
    settings.models = vec![model];
    settings.surfaces.chat.generation.model_id = Some("m1".to_string());
    settings.surfaces.quick_actions.generation.model_id = Some("m1".to_string());
    settings.surfaces.title_generation.generation.model_id = Some("m1".to_string());
    settings.keymaps = ["macos", "linux"]
        .iter()
        .map(|os| KeymapGroup {
            context: format!("os == {os}"),
            bindings: Default::default(),
        })
        .collect();
    let clean = serde_json::to_value(&settings).unwrap();

    let mut dirty = clean.clone();
    dirty["code_theme"] = serde_json::json!("github");
    dirty["debug_mode"] = serde_json::json!(true);
    dirty["surfaces"]["quick_actions"]["generation"]["enabled_tools"] = serde_json::json!(["x"]);
    dirty["surfaces"]["title_generation"]["generation"]["enabled_tools"] = serde_json::json!(["x"]);
    dirty["surfaces"]["title_generation"]["generation"]["parameters"] =
        serde_json::json!({"temperature": 0.5});
    dirty["keymaps"].as_array_mut().unwrap().push(serde_json::json!({
        "context": "os == windows",
        "bindings": {"ctrl+f1": "open_context_menu"}
    }));

    let path = dir.path().join("settings.json");
    fs::write(&path, serde_json::to_string_pretty(&dirty).unwrap()).unwrap();
    ConfigService::load(dir.path()).expect("load");
    let first = fs::read_to_string(&path).unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(&first).unwrap(), clean);

    ConfigService::load(dir.path()).expect("reload");
    assert_eq!(fs::read_to_string(&path).unwrap(), first);
}

fn first_model_id(service: &ConfigService) -> String {
    service.settings().models[0].id.clone()
}

#[test]
fn mutate_rejects_empty_model_without_writing() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let id = first_model_id(&service);
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();
    let settings_before = serde_json::to_value(service.settings()).unwrap();

    let err = service
        .mutate(|s| s.settings.models[0].model = String::new())
        .unwrap_err();

    match err {
        ConfigError::InvalidSettings { key, .. } => {
            assert_eq!(key, Some(format!("models[{id}].model")));
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
    assert_eq!(serde_json::to_value(service.settings()).unwrap(), settings_before);
}

#[test]
fn mutate_writes_valid_added_model() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");

    service
        .mutate(|s| s.add_model(test_model("second", "${KEY}")))
        .expect("mutate");

    let ids = model_ids(&read_json(&dir.path().join("settings.json")));
    assert!(ids.contains(&"second".to_string()), "{ids:?}");
}

#[test]
fn try_mutate_error_leaves_file_and_settings_unchanged() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();
    let settings_before = serde_json::to_value(service.settings()).unwrap();

    let result: Result<Mutated<()>, ConfigError> = service.try_mutate(|s| {
        s.update_setting("theme", serde_json::json!("light")).unwrap();
        Err(ConfigError::invalid("nope"))
    });

    assert!(result.is_err());
    assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
    assert_eq!(serde_json::to_value(service.settings()).unwrap(), settings_before);
}

#[test]
fn check_changed_rejects_out_of_range_retention() {
    let before = Settings::default();
    let mut after = before.clone();
    after.history_retention_days = 4000;

    let err = catalog::check_changed(&before, &after).unwrap_err();

    match err {
        ConfigError::InvalidSettings { key, .. } => {
            assert_eq!(key.as_deref(), Some("history_retention_days"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn service_with_linux_ctrl_f1() -> (TempDir, ConfigService) {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service
        .try_mutate(|c| {
            c.settings.keymaps = vec![keymap_group("os == linux", &[])];
            c.add_shortcut_for_os("linux", "open_context_menu", "ctrl+f1")
        })
        .expect("bind");
    (dir, service)
}

fn invalid_key_and_message(err: ConfigError) -> (Option<String>, String) {
    match err {
        ConfigError::InvalidSettings { key, message } => (key, message),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn update_setting_launch_at_startup_writes_file() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");

    service
        .try_mutate(|c| c.update_setting("launch_at_startup", serde_json::json!(true)))
        .expect("write");

    let file = read_json(&dir.path().join("settings.json"));
    assert_eq!(file["launch_at_startup"], serde_json::json!(true));
}

#[test]
fn add_shortcut_rejects_collision_in_same_os_set_ignoring_case() {
    for shortcut in ["ctrl+f1", "CTRL+F1"] {
        let (dir, mut service) = service_with_linux_ctrl_f1();
        let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();

        let err = service
            .try_mutate(|c| c.add_shortcut_for_os("linux", "clear_context", shortcut))
            .unwrap_err();

        let (key, message) = invalid_key_and_message(err);
        assert_eq!(key.as_deref(), Some("keymaps.clear_context"));
        assert!(message.contains("open_context_menu"), "{message}");
        assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
    }
}

#[test]
fn add_shortcut_accepts_same_shortcut_in_other_os_set() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();

    service
        .try_mutate(|c| c.add_shortcut_for_os("macos", "clear_context", "ctrl+f1"))
        .expect("other os set");
}

fn two_linux_groups_service() -> (TempDir, ConfigService) {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service
        .try_mutate(|c| {
            c.settings.keymaps = vec![
                keymap_group("os == linux", &[("ctrl+f1", "open_context_menu")]),
                keymap_group("os==linux", &[("ctrl+f3", "clear_context")]),
            ];
            Ok(())
        })
        .expect("seed");
    (dir, service)
}

fn keymap_group(context: &str, bindings: &[(&str, &str)]) -> KeymapGroup {
    KeymapGroup {
        context: context.to_string(),
        bindings: bindings.iter().map(|(k, a)| (k.to_string(), a.to_string())).collect(),
    }
}

fn bindings_of(service: &ConfigService, index: usize) -> HashMap<String, String> {
    service.settings().keymaps[index].bindings.clone()
}

#[test]
fn add_shortcut_keeps_the_other_shortcuts_of_the_action() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();

    service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "open_context_menu", "shift+f1"))
        .expect("add");

    let linux = service.settings().keymaps.iter().find(|g| g.context == "os == linux").unwrap();
    assert_eq!(linux.bindings.get("ctrl+f1").map(String::as_str), Some("open_context_menu"));
    assert_eq!(linux.bindings.get("shift+f1").map(String::as_str), Some("open_context_menu"));
}

#[test]
fn add_shortcut_rejects_collision_in_a_second_matching_group() {
    let (dir, mut service) = two_linux_groups_service();
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();

    let err = service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "open_context_menu", "CTRL+F3"))
        .unwrap_err();

    let (key, message) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("keymaps.open_context_menu"));
    assert_eq!(message, "CTRL+F3 is already used by clear_context");
    assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
}

#[test]
fn add_shortcut_of_the_same_action_with_other_case_changes_nothing() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();

    service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "open_context_menu", "Ctrl+F1"))
        .expect("no-op");

    let linux = service.settings().keymaps.iter().find(|g| g.context == "os == linux").unwrap();
    assert_eq!(linux.bindings.keys().collect::<Vec<_>>(), vec!["ctrl+f1"]);
}

#[test]
fn add_shortcut_reports_another_action_even_when_the_same_action_also_matches() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();
    service
        .try_mutate(|c| {
            c.settings.keymaps = vec![keymap_group(
                "os == linux",
                &[("ctrl+f1", "open_context_menu"), ("CTRL+F1", "clear_context")],
            )];
            Ok(())
        })
        .expect("seed");

    let err = service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "open_context_menu", "ctrl+f1"))
        .unwrap_err();

    let (key, message) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("keymaps.open_context_menu"));
    assert_eq!(message, "ctrl+f1 is already used by clear_context");
}

#[test]
fn add_shortcut_rejects_an_invalid_shortcut() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();

    let err = service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "open_context_menu", "f5"))
        .unwrap_err();

    let (key, message) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("keymaps.open_context_menu"));
    assert_eq!(message, "f5 is not a valid shortcut");
}

#[test]
fn add_shortcut_rejects_an_unknown_action() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();

    let err = service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "no_such_action", "ctrl+f5"))
        .unwrap_err();

    let (key, message) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("keymaps.no_such_action"));
    assert_eq!(message, "unknown shortcut action");
}

#[test]
fn add_shortcut_creates_the_group_of_the_os_when_missing() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let macos = keymap_group("os == macos", &[("cmd+f1", "open_context_menu")]);
    service
        .try_mutate(|c| {
            c.settings.keymaps = vec![macos.clone()];
            Ok(())
        })
        .expect("seed");

    service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "clear_context", "ctrl+f2"))
        .expect("add");

    let keymaps = &service.settings().keymaps;
    assert_eq!(keymaps.len(), 2);
    assert_eq!(keymaps[0].context, macos.context);
    assert_eq!(keymaps[0].bindings, macos.bindings);
    assert_eq!(keymaps[1].context, "os == linux");
    assert_eq!(
        keymaps[1].bindings,
        HashMap::from([("ctrl+f2".to_string(), "clear_context".to_string())])
    );
}

#[test]
fn remove_shortcut_leaves_the_other_shortcuts_of_the_action() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();
    service
        .try_mutate(|c| c.add_shortcut_for_os("linux", "open_context_menu", "shift+f1"))
        .expect("add");

    service
        .try_mutate(|c| c.remove_shortcut_for_os("linux", "open_context_menu", "shift+f1"))
        .expect("remove");

    let linux = service.settings().keymaps.iter().find(|g| g.context == "os == linux").unwrap();
    assert_eq!(linux.bindings.keys().collect::<Vec<_>>(), vec!["ctrl+f1"]);
}

#[test]
fn remove_shortcut_reaches_a_second_matching_group() {
    let (_dir, mut service) = two_linux_groups_service();

    service
        .try_mutate(|c| c.remove_shortcut_for_os("linux", "clear_context", "ctrl+f3"))
        .expect("remove");

    assert_eq!(
        bindings_of(&service, 0),
        HashMap::from([("ctrl+f1".to_string(), "open_context_menu".to_string())])
    );
    assert!(bindings_of(&service, 1).is_empty());
}

#[test]
fn remove_shortcut_not_bound_to_the_action_is_rejected() {
    let (dir, mut service) = service_with_linux_ctrl_f1();
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();

    for (action, shortcut) in [("open_context_menu", "alt+f9"), ("clear_context", "ctrl+f1")] {
        let err = service
            .try_mutate(|c| c.remove_shortcut_for_os("linux", action, shortcut))
            .unwrap_err();

        let (key, message) = invalid_key_and_message(err);
        assert_eq!(key.as_deref(), Some(format!("keymaps.{action}").as_str()));
        assert_eq!(message, format!("{shortcut} is not bound to {action}"));
        assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
    }
}

#[test]
fn remove_shortcut_ignores_case() {
    let (_dir, mut service) = service_with_linux_ctrl_f1();

    service
        .try_mutate(|c| c.remove_shortcut_for_os("linux", "open_context_menu", "CTRL+F1"))
        .expect("remove");

    let linux = service.settings().keymaps.iter().find(|g| g.context == "os == linux").unwrap();
    assert!(linux.bindings.is_empty());
}

#[test]
fn update_setting_wrong_type_is_rejected_with_key() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();

    let err = service
        .try_mutate(|c| c.update_setting("recent_apps_count", serde_json::json!("x")))
        .unwrap_err();

    let (key, _) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("recent_apps_count"));
    assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
}

#[test]
fn update_setting_menu_digit_select_delay_checks_range() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();

    let err = service
        .try_mutate(|c| c.update_setting("menu_digit_select_delay_ms", serde_json::json!(50)))
        .unwrap_err();

    let (key, _) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("menu_digit_select_delay_ms"));
    assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);

    service
        .try_mutate(|c| c.update_setting("menu_digit_select_delay_ms", serde_json::json!(300)))
        .expect("in range");
    let written = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert!(written.contains("\"menu_digit_select_delay_ms\": 300"), "{written}");
}

#[test]
fn update_setting_mcp_servers_rejects_invalid_name() {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    let bytes_before = fs::read(dir.path().join("settings.json")).unwrap();

    let err = service
        .try_mutate(|c| {
            c.update_setting("mcp_servers", serde_json::json!({"bad name": {"command": "x"}}))
        })
        .unwrap_err();

    let (key, _) = invalid_key_and_message(err);
    assert_eq!(key.as_deref(), Some("mcp_servers.bad name"));
    assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), bytes_before);
}

fn settings_json_without_env_vars() -> serde_json::Value {
    let mut settings = Settings::default();
    settings.models = vec![
        test_model("m1", "${OPENAI_API_KEY}"),
        test_model("m2", "${OPENAI_API_KEY}"),
        test_model("m3", "${OPENAI_API_KEY}"),
        test_model("m4", "${ENIGMA_API_KEY}"),
        test_model("m5", "${OPENROUTER_API_KEY}"),
        test_stt_model("m6", "${ELEVENLABS_API_KEY}"),
    ];
    settings.surfaces.chat.generation.model_id = Some("m1".to_string());
    settings.surfaces.quick_actions.generation.model_id = Some("m1".to_string());
    settings.surfaces.title_generation.generation.model_id = Some("m1".to_string());
    settings.surfaces.speech_to_text.keyterms_file = Some("${STT_KEYTERMS_FILE}".to_string());
    settings.surfaces.speech_to_text.prompt = Some("${STT_PROMPT_FILE}".to_string());
    let mut raw = serde_json::to_value(&settings).unwrap();
    raw["mcp_servers"] = serde_json::json!({
        "promptheus": {
            "command": "bun",
            "args": ["${PROMPTHEUS_MCP_PATH}/src/index.ts"],
            "env": {
                "LLM_API_KEY": "${OPENAI_API_KEY}",
                "TAVILY_API_KEY": "${TAVILY_API_KEY}"
            }
        }
    });
    raw.as_object_mut().unwrap().remove("env_vars");
    raw
}

fn load_written_settings(raw: &serde_json::Value) -> (TempDir, ConfigService, String) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, serde_json::to_string_pretty(raw).unwrap()).unwrap();
    let service = ConfigService::load(dir.path()).expect("load");
    let written = fs::read_to_string(&path).unwrap();
    (dir, service, written)
}

#[test]
fn env_vars_are_seeded_from_references_when_the_key_is_absent() {
    let (_dir, service, written) = load_written_settings(&settings_json_without_env_vars());

    let env_vars = &service.settings().env_vars;
    assert_eq!(env_vars.len(), 8);
    let secret: Vec<&str> = env_vars
        .iter()
        .filter(|var| var.secret)
        .map(|var| var.name.as_str())
        .collect();
    assert_eq!(
        secret,
        [
            "ELEVENLABS_API_KEY",
            "ENIGMA_API_KEY",
            "OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "TAVILY_API_KEY"
        ]
    );
    let plain: Vec<&str> = env_vars
        .iter()
        .filter(|var| !var.secret)
        .map(|var| var.name.as_str())
        .collect();
    assert_eq!(plain, ["PROMPTHEUS_MCP_PATH", "STT_KEYTERMS_FILE", "STT_PROMPT_FILE"]);
    assert!(env_vars
        .iter()
        .all(|var| var.description.is_empty() && var.renamed_from.is_empty()));
    assert!(serde_json::from_str::<serde_json::Value>(&written).unwrap()["env_vars"].is_array());
}

#[test]
fn seeded_env_vars_are_stable_on_the_next_load() {
    let (dir, first_service, first) = load_written_settings(&settings_json_without_env_vars());

    let second_service = ConfigService::load(dir.path()).expect("reload");

    assert_eq!(second_service.settings().env_vars, first_service.settings().env_vars);
    assert_eq!(fs::read_to_string(dir.path().join("settings.json")).unwrap(), first);
}

#[test]
fn existing_env_vars_are_not_reseeded() {
    let custom = serde_json::json!([{
        "name": "CUSTOM_VAR",
        "description": "mine",
        "secret": true,
        "renamed_from": ["OLD_VAR"]
    }]);
    for list in [custom.clone(), serde_json::json!([])] {
        let mut raw = settings_json_without_env_vars();
        raw["env_vars"] = list.clone();

        let (_dir, service, written) = load_written_settings(&raw);

        assert_eq!(serde_json::to_value(&service.settings().env_vars).unwrap(), list);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&written).unwrap()["env_vars"], list);
    }
}

#[test]
fn env_vars_follow_webview_providers_in_the_written_file() {
    let (_dir, _service, written) = load_written_settings(&settings_json_without_env_vars());

    assert_keys_in_order(&written, &["webview_providers", "env_vars"]);
}

#[test]
fn check_settings_document_accepts_valid_settings() {
    let default_json = include_str!("../../../resources/default_settings.json");
    assert!(check_settings_document(default_json).is_ok());
}

#[test]
fn check_settings_document_rejects_a_conflict_marker() {
    let default_json = include_str!("../../../resources/default_settings.json");
    let broken = format!("<<<<<<< HEAD\n{default_json}");
    assert!(matches!(
        check_settings_document(&broken),
        Err(ConfigError::JsonParse(_))
    ));
}

#[test]
fn check_settings_document_creates_no_file() {
    let dir = TempDir::new().unwrap();
    let default_json = include_str!("../../../resources/default_settings.json");
    check_settings_document(default_json).unwrap();
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

const LAST_GOOD_NAME: &str = "Last Good";

fn broken_settings_with_last_good_copy() -> (TempDir, String) {
    let dir = setup_test_dir();
    let mut service = ConfigService::load(dir.path()).expect("load");
    service
        .try_mutate(|c| c.update_setting("preferred_name", serde_json::json!(LAST_GOOD_NAME)))
        .expect("mutate");
    let valid = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let broken = format!("<<<<<<< HEAD\n{valid}");
    fs::write(dir.path().join("settings.json"), &broken).unwrap();
    (dir, broken)
}

#[test]
fn load_writes_last_good_copy_equal_to_settings_json() {
    let dir = setup_test_dir();
    ConfigService::load(dir.path()).expect("load");

    assert_eq!(
        fs::read(dir.path().join("settings.last-good.json")).unwrap(),
        fs::read(dir.path().join("settings.json")).unwrap()
    );
}

#[test]
fn load_with_broken_settings_starts_on_last_good_copy_and_refuses_writes() {
    let (dir, broken) = broken_settings_with_last_good_copy();

    let mut service = ConfigService::load(dir.path()).expect("fallback load");

    assert_eq!(service.settings().preferred_name, LAST_GOOD_NAME);
    assert_eq!(fs::read_to_string(dir.path().join("settings.json")).unwrap(), broken);
    let fallback = service.fallback().expect("fallback set");
    assert_eq!(fallback.path, dir.path().join("settings.json").display().to_string());
    assert!(fallback.error.contains("JSON parse error"), "{}", fallback.error);

    let write = service.try_mutate(|c| c.update_setting("theme", serde_json::json!("dark")));
    assert!(write.is_err());
    assert_eq!(fs::read_to_string(dir.path().join("settings.json")).unwrap(), broken);
}

#[test]
fn reload_after_fixing_settings_json_clears_the_fallback() {
    let (dir, broken) = broken_settings_with_last_good_copy();
    let mut service = ConfigService::load(dir.path()).expect("fallback load");
    let valid = broken.strip_prefix("<<<<<<< HEAD\n").unwrap();

    assert!(service.reload().is_err());
    assert!(service.fallback().is_some());

    fs::write(dir.path().join("settings.json"), valid).unwrap();
    service.reload().expect("reload");

    assert!(service.fallback().is_none());
    assert_eq!(service.settings().preferred_name, LAST_GOOD_NAME);
}

#[test]
fn load_with_broken_settings_and_no_last_good_copy_fails() {
    let dir = setup_test_dir();
    fs::write(dir.path().join("settings.json"), "<<<<<<< HEAD\n{}").unwrap();

    assert!(ConfigService::load(dir.path()).is_err());
}
