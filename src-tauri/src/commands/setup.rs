use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::commands::settings::finish_mutation;
use crate::models::settings::{ModelConfig, Settings};
use crate::services::ai::AiService;
use crate::services::change_log::TestResult;
use crate::services::config::{ConfigService, SurfaceKind};
use crate::services::missing_env::env_is_set;
use crate::services::settings_agent::testing::test_model_config;
use crate::services::setup::{assign_setup_model as assign, SetupSession};
use crate::services::ui_state::UiStateService;

pub const SETUP_STEP_KEY: &str = "settings-dialog.setup.step";
pub const SETUP_FINISHED_KEY: &str = "settings-dialog.setup.finished";
const FIRST_STEP: u8 = 1;
const LAST_STEP: u8 = 5;

#[derive(Serialize)]
pub struct SetupStatus {
    active: bool,
    step: u8,
}

#[tauri::command]
pub async fn get_setup_status(
    session: State<'_, SetupSession>,
    ui_state: State<'_, Arc<Mutex<UiStateService>>>,
) -> crate::Result<SetupStatus> {
    let step = ui_state
        .lock()
        .await
        .get(SETUP_STEP_KEY)
        .and_then(|value| value.as_u64())
        .and_then(|step| u8::try_from(step).ok())
        .filter(|step| (FIRST_STEP..=LAST_STEP).contains(step))
        .unwrap_or(FIRST_STEP);
    Ok(SetupStatus { active: session.is_active(), step })
}

#[tauri::command]
pub async fn finish_setup(
    session: State<'_, SetupSession>,
    ui_state: State<'_, Arc<Mutex<UiStateService>>>,
) -> crate::Result<()> {
    ui_state.lock().await.set(SETUP_FINISHED_KEY, serde_json::json!(true))?;
    session.finish();
    log::info!("setup finished");
    Ok(())
}

#[tauri::command]
pub async fn test_model(app: AppHandle, model: ModelConfig) -> crate::Result<TestResult> {
    let config_dir = app.path().app_config_dir()?;
    let models = [model];
    let factory = |_: &Settings| AiService::new(&models);
    let result = test_model_config(&models[0], &config_dir, &factory).await;
    log::info!("model test finished (ok: {})", result.ok);
    Ok(result)
}

#[tauri::command]
pub async fn assign_setup_model(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    model_id: String,
) -> crate::Result<Option<SurfaceKind>> {
    let mutated = config.lock().await.try_mutate(|c| {
        let mut probe = c.settings().clone();
        let surface = assign(&mut probe, &model_id, env_is_set);
        if let Some(surface) = surface {
            c.update_surface_model(surface, Some(model_id.clone()));
        }
        Ok(surface)
    })?;
    if let Some(surface) = mutated.value {
        log::info!("setup model assigned to {surface:?}");
    }
    finish_mutation(&app, mutated.reloaded).await?;
    Ok(mutated.value)
}
