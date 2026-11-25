use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::watch;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::models::ai::{Source, StreamEvent, ToolCall, ToolCallStatus};
use crate::services::config::ConfigService;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("Execution already in progress")]
    AlreadyExecuting,

    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

#[derive(Clone, Serialize)]
pub struct ExecutionSnapshot {
    pub execution_id: String,
    pub user_message: String,
    pub accumulated_text: String,
    pub accumulated_thinking: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub is_thinking: bool,
    pub finished: bool,
    pub error: Option<String>,
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub sources: Vec<Source>,
}

pub struct LiveExecution {
    pub snapshot: ExecutionSnapshot,
    pub channel: Option<Channel<StreamEvent>>,
}

impl LiveExecution {
    pub fn send(&mut self, event: StreamEvent) {
        if let Some(channel) = &self.channel {
            if channel.send(event).is_err() {
                self.channel = None;
            }
        }
    }

    pub fn finish_with_error(&mut self, message: String) {
        self.snapshot.finished = true;
        self.snapshot.error = Some(message.clone());
        self.send(StreamEvent::Error { message });
    }

    pub fn finish_done(
        &mut self,
        full_text: String,
        full_thinking: Option<String>,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
    ) {
        self.snapshot.finished = true;
        self.snapshot.prompt_tokens = prompt_tokens;
        self.snapshot.completion_tokens = completion_tokens;
        self.send(StreamEvent::Done {
            full_text,
            full_thinking,
            prompt_tokens,
            completion_tokens,
            sources: self.snapshot.sources.clone(),
        });
    }

    pub fn add_source(&mut self, source: Source) {
        if !self.snapshot.sources.iter().any(|s| s.url == source.url) {
            self.snapshot.sources.push(source);
        }
    }

    pub fn finish_tool_call(
        &mut self,
        tool_call_id: String,
        result: Option<String>,
        error: Option<String>,
    ) {
        if let Some(tc) = self
            .snapshot
            .tool_calls
            .iter_mut()
            .find(|t| t.tool_call_id == tool_call_id)
        {
            tc.status = if error.is_some() {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };
            tc.result.clone_from(&result);
            tc.error.clone_from(&error);
            tc.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
        self.send(StreamEvent::ToolCallDone {
            tool_call_id,
            result,
            error,
        });
    }
}

pub struct PromptExecutionService {
    executing_skill_id: Option<String>,
    cancel_sender: Option<watch::Sender<bool>>,
    live_cancel_sender: Option<watch::Sender<bool>>,
    pub live: Option<Arc<TokioMutex<LiveExecution>>>,
}

impl PromptExecutionService {
    pub fn new() -> Self {
        Self {
            executing_skill_id: None,
            cancel_sender: None,
            live_cancel_sender: None,
            live: None,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.executing_skill_id.is_some()
    }

    #[cfg(test)]
    pub fn executing_skill_id(&self) -> Option<&str> {
        self.executing_skill_id.as_deref()
    }

    pub fn start_skill_execution(
        &mut self,
        skill_id: String,
    ) -> Result<(String, watch::Receiver<bool>), ExecutionError> {
        if self.is_busy() {
            return Err(ExecutionError::AlreadyExecuting);
        }
        let execution_id = Uuid::new_v4().to_string();
        let (tx, rx) = watch::channel(false);
        self.executing_skill_id = Some(skill_id);
        self.cancel_sender = Some(tx);
        Ok((execution_id, rx))
    }

    pub fn finish_skill_execution(&mut self) {
        self.executing_skill_id = None;
        self.cancel_sender = None;
        self.live = None;
    }

    pub fn cancel_execution(&mut self) -> bool {
        if let Some(sender) = &self.cancel_sender {
            let _ = sender.send(true);
            return true;
        }
        false
    }

    pub fn cancel_live(&mut self) -> bool {
        if let Some(sender) = &self.live_cancel_sender {
            let _ = sender.send(true);
            return true;
        }
        false
    }

    pub fn clear_live(&mut self) {
        self.live = None;
        self.live_cancel_sender = None;
    }

    pub fn start_live(
        &mut self,
        execution_id: &str,
        user_message: String,
        channel: Channel<StreamEvent>,
    ) -> (Arc<TokioMutex<LiveExecution>>, watch::Receiver<bool>) {
        let (tx, rx) = watch::channel(false);
        let live = Arc::new(TokioMutex::new(LiveExecution {
            snapshot: ExecutionSnapshot {
                execution_id: execution_id.to_string(),
                user_message,
                accumulated_text: String::new(),
                accumulated_thinking: None,
                tool_calls: Vec::new(),
                is_thinking: false,
                finished: false,
                error: None,
                prompt_tokens: None,
                completion_tokens: None,
                sources: Vec::new(),
            },
            channel: Some(channel),
        }));
        self.live = Some(Arc::clone(&live));
        self.live_cancel_sender = Some(tx);
        (live, rx)
    }

    pub fn resolve_model(
        config: &ConfigService,
        model_id: Option<&str>,
    ) -> Result<String, ExecutionError> {
        resolve(config, model_id, Surface::Chat)
    }

    pub fn resolve_quick_action_model(
        config: &ConfigService,
        model_id: Option<&str>,
    ) -> Result<String, ExecutionError> {
        resolve(config, model_id, Surface::QuickActions)
    }

    pub fn resolve_title_generation_model(
        config: &ConfigService,
    ) -> Result<String, ExecutionError> {
        resolve(config, None, Surface::TitleGeneration)
    }
}

#[derive(Clone, Copy)]
enum Surface {
    Chat,
    QuickActions,
    TitleGeneration,
}

fn resolve(
    config: &ConfigService,
    explicit: Option<&str>,
    surface: Surface,
) -> Result<String, ExecutionError> {
    let settings = config.settings();
    if let Some(id) = explicit {
        return if settings.models.iter().any(|m| m.id == id) {
            Ok(id.to_string())
        } else {
            Err(ExecutionError::ModelNotFound(id.to_string()))
        };
    }
    let surfaces = &settings.surfaces;
    let chat = surfaces.chat.generation.model_id.clone();
    let (configured, label) = match surface {
        Surface::Chat => (chat, "chat"),
        Surface::QuickActions => (
            surfaces.quick_actions.generation.model_id.clone().or(chat),
            "quick action",
        ),
        Surface::TitleGeneration => (
            surfaces.title_generation.generation.model_id.clone().or(chat),
            "title generation",
        ),
    };
    configured
        .ok_or_else(|| ExecutionError::ModelNotFound(format!("no {label} model configured")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_config() -> (TempDir, ConfigService) {
        setup_config_with(|_| {})
    }

    fn setup_config_with(
        configure: impl FnOnce(&mut crate::models::settings::Settings),
    ) -> (TempDir, ConfigService) {
        let dir = TempDir::new().unwrap();
        let mut settings = crate::models::settings::Settings {
            models: vec![crate::models::settings::ModelConfig {
                id: "model-1".to_string(),
                model: "gpt-4".to_string(),
                display_name: "GPT-4".to_string(),
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
        settings.surfaces.chat.generation.model_id = Some("model-1".to_string());
        configure(&mut settings);
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, serde_json::to_string(&settings).unwrap()).unwrap();
        let config = ConfigService::load(dir.path()).unwrap();
        (dir, config)
    }

    #[test]
    fn start_skill_execution_succeeds_when_idle() {
        let mut svc = PromptExecutionService::new();
        assert!(!svc.is_busy());
        let (id, _rx) = svc.start_skill_execution("test-skill".to_string()).unwrap();
        assert!(!id.is_empty());
        assert!(svc.is_busy());
        assert_eq!(svc.executing_skill_id(), Some("test-skill"));
    }

    #[test]
    fn start_skill_execution_fails_when_busy() {
        let mut svc = PromptExecutionService::new();
        svc.start_skill_execution("skill-1".to_string()).unwrap();
        let result = svc.start_skill_execution("skill-2".to_string());
        assert!(matches!(result, Err(ExecutionError::AlreadyExecuting)));
    }

    #[test]
    fn finish_skill_execution_resets_state() {
        let mut svc = PromptExecutionService::new();
        svc.start_skill_execution("test-skill".to_string()).unwrap();
        assert!(svc.is_busy());
        svc.finish_skill_execution();
        assert!(!svc.is_busy());
        assert!(svc.executing_skill_id().is_none());
    }

    #[test]
    fn cancel_execution_sends_signal() {
        let mut svc = PromptExecutionService::new();
        let (_id, mut rx) = svc.start_skill_execution("test-skill".to_string()).unwrap();
        assert!(!*rx.borrow());
        assert!(svc.cancel_execution());
        assert!(rx.has_changed().unwrap());
        assert!(*rx.borrow_and_update());
    }

    #[test]
    fn cancel_execution_returns_false_when_idle() {
        let mut svc = PromptExecutionService::new();
        assert!(!svc.cancel_execution());
    }

    #[test]
    fn resolve_model_falls_back_to_default() {
        let (_dir, config) = setup_config();
        let result = PromptExecutionService::resolve_model(&config, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "model-1");
    }

    #[test]
    fn resolve_model_validates_explicit_id() {
        let (_dir, config) = setup_config();
        let result = PromptExecutionService::resolve_model(&config, Some("model-1"));
        assert!(result.is_ok());

        let result = PromptExecutionService::resolve_model(&config, Some("nonexistent"));
        assert!(matches!(result, Err(ExecutionError::ModelNotFound(_))));
    }

    #[test]
    fn resolve_quick_action_model_prefers_surface_then_chat() {
        let (_dir, config) = setup_config();
        let result = PromptExecutionService::resolve_quick_action_model(&config, None);
        assert_eq!(result.unwrap(), "model-1");

        let (_dir, config) = setup_config_with(|settings| {
            settings.surfaces.quick_actions.generation.model_id = Some("quick".to_string());
        });
        let result = PromptExecutionService::resolve_quick_action_model(&config, None);
        assert_eq!(result.unwrap(), "quick");

        let result =
            PromptExecutionService::resolve_quick_action_model(&config, Some("nonexistent"));
        assert!(matches!(result, Err(ExecutionError::ModelNotFound(_))));
    }

    #[test]
    fn resolve_title_generation_model_prefers_surface_then_chat() {
        let (_dir, config) = setup_config();
        let result = PromptExecutionService::resolve_title_generation_model(&config);
        assert_eq!(result.unwrap(), "model-1");

        let (_dir, config) = setup_config_with(|settings| {
            settings.surfaces.title_generation.generation.model_id = Some("title".to_string());
        });
        let result = PromptExecutionService::resolve_title_generation_model(&config);
        assert_eq!(result.unwrap(), "title");
    }

    #[test]
    fn finish_allows_restart() {
        let mut svc = PromptExecutionService::new();
        let (id1, _rx1) = svc.start_skill_execution("skill-1".to_string()).unwrap();
        svc.finish_skill_execution();
        let (id2, _rx2) = svc.start_skill_execution("skill-2".to_string()).unwrap();
        assert_ne!(id1, id2);
    }
}
