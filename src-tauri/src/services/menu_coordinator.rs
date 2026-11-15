use crate::models::context::ContextItem;
use crate::models::menu::{MenuItem, MenuItemType};
use crate::models::skill::{is_offered_in, OfferedIn, SkillSummary};
use crate::services::config::ConfigService;

#[derive(Default)]
pub struct MenuState {
    pub skills: Vec<SkillSummary>,
    pub context_items: Vec<ContextItem>,
    pub is_recording: bool,
    pub is_action_executing: bool,
}

#[derive(Default)]
pub struct MenuCoordinator {
    pub state: MenuState,
}

impl MenuCoordinator {
    pub fn get_menu_items(&self, config: &ConfigService) -> Vec<MenuItem> {
        let section_order = &config.settings().menu_section_order;
        let total_sections = section_order.len();
        let mut all_items: Vec<MenuItem> = Vec::new();

        for (section_index, section_id) in section_order.iter().enumerate() {
            let is_last_section = section_index == total_sections - 1;

            let mut section_items = match section_id.as_str() {
                "chat" => self.build_chat_items(),
                "skills" | "prompts" | "SkillProvider" => self.build_skill_items(),
                "models" => self.build_models_items(config),
                "settings" => self.build_settings_items(),
                "speech" | "SpeechMenuProvider" => self.build_speech_items(),
                "lastInteraction" | "LastInteractionMenuProvider" => {
                    self.build_last_interaction_items()
                }
                "context" | "ContextMenuProvider" => self.build_context_items(),
                _ => Vec::new(),
            };

            if section_items.is_empty() {
                continue;
            }

            for item in &mut section_items {
                item.section_id = Some(section_id.clone());
            }

            all_items.extend(section_items);

            if !is_last_section {
                if let Some(last) = all_items.last_mut() {
                    last.separator_after = true;
                }
            }
        }

        all_items
    }

    fn build_chat_items(&self) -> Vec<MenuItem> {
        vec![MenuItem::new("__chat__", "Chat", MenuItemType::Chat)]
    }

    fn build_skill_items(&self) -> Vec<MenuItem> {
        self.state
            .skills
            .iter()
            .filter(|skill| is_offered_in(&skill.offered_in, OfferedIn::ContextMenu))
            .map(|skill| {
                MenuItem::new(&skill.name, &skill.display_name, MenuItemType::Skill)
                    .with_data(serde_json::json!({
                        "skill_id": skill.name,
                        "skill_name": skill.display_name,
                    }))
                    .with_tooltip(skill.description.clone())
            })
            .collect()
    }

    fn build_speech_items(&self) -> Vec<MenuItem> {
        let state = &self.state;
        let (label, icon) = if state.is_recording {
            ("Stop Recording", "square")
        } else {
            ("Speech to text", "mic")
        };

        vec![MenuItem::new("system_speech_to_text", label, MenuItemType::Speech)
            .with_data(serde_json::json!({"type": "speech_to_text"}))
            .with_enabled(state.is_recording || !state.is_action_executing)
            .with_icon(icon)]
    }

    fn build_last_interaction_items(&self) -> Vec<MenuItem> {
        vec![MenuItem::new(
            "last_interaction_section",
            "Last interaction",
            MenuItemType::LastInteraction,
        )]
    }

    fn build_context_items(&self) -> Vec<MenuItem> {
        vec![MenuItem::new("environment_section", "Context", MenuItemType::Context)
            .with_data(serde_json::json!({ "items": self.state.context_items }))]
    }

    fn build_models_items(&self, config: &ConfigService) -> Vec<MenuItem> {
        let settings = config.settings();
        if settings.models.is_empty() {
            return Vec::new();
        }

        let default_model_id = settings
            .surfaces
            .quick_actions
            .generation
            .model_id
            .clone()
            .or_else(|| settings.surfaces.chat.generation.model_id.clone());

        let default_reasoning_effort = settings
            .surfaces
            .quick_actions
            .generation
            .parameters
            .reasoning_effort
            .clone();

        let models: Vec<serde_json::Value> = settings
            .models
            .iter()
            .filter(|m| m.is_text())
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "display_name": m.display_name,
                    "model": m.model,
                    "provider": m.provider,
                    "group": m.group,
                })
            })
            .collect();

        let stt_models: Vec<serde_json::Value> = settings
            .models
            .iter()
            .filter(|m| m.is_stt())
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "display_name": m.display_name,
                    "model": m.model,
                    "provider": m.provider,
                    "group": m.group,
                })
            })
            .collect();

        vec![MenuItem::new("__models__", "Models", MenuItemType::Models).with_data(
            serde_json::json!({
                "models": models,
                "default_model_id": default_model_id,
                "default_reasoning_effort": default_reasoning_effort,
                "stt_models": stt_models,
                "speech_to_text_model_id": settings.surfaces.speech_to_text.model_id,
            }),
        )]
    }

    fn build_settings_items(&self) -> Vec<MenuItem> {
        vec![MenuItem::new(
            "settings_section",
            "Settings",
            MenuItemType::SettingsSection,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config_service() -> ConfigService {
        let dir = tempfile::TempDir::new().unwrap();
        let default_json = include_str!("config/test_fixture_settings.json");
        std::fs::write(dir.path().join("settings.json"), default_json).unwrap();
        let svc = ConfigService::load(dir.path()).unwrap();
        std::mem::forget(dir);
        svc
    }

    fn config_with_section_order(order: &[&str]) -> ConfigService {
        let dir = tempfile::TempDir::new().unwrap();
        let mut settings: serde_json::Value =
            serde_json::from_str(include_str!("config/test_fixture_settings.json")).unwrap();
        settings["menu_section_order"] = serde_json::json!(order);
        std::fs::write(dir.path().join("settings.json"), settings.to_string()).unwrap();
        let svc = ConfigService::load(dir.path()).unwrap();
        std::mem::forget(dir);
        svc
    }

    fn fixed_state_coordinator() -> MenuCoordinator {
        let skill = |name: &str, display_name: &str, description: Option<&str>, offered_in| SkillSummary {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: description.map(str::to_string),
            offered_in,
        };

        MenuCoordinator {
            state: MenuState {
                skills: vec![
                    skill("summarize", "Summarize", Some("Short summary"), None),
                    skill("pill-only", "Pill only", None, Some(vec![OfferedIn::Pill])),
                    skill("translate", "Translate", None, Some(vec![OfferedIn::ContextMenu])),
                ],
                context_items: vec![
                    ContextItem::Text { number: 1, content: "hello".into() },
                    ContextItem::Image { number: 1, data: "img".into(), media_type: "image/png".into() },
                ],
                is_recording: false,
                is_action_executing: true,
            },
        }
    }

    fn expected_full_menu() -> serde_json::Value {
        let item = |id: &str, label: &str, item_type: &str, data: serde_json::Value, section: &str, separator: bool| {
            serde_json::json!({
                "id": id,
                "label": label,
                "item_type": item_type,
                "data": data,
                "enabled": true,
                "separator_after": separator,
                "tooltip": null,
                "icon": null,
                "section_id": section,
            })
        };
        let mut speech = item(
            "system_speech_to_text",
            "Speech to text",
            "speech",
            serde_json::json!({"type": "speech_to_text"}),
            "speech",
            true,
        );
        speech["enabled"] = serde_json::json!(false);
        speech["icon"] = serde_json::json!("mic");
        let mut summarize = item(
            "summarize",
            "Summarize",
            "skill",
            serde_json::json!({"skill_id": "summarize", "skill_name": "Summarize"}),
            "skills",
            false,
        );
        summarize["tooltip"] = serde_json::json!("Short summary");
        let models = serde_json::json!({
            "models": [
                {"id": "550e8400-e29b-41d4-a716-446655440000", "display_name": "gpt-5.4", "model": "gpt-5.4", "provider": "openai", "group": null},
                {"id": "b2c3d4e5-f6a7-4b8c-9d0e-1f2a3b4c5d6e", "display_name": "gpt-4.1-mini", "model": "gpt-4.1-mini", "provider": "openai", "group": null},
            ],
            "default_model_id": "550e8400-e29b-41d4-a716-446655440000",
            "default_reasoning_effort": null,
            "stt_models": [
                {"id": "c3d4e5f6-a7b8-4c9d-8e0f-1a2b3c4d5e6f", "display_name": "gpt-4o-transcribe", "model": "gpt-4o-transcribe", "provider": null, "group": null},
            ],
            "speech_to_text_model_id": "c3d4e5f6-a7b8-4c9d-8e0f-1a2b3c4d5e6f",
        });
        serde_json::json!([
            item("__chat__", "Chat", "chat", serde_json::Value::Null, "chat", true),
            summarize,
            item(
                "translate",
                "Translate",
                "skill",
                serde_json::json!({"skill_id": "translate", "skill_name": "Translate"}),
                "skills",
                true,
            ),
            item("__models__", "Models", "models", models, "models", true),
            speech,
            item("last_interaction_section", "Last interaction", "last_interaction", serde_json::Value::Null, "lastInteraction", true),
            item(
                "environment_section",
                "Context",
                "context",
                serde_json::json!({"items": [
                    {"item_type": "text", "number": 1, "content": "hello"},
                    {"item_type": "image", "number": 1, "data": "img", "media_type": "image/png"},
                ]}),
                "context",
                true,
            ),
            item("settings_section", "Settings", "settings_section", serde_json::Value::Null, "settings", false),
        ])
    }

    #[test]
    fn full_menu_serializes_to_expected_json() {
        let config = config_with_section_order(&[
            "chat", "skills", "models", "speech", "lastInteraction", "context", "settings",
        ]);

        let items = fixed_state_coordinator().get_menu_items(&config);

        assert_eq!(serde_json::to_value(items).unwrap(), expected_full_menu());
    }

    #[test]
    fn fresh_install_without_models_builds_menu_without_models_section() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = ConfigService::load(dir.path()).unwrap();

        let items = fixed_state_coordinator().get_menu_items(&config);

        assert!(config.settings().models.is_empty());
        assert!(items.iter().all(|i| i.id != "__models__"));
    }

    fn skill(name: &str, offered_in: Option<Vec<OfferedIn>>) -> SkillSummary {
        SkillSummary {
            name: name.to_string(),
            display_name: name.to_string(),
            description: None,
            offered_in,
        }
    }

    fn section_ids(items: &[MenuItem]) -> Vec<&str> {
        items.iter().filter_map(|i| i.section_id.as_deref()).collect()
    }

    #[test]
    fn default_section_order_builds_sections_in_order() {
        let coordinator = MenuCoordinator {
            state: MenuState {
                skills: vec![skill("p-1", None)],
                ..MenuState::default()
            },
        };

        let items = coordinator.get_menu_items(&make_config_service());

        assert_eq!(
            section_ids(&items),
            vec!["ContextMenuProvider", "LastInteractionMenuProvider", "prompts", "SpeechMenuProvider", "settings"]
        );
    }

    #[test]
    fn separator_after_last_item_of_every_non_final_section() {
        let items = MenuCoordinator::default().get_menu_items(&make_config_service());

        let (last, others) = items.split_last().unwrap();
        assert!(!last.separator_after);
        assert_eq!(last.item_type, MenuItemType::SettingsSection);
        assert!(others.iter().all(|i| i.separator_after));
    }

    #[test]
    fn skills_section_keeps_only_context_menu_skills() {
        let coordinator = MenuCoordinator {
            state: MenuState {
                skills: vec![
                    skill("pill-only", Some(vec![OfferedIn::Pill])),
                    skill("nowhere", Some(vec![])),
                    skill("everywhere", None),
                    skill("menu", Some(vec![OfferedIn::ContextMenu])),
                ],
                ..MenuState::default()
            },
        };
        let config = config_with_section_order(&["skills", "settings"]);

        let ids: Vec<String> = coordinator
            .get_menu_items(&config)
            .into_iter()
            .filter(|i| i.item_type == MenuItemType::Skill)
            .map(|i| i.id)
            .collect();

        assert_eq!(ids, vec!["everywhere", "menu"]);
    }

    #[test]
    fn empty_skills_skip_the_section() {
        let config = config_with_section_order(&["skills", "settings"]);

        let items = MenuCoordinator::default().get_menu_items(&config);

        assert_eq!(section_ids(&items), vec!["settings"]);
        assert!(!items[0].separator_after);
    }

    #[test]
    fn speech_item_follows_recording_and_executing_state() {
        let config = config_with_section_order(&["speech"]);
        let speech_item = |is_recording, is_action_executing| {
            let coordinator = MenuCoordinator {
                state: MenuState { is_recording, is_action_executing, ..MenuState::default() },
            };
            coordinator.get_menu_items(&config).remove(0)
        };

        let idle = speech_item(false, false);
        assert_eq!((idle.label.as_str(), idle.icon.as_deref(), idle.enabled), ("Speech to text", Some("mic"), true));

        let recording = speech_item(true, false);
        assert_eq!((recording.label.as_str(), recording.icon.as_deref(), recording.enabled), ("Stop Recording", Some("square"), true));

        assert!(!speech_item(false, true).enabled);

        let recording_while_executing = speech_item(true, true);
        assert!(recording_while_executing.enabled);
        assert_eq!(recording_while_executing.label, "Stop Recording");
    }

    #[test]
    fn context_section_lists_items_and_is_present_when_empty() {
        let config = config_with_section_order(&["context"]);

        let empty = MenuCoordinator::default().get_menu_items(&config);
        assert_eq!(empty[0].data.as_ref().unwrap()["items"], serde_json::json!([]));

        let coordinator = MenuCoordinator {
            state: MenuState {
                context_items: vec![ContextItem::Text { number: 1, content: "hello".into() }],
                ..MenuState::default()
            },
        };
        let items = coordinator.get_menu_items(&config);
        assert_eq!(items[0].id, "environment_section");
        assert_eq!(items[0].data.as_ref().unwrap()["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn only_settings_without_state() {
        let config = config_with_section_order(&["chat", "settings"]);
        let items = MenuCoordinator::default().get_menu_items(&config);
        assert_eq!(section_ids(&items), vec!["chat", "settings"]);

        let items = MenuCoordinator::default().get_menu_items(&make_config_service());
        assert!(items.iter().any(|i| i.item_type == MenuItemType::SettingsSection));
    }

    #[test]
    fn unknown_section_id_is_skipped() {
        let config = config_with_section_order(&["nope", "settings"]);

        let items = MenuCoordinator::default().get_menu_items(&config);

        assert_eq!(section_ids(&items), vec!["settings"]);
    }
}
