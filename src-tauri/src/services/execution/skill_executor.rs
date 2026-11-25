use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Local;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::parameters::merge_optional_parameters;
use super::stream::run_stream_loop;
use super::system_prompt::build_system_prompt_base;
use crate::services::placeholder_registry::PlaceholderContext;
use crate::models::ai::StreamEvent;
use crate::models::history::{InvocationSource, OutputRoute, SerializedConversationNode};
use crate::models::message::{AppliedSkill, ProcessedMessage};
use crate::models::settings::ModelParameters;
use crate::services::ai::AiService;
use crate::services::clipboard::ClipboardService;
use crate::services::config::ConfigService;
use crate::services::context::ContextManagerService;
use crate::services::execution::lifecycle::PromptExecutionService;
use crate::services::notification::{NotificationLevel, NotificationService};
use crate::services::recent_apps::RecentAppsState;
use crate::services::skill::SkillService;
use crate::services::skill_message;
use crate::services::sqlite_history::SqliteHistoryService;
use crate::Error;

pub async fn run_skill_stream(
    app: &AppHandle,
    skill_name: String,
    input_override: Option<String>,
    invocation_source: InvocationSource,
    output_route: OutputRoute,
    notify_success: bool,
    on_event: Channel<StreamEvent>,
) -> crate::Result<Option<String>> {
    let start_time = Instant::now();
    let config = app.state::<Arc<Mutex<ConfigService>>>();
    let clipboard = app.state::<ClipboardService>();
    let ai = app.state::<Arc<Mutex<AiService>>>();
    let skill_service = app.state::<Arc<Mutex<SkillService>>>();
    let context = app.state::<Arc<Mutex<ContextManagerService>>>();
    let history = app.state::<Arc<Mutex<SqliteHistoryService>>>();
    let notifications = app.state::<NotificationService>();
    let prompt_execution = app.state::<Arc<Mutex<PromptExecutionService>>>();
    let recent_apps = app.state::<Arc<RecentAppsState>>();
    let active_app = recent_apps.active().await;
    let recent_apps_display = recent_apps.display().await;

    let prepared = match {
        let mut prompt_execution = prompt_execution.lock().await;
        let config_guard = config.lock().await;
        let mut skill_service_guard = skill_service.lock().await;
        let history_guard = history.lock().await;
        let ai_guard = ai.lock().await;
        let context_guard = context.lock().await;
        SkillExecutor::prepare(
            &mut prompt_execution,
            &config_guard,
            &clipboard,
            &mut skill_service_guard,
            history_guard.conn(),
            &ai_guard,
            &context_guard,
            &active_app,
            &recent_apps_display,
            &skill_name,
            input_override,
        )
    } {
        Ok(prepared) => prepared,
        Err(error) => {
            if !notify_success {
                notifications.notify(
                    "prompt_execution_error",
                    NotificationLevel::Error,
                    "Execution failed",
                    Some(error.to_string()),
                    config.lock().await.settings(),
                );
            }
            return Err(error);
        }
    };

    let live_execution = prompt_execution
        .lock()
        .await
        .start_live(
            &prepared.execution_id,
            prepared.user_display_text.clone(),
            on_event,
        )
        .0;

    let _ = app.emit(
        "execution-started",
        ExecutionStartedPayload {
            execution_id: prepared.execution_id.clone(),
            skill_id: Some(skill_name.clone()),
        },
    );

    let stream_result = match prepared
        .ai
        .complete_stream(
            &prepared.model_id,
            prepared.messages,
            prepared.param_overrides,
            None,
            vec![],
        )
        .await
    {
        Ok(stream) => run_stream_loop(stream, live_execution, Some(prepared.cancel_rx), "").await,
        Err(e) => Err(e.into()),
    };

    let mut prompt_execution = prompt_execution.lock().await;
    let config_guard = config.lock().await;

    match stream_result {
        Ok(result) => {
            let entry_id = SkillExecutor::record_result(
                &*history.lock().await,
                &clipboard,
                &notifications,
                &config_guard,
                &mut prompt_execution,
                app,
                &skill_name,
                &prepared.skill_display_name,
                prepared.skill_version_id,
                &prepared.input_content,
                prepared.user_display_text,
                result.full_text,
                start_time.elapsed(),
                &prepared.model_display_name,
                &prepared.execution_id,
                invocation_source,
                output_route,
                notify_success,
            );
            Ok(Some(entry_id))
        }
        Err(error) => {
            let message = error.to_string();
            let is_cancelled = message == "Cancelled";
            SkillExecutor::record_error(
                &notifications,
                &config_guard,
                &mut prompt_execution,
                app,
                &prepared.execution_id,
                message,
                is_cancelled,
            );
            if is_cancelled {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

pub struct SkillPrepared {
    pub execution_id: String,
    pub cancel_rx: tokio::sync::watch::Receiver<bool>,
    pub model_id: String,
    pub model_display_name: String,
    pub skill_display_name: String,
    pub skill_version_id: i64,
    pub input_content: String,
    pub user_display_text: String,
    pub messages: Vec<ProcessedMessage>,
    pub ai: AiService,
    pub param_overrides: Option<ModelParameters>,
}

pub struct SkillExecutor;

impl SkillExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        prompt_execution: &mut PromptExecutionService,
        config: &ConfigService,
        clipboard: &ClipboardService,
        skill_service: &mut SkillService,
        history_conn: &rusqlite::Connection,
        ai: &AiService,
        context: &ContextManagerService,
        active_app: &str,
        recent_apps: &str,
        skill_name: &str,
        input_override: Option<String>,
    ) -> crate::Result<SkillPrepared> {
        let (execution_id, cancel_rx) =
            prompt_execution.start_skill_execution(skill_name.to_string())?;

        let skill_version_id = match skill_service.ensure_version(skill_name, history_conn) {
            Ok(id) => id,
            Err(e) => {
                prompt_execution.finish_skill_execution();
                return Err(Error::from(e));
            }
        };

        let skill = match skill_message::resolve_skill_or_err(skill_service, skill_name) {
            Ok(s) => s,
            Err(e) => {
                prompt_execution.finish_skill_execution();
                return Err(Error::from(e));
            }
        };
        let skill_display_name = skill.display_name.clone();

        let model_id = match PromptExecutionService::resolve_quick_action_model(
            config,
            skill.model.as_deref(),
        ) {
            Ok(id) => id,
            Err(e) => {
                prompt_execution.finish_skill_execution();
                return Err(Error::from(e));
            }
        };

        let model_display_name = config
            .settings()
            .models
            .iter()
            .find(|m| m.id == model_id)
            .map(|m| m.display_name.clone())
            .unwrap_or_else(|| model_id.clone());

        let input_content = match input_override {
            Some(text) => text,
            None => match clipboard.get_text() {
                Ok(text) => text,
                Err(e) => {
                    prompt_execution.finish_skill_execution();
                    return Err(Error::from(e));
                }
            },
        };

        let placeholder_ctx = PlaceholderContext::with_apps(active_app, recent_apps);
        let system_prompt = build_system_prompt_base(config, None, &placeholder_ctx);
        let messages = skill_message::prepare_skill_messages(
            &system_prompt,
            &skill,
            &input_content,
            context,
            &placeholder_ctx,
        );

        let ai = ai.clone();
        let surface_params = config
            .settings()
            .surfaces
            .quick_actions
            .generation
            .parameters
            .clone();
        let skill_params = skill.parameters.as_ref().map(ModelParameters::from_map);
        let param_overrides = merge_optional_parameters(Some(surface_params), skill_params);

        let user_display_text = format!("/{skill_name} {input_content}");

        Ok(SkillPrepared {
            execution_id,
            cancel_rx,
            model_id,
            model_display_name,
            skill_display_name,
            skill_version_id,
            input_content,
            user_display_text,
            messages,
            ai,
            param_overrides,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_result(
        history: &SqliteHistoryService,
        clipboard: &ClipboardService,
        notifications: &NotificationService,
        config: &ConfigService,
        prompt_execution: &mut PromptExecutionService,
        app: &AppHandle,
        skill_name: &str,
        skill_display_name: &str,
        skill_version_id: i64,
        input_content: &str,
        user_display_text: String,
        full_text: String,
        elapsed: Duration,
        model_display_name: &str,
        execution_id: &str,
        invocation_source: InvocationSource,
        output_route: OutputRoute,
        notify_success: bool,
    ) -> String {
        let _ = clipboard.set_text(&full_text);
        let now = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let user_node_id = format!("skill-user-{}", Uuid::new_v4());
        let assistant_node_id = format!("skill-asst-{}", Uuid::new_v4());

        let user_node = SerializedConversationNode {
            node_id: user_node_id.clone(),
            parent_id: None,
            role: "user".to_string(),
            content: user_display_text,
            timestamp: now.clone(),
            children: vec![assistant_node_id.clone()],
            updates: vec![],
            prompt_tokens: None,
            completion_tokens: None,
            thinking: None,
            thinking_duration: None,
            query_duration: None,
            error: None,
            cancelled: false,
            tool_calls: vec![],
            text_attachments: vec![],
            sources: vec![],
            applied_skills: vec![AppliedSkill {
                name: skill_name.to_string(),
                skill_version_id,
                input: input_content.to_string(),
            }],
        };

        let assistant_node = SerializedConversationNode {
            node_id: assistant_node_id.clone(),
            parent_id: Some(user_node_id.clone()),
            role: "assistant".to_string(),
            content: full_text,
            timestamp: now,
            children: vec![],
            updates: vec![],
            prompt_tokens: None,
            completion_tokens: None,
            thinking: None,
            thinking_duration: None,
            query_duration: Some(elapsed.as_secs_f64()),
            error: None,
            cancelled: false,
            tool_calls: vec![],
            text_attachments: vec![],
            sources: vec![],
            applied_skills: vec![],
        };

        let added_id = history.add_conversation_entry(
            String::new(),
            Some(skill_name.to_string()),
            Some(skill_display_name.to_string()),
            true,
            None,
            vec![user_node, assistant_node],
            Some(user_node_id.clone()),
            vec![user_node_id, assistant_node_id],
            true,
            None,
            vec![],
            None,
            None,
            invocation_source,
            output_route,
        );
        let _ = crate::services::history_events::emit_history_changed(
            app,
            Some(added_id.clone()),
            None,
        );

        if notify_success {
            notifications.notify(
                "prompt_execution_success",
                NotificationLevel::Success,
                skill_display_name.to_string(),
                Some(format!(
                    "{model_display_name} · {:.1}s · copied to clipboard",
                    elapsed.as_secs_f64()
                )),
                config.settings(),
            );
        }

        prompt_execution.finish_skill_execution();
        let _ = app.emit(
            "execution-completed",
            ExecutionCompletedPayload {
                execution_id: execution_id.to_string(),
                success: true,
                error: None,
                cancelled: false,
            },
        );
        added_id
    }

    pub fn record_error(
        notifications: &NotificationService,
        config: &ConfigService,
        prompt_execution: &mut PromptExecutionService,
        app: &AppHandle,
        execution_id: &str,
        message: String,
        is_cancelled: bool,
    ) {
        if is_cancelled {
            notifications.notify(
                "prompt_execution_cancel",
                NotificationLevel::Info,
                "Prompt cancelled",
                None::<&str>,
                config.settings(),
            );
        } else {
            notifications.notify(
                "prompt_execution_error",
                NotificationLevel::Error,
                "Execution failed",
                Some(message.as_str()),
                config.settings(),
            );
        }

        prompt_execution.finish_skill_execution();
        let _ = app.emit(
            "execution-completed",
            ExecutionCompletedPayload {
                execution_id: execution_id.to_string(),
                success: false,
                error: Some(message),
                cancelled: is_cancelled,
            },
        );
    }
}

#[derive(Clone, serde::Serialize)]
pub struct ExecutionStartedPayload {
    pub execution_id: String,
    pub skill_id: Option<String>,
}

#[derive(Clone, serde::Serialize)]
pub struct ExecutionCompletedPayload {
    pub execution_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub cancelled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::context::ContextManagerService;
    use crate::services::database::Database;
    use crate::services::skill::SkillService;
    use tempfile::TempDir;

    fn setup_skill_service(name: &str, model: &str) -> (TempDir, SkillService) {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_path = skill_dir.join("SKILL.md");
        let body = format!(
            "---\nname: {name}\ndescription: A test skill\nmodel: {model}\n---\n\nDo the thing."
        );
        std::fs::write(&skill_path, body).unwrap();

        let svc = SkillService::load(dir.path(), &[]).unwrap();
        (dir, svc)
    }

    fn setup_config_with_model(model_id: &str) -> (TempDir, ConfigService) {
        let dir = TempDir::new().unwrap();
        let mut settings = crate::models::settings::Settings {
            models: vec![crate::models::settings::ModelConfig {
                id: model_id.to_string(),
                model: "gpt-4".to_string(),
                display_name: "Model".to_string(),
                model_type: crate::models::settings::ModelType::Text,
                provider: Some(Default::default()),
                group: None,
                api_key: Some("test-key".to_string()),
                base_url: None,
                parameters: None,
                context_window_size: None,
                api_mode: None,
                capabilities: None,
                store: true,
            }],
            ..Default::default()
        };
        settings.surfaces.quick_actions.generation.model_id = Some(model_id.to_string());
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, serde_json::to_string(&settings).unwrap()).unwrap();
        let config = ConfigService::load(dir.path()).unwrap();
        (dir, config)
    }

    #[test]
    fn prepare_happy_path_returns_skill_metadata() {
        let mut svc = PromptExecutionService::new();
        let (_skill_dir, mut skill_service) = setup_skill_service("translate", "test-model");
        let (_cfg_dir, config) = setup_config_with_model("test-model");

        let db_dir = TempDir::new().unwrap();
        let database = Database::open(db_dir.path()).unwrap();

        let ai = AiService::new(&config.settings().models);
        let context = ContextManagerService::new();
        let clipboard = ClipboardService::without_app();

        let prepared = SkillExecutor::prepare(
            &mut svc,
            &config,
            &clipboard,
            &mut skill_service,
            database.conn(),
            &ai,
            &context,
            "",
            "",
            "translate",
            Some("hello".to_string()),
        );

        let prepared = prepared.expect("prepare should succeed");
        assert_eq!(prepared.input_content, "hello");
        assert_eq!(prepared.user_display_text, "/translate hello");
        assert!(svc.is_busy());
    }
}
