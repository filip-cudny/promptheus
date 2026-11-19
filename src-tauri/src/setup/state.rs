use std::path::Path;
use std::sync::Arc;

use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

use crate::services::ai::AiService;
use crate::services::clipboard::ClipboardService;
use crate::services::config::ConfigService;
use crate::services::context::ContextManagerService;
use crate::services::database::Database;
use crate::services::dock::DockManager;
use crate::services::execution::PromptExecutionService;
use crate::services::history_events::HistoryVersion;
use crate::services::history_search::HistorySearch;
use crate::services::mcp::McpRegistry;
use crate::services::menu_coordinator::MenuCoordinator;
use crate::services::notification::NotificationService;
use crate::services::recent_apps::RecentAppsState;
use crate::services::settings_agent::engine::SharedSession;
use crate::services::settings_agent::transcript;
use crate::services::settings_sync::SettingsSync;
use crate::services::shell_env::get_shell_env;
use crate::services::skill::SkillService;
use crate::services::speech::widget::RecordingWidget;
use crate::services::speech::{AudioClipStore, SpeechService};
use crate::services::sqlite_history::SqliteHistoryService;
use crate::services::{self, change_log, conversation_context, ui_state};

pub fn manage(
    app: &tauri::App,
    config_dir: &Path,
    config_service: ConfigService,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    app.manage(DockManager::new());
    app.manage(services::ai_webview::AiWebviewState::default());

    let skills_dir = config_dir.join("skills");
    let skill_service = SkillService::load(
        &skills_dir,
        &config_service.settings().skills_order,
    )
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    log::info!(
        "loaded {} skills from {}",
        skill_service.list_skills().len(),
        skills_dir.display()
    );

    let clipboard_service = ClipboardService::new(app.handle().clone());
    let notification_service = NotificationService::new(app.handle().clone());
    let ui_state_service = ui_state::UiStateService::load(config_dir)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let ai_service = AiService::new(&config_service.settings().models);
    let context_service = ContextManagerService::new();
    let app_data_dir = app.path().app_data_dir()?;
    let database =
        Database::open(&app_data_dir).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    skill_service
        .prune_missing_skills(database.conn())
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    let retention_days = config_service.settings().history_retention_days;
    let history_service = SqliteHistoryService::new(database, retention_days);
    let pruned = history_service.enforce_retention();
    if pruned > 0 {
        log::info!(
            "history retention: pruned {} entries older than {} days",
            pruned,
            retention_days
        );
    }
    let change_log_pruned = change_log::prune(
        &change_log::change_log_dir(config_dir),
        &change_log::secret_backup_dir(config_dir),
        retention_days,
        chrono::Utc::now(),
    );
    if change_log_pruned > 0 {
        log::info!(
            "change log retention: pruned {} entries older than {} days",
            change_log_pruned,
            retention_days
        );
    }
    let transcripts_pruned = transcript::prune(
        &app_data_dir.join("settings-agent-turns"),
        retention_days,
        chrono::Utc::now(),
    );
    if transcripts_pruned > 0 {
        log::info!(
            "settings agent transcripts: pruned {} older than {} days",
            transcripts_pruned,
            retention_days
        );
    }
    let audio_clip_store = AudioClipStore::new(&app_data_dir);
    audio_clip_store
        .initialize()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    let swept = audio_clip_store.sweep(history_service.conn());
    if swept > 0 {
        log::info!("audio clip startup sweep removed {} clips", swept);
    }

    let mcp_servers_config = config_service.settings().mcp_servers.clone();
    let mcp_registry = tauri::async_runtime::block_on(McpRegistry::start_all(&mcp_servers_config));

    app.manage(Arc::new(Mutex::new(config_service)));
    app.manage(clipboard_service);
    app.manage(notification_service);
    app.manage(services::missing_env::MissingEnvToastState::default());
    app.manage(Arc::new(Mutex::new(MenuCoordinator::default())));
    app.manage(Arc::new(Mutex::new(context_service)));
    app.manage(Arc::new(Mutex::new(ai_service)));
    app.manage(Arc::new(Mutex::new(history_service)));
    app.manage(Arc::new(Mutex::new(HistorySearch::new())));
    app.manage(HistoryVersion::new());
    app.manage(Arc::new(mcp_registry));
    app.manage(Arc::new(Mutex::new(PromptExecutionService::new())));
    app.manage(Arc::new(Mutex::new(skill_service)));
    app.manage(Arc::new(Mutex::new(SpeechService::new())));
    app.manage(RecordingWidget::new());
    app.manage(Arc::new(audio_clip_store));
    app.manage(Arc::new(Mutex::new(ui_state_service)));
    app.manage(Arc::new(Mutex::new(
        conversation_context::ConversationContextCache::new(),
    )));
    app.manage(Arc::new(RecentAppsState::new()));
    app.manage(SharedSession::default());
    app.manage(Arc::new(SettingsSync::new(
        config_dir.to_path_buf(),
        Arc::new(|| get_shell_env().clone()),
        None,
        None,
    )));

    if !mcp_servers_config.is_empty() {
        let _ = app.emit("mcp-ready", ());
    }

    Ok(())
}
