use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

use crate::models::settings::{
    ModelConfig, NotificationSettings, Settings, SpeechToTextConfig,
};
use crate::commands::skills::{emit_changed as emit_skills_changed, rescan_and_prune};
use crate::services::ai::AiService;
use crate::services::change_log;
use crate::services::config::catalog::{CatalogEntry, ENTRIES};
use crate::services::config::{ConfigError, ConfigService, SettingsFallback, SurfaceKind};
use crate::services::history_events::emit_history_changed;
use crate::services::missing_env::{collect_missing_enabled, env_is_set, MissingEnvRef};
use crate::services::settings_agent::transcript;
use crate::services::skill::SkillService;
use crate::services::sqlite_history::SqliteHistoryService;

static SETTINGS_VERSION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Clone)]
struct SettingsChangedEvent {
    version: u64,
}

pub(crate) fn emit_changed(app: &AppHandle) -> crate::Result<()> {
    let version = SETTINGS_VERSION.fetch_add(1, Ordering::Relaxed) + 1;
    app.emit("settings-changed", SettingsChangedEvent { version })?;
    Ok(())
}

pub(crate) fn rebuild_ai(config: &ConfigService, ai: &mut AiService) {
    *ai = AiService::new(&config.settings().models);
}

pub(crate) fn apply_reloaded_settings(app: &AppHandle, settings: &Settings) {
    crate::reload_shortcuts(app, settings);
    crate::services::autostart::reconcile(app, settings, false);
    crate::services::dock_icon::apply(app, settings.appearance.logo);
}

pub(crate) async fn apply_settings_reload(app: &AppHandle, reason: &str) -> crate::Result<()> {
    let config = app.state::<Arc<Mutex<ConfigService>>>();
    let ai = app.state::<Arc<Mutex<AiService>>>();
    let skills = app.state::<Arc<Mutex<SkillService>>>();
    let history = app.state::<Arc<Mutex<SqliteHistoryService>>>();

    let config_dir = app.path().app_config_dir()?;
    let settings = {
        let guard = config.lock().await;
        crate::services::secrets::copy_renamed_values(&config_dir, guard.settings());
        rebuild_ai(&guard, &mut *ai.lock().await);
        guard.settings().clone()
    };
    rescan_and_prune(&skills, &config, &history).await?;
    apply_reloaded_settings(app, &settings);
    if reason == "sync" {
        crate::services::missing_env::check_and_toast(app, &settings);
    }
    emit_changed(app)?;
    emit_skills_changed(app, reason)
}

pub(crate) async fn finish_mutation(app: &AppHandle, reloaded: bool) -> crate::Result<()> {
    let logo = app
        .state::<Arc<Mutex<ConfigService>>>()
        .lock()
        .await
        .settings()
        .appearance
        .logo;
    crate::services::dock_icon::apply(app, logo);
    if reloaded {
        apply_settings_reload(app, "reload").await
    } else {
        emit_changed(app)
    }
}

pub(crate) enum SettingWrite<'a> {
    Setting(&'a str),
    Shortcut,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reapply {
    pub shortcuts: bool,
    pub autostart: bool,
}

pub(crate) fn reapply_for(write: SettingWrite) -> Reapply {
    Reapply {
        shortcuts: matches!(write, SettingWrite::Shortcut),
        autostart: matches!(write, SettingWrite::Setting("launch_at_startup")),
    }
}

async fn try_mutate_settings(
    app: &AppHandle,
    config: &Mutex<ConfigService>,
    write: SettingWrite<'_>,
    f: impl FnOnce(&mut ConfigService) -> Result<(), ConfigError>,
) -> crate::Result<()> {
    let (reloaded, settings) = {
        let mut guard = config.lock().await;
        let reloaded = guard.try_mutate(f)?.reloaded;
        (reloaded, guard.settings().clone())
    };
    let reapply = reapply_for(write);
    if reapply.autostart {
        crate::services::autostart::reconcile(app, &settings, false);
    }
    if reapply.shortcuts {
        crate::reload_shortcuts(app, &settings);
    }
    finish_mutation(app, reloaded).await
}

async fn mutate_settings(
    app: &AppHandle,
    config: &Mutex<ConfigService>,
    f: impl FnOnce(&mut ConfigService),
) -> crate::Result<()> {
    let reloaded = config.lock().await.mutate(f)?.reloaded;
    finish_mutation(app, reloaded).await
}

#[tauri::command]
pub async fn get_settings(
    config: State<'_, Arc<Mutex<ConfigService>>>,
) -> crate::Result<Settings> {
    Ok(config.lock().await.settings().clone())
}

#[tauri::command]
pub async fn get_missing_env_refs(
    config: State<'_, Arc<Mutex<ConfigService>>>,
) -> crate::Result<Vec<MissingEnvRef>> {
    let settings = config.lock().await.settings().clone();
    Ok(collect_missing_enabled(&settings, env_is_set))
}

#[tauri::command]
pub async fn get_settings_fallback(
    config: State<'_, Arc<Mutex<ConfigService>>>,
) -> crate::Result<Option<SettingsFallback>> {
    Ok(config.lock().await.fallback().cloned())
}

#[tauri::command]
pub async fn retry_settings_load(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
) -> crate::Result<Option<SettingsFallback>> {
    let reloaded = config.lock().await.reload();
    match reloaded {
        Ok(()) => {
            log::info!("settings.json reloaded on retry");
            apply_settings_reload(&app, "retry").await?;
            Ok(None)
        }
        Err(e) => match config.lock().await.fallback().cloned() {
            Some(fallback) => {
                log::warn!("settings retry failed: {}", fallback.error);
                Ok(Some(fallback))
            }
            None => Err(e.into()),
        },
    }
}

#[tauri::command]
pub async fn get_settings_catalog() -> crate::Result<&'static [CatalogEntry]> {
    Ok(ENTRIES)
}

#[tauri::command]
pub async fn update_setting(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    key: String,
    value: serde_json::Value,
) -> crate::Result<()> {
    try_mutate_settings(&app, &config, SettingWrite::Setting(&key), |c| {
        c.update_setting(&key, value)
    })
    .await
}

#[tauri::command]
pub async fn add_shortcut(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    action: String,
    shortcut: String,
) -> crate::Result<()> {
    try_mutate_settings(&app, &config, SettingWrite::Shortcut, |c| {
        c.add_shortcut(&action, &shortcut)
    })
    .await
}

#[tauri::command]
pub async fn remove_shortcut(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    action: String,
    shortcut: String,
) -> crate::Result<()> {
    try_mutate_settings(&app, &config, SettingWrite::Shortcut, |c| {
        c.remove_shortcut(&action, &shortcut)
    })
    .await
}

#[tauri::command]
pub async fn update_surface_model(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    surface: SurfaceKind,
    model_id: Option<String>,
) -> crate::Result<()> {
    mutate_settings(&app, &config, |c| c.update_surface_model(surface, model_id)).await
}

#[tauri::command]
pub async fn update_surface_parameter(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    surface: SurfaceKind,
    key: String,
    value: serde_json::Value,
) -> crate::Result<()> {
    mutate_settings(&app, &config, |c| {
        c.update_surface_parameter(surface, &key, value)
    })
    .await
}

#[tauri::command]
pub async fn update_surface_enabled_tools(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    surface: SurfaceKind,
    tools: Vec<String>,
) -> crate::Result<()> {
    mutate_settings(&app, &config, |c| {
        c.update_surface_enabled_tools(surface, tools)
    })
    .await
}

#[tauri::command]
pub async fn set_surface_tool(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    surface: SurfaceKind,
    tool: String,
    enabled: bool,
) -> crate::Result<()> {
    mutate_settings(&app, &config, |c| c.set_surface_tool(surface, &tool, enabled)).await
}

#[tauri::command]
pub async fn update_speech_to_text_config(
    app: AppHandle,
    config_svc: State<'_, Arc<Mutex<ConfigService>>>,
    config: SpeechToTextConfig,
) -> crate::Result<()> {
    mutate_settings(&app, &config_svc, |c| c.update_speech_to_text(config)).await
}

async fn mutate_models(
    app: &AppHandle,
    config: &Mutex<ConfigService>,
    ai: &Mutex<AiService>,
    f: impl FnOnce(&mut ConfigService),
) -> crate::Result<()> {
    let reloaded = {
        let mut svc = config.lock().await;
        let reloaded = svc.mutate(f)?.reloaded;
        rebuild_ai(&svc, &mut *ai.lock().await);
        reloaded
    };
    finish_mutation(app, reloaded).await
}

#[tauri::command]
pub async fn add_model(
    app: AppHandle,
    config_svc: State<'_, Arc<Mutex<ConfigService>>>,
    ai: State<'_, Arc<Mutex<AiService>>>,
    config: ModelConfig,
) -> crate::Result<()> {
    mutate_models(&app, &config_svc, &ai, |c| c.add_model(config)).await
}

#[tauri::command]
pub async fn update_model(
    app: AppHandle,
    config_svc: State<'_, Arc<Mutex<ConfigService>>>,
    ai: State<'_, Arc<Mutex<AiService>>>,
    model_id: String,
    config: ModelConfig,
) -> crate::Result<()> {
    mutate_models(&app, &config_svc, &ai, |c| c.update_model(&model_id, config)).await
}

#[tauri::command]
pub async fn delete_model(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    ai: State<'_, Arc<Mutex<AiService>>>,
    model_id: String,
) -> crate::Result<()> {
    mutate_models(&app, &config, &ai, |c| c.delete_model(&model_id)).await
}

#[tauri::command]
pub async fn update_notifications(
    app: AppHandle,
    config_svc: State<'_, Arc<Mutex<ConfigService>>>,
    config: NotificationSettings,
) -> crate::Result<()> {
    mutate_settings(&app, &config_svc, |c| c.update_notifications(config)).await
}

#[tauri::command]
pub async fn update_history_retention(
    app: AppHandle,
    config_svc: State<'_, Arc<Mutex<ConfigService>>>,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    days: u32,
) -> crate::Result<()> {
    mutate_settings(&app, &config_svc, |c| c.update_history_retention(days)).await?;

    let pruned = {
        let mut history = history.lock().await;
        tokio::task::block_in_place(|| {
            history.set_retention_days(days);
            let pruned = history.enforce_retention();
            if pruned > 0 {
                history.compact_after_bulk_delete();
            }
            pruned
        })
    };

    log::info!(
        "history retention set to {} days, pruned {} entries",
        days,
        pruned
    );

    prune_change_log(&app, days);
    prune_settings_agent_transcripts(&app, days);

    if pruned > 0 {
        emit_history_changed(&app, None, None)?;
    }
    Ok(())
}

fn prune_change_log(app: &AppHandle, days: u32) {
    let config_dir = match app.path().app_config_dir() {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!("change log retention: config dir unavailable: {e}");
            return;
        }
    };
    let pruned = tokio::task::block_in_place(|| {
        change_log::prune(
            &change_log::change_log_dir(&config_dir),
            &change_log::secret_backup_dir(&config_dir),
            days,
            chrono::Utc::now(),
        )
    });
    if pruned > 0 {
        log::info!("change log retention: pruned {} entries older than {} days", pruned, days);
    }
}

fn prune_settings_agent_transcripts(app: &AppHandle, days: u32) {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!("settings agent transcripts retention: app data dir unavailable: {e}");
            return;
        }
    };
    let pruned = tokio::task::block_in_place(|| {
        transcript::prune(&app_data_dir.join("settings-agent-turns"), days, chrono::Utc::now())
    });
    if pruned > 0 {
        log::info!("settings agent transcripts: pruned {} older than {} days", pruned, days);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_at_startup_reapplies_autostart_only() {
        let reapply = reapply_for(SettingWrite::Setting("launch_at_startup"));
        assert_eq!(reapply, Reapply { shortcuts: false, autostart: true });
    }

    #[test]
    fn shortcut_write_reapplies_shortcuts() {
        assert!(reapply_for(SettingWrite::Shortcut).shortcuts);
    }
}
