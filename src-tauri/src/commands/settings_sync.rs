use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::services::settings_sync::{
    connect_app, run_sync, ConnectRequest, PrepareSource, Prepared, SettingsSync, SyncResult,
    SyncStatus, Trigger,
};
use crate::Error;

const CLONE_DIR_NAME: &str = "settings-repo";

fn clone_dir(app: &AppHandle) -> crate::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join(CLONE_DIR_NAME))
}

#[tauri::command]
pub async fn get_settings_sync_status(
    engine: State<'_, Arc<SettingsSync>>,
) -> crate::Result<SyncStatus> {
    Ok(engine.status())
}

#[tauri::command]
pub async fn sync_settings_now(
    app: AppHandle,
    engine: State<'_, Arc<SettingsSync>>,
) -> crate::Result<SyncStatus> {
    match run_sync(&app, Trigger::Manual).await {
        SyncResult::AlreadySyncing => Err(Error::Other("already syncing".to_string())),
        SyncResult::Paused | SyncResult::Completed(_) => Ok(engine.status()),
    }
}

#[tauri::command]
pub async fn sync_clone_dir(app: AppHandle) -> crate::Result<String> {
    Ok(clone_dir(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn sync_prepare(
    app: AppHandle,
    engine: State<'_, Arc<SettingsSync>>,
    source: PrepareSource,
) -> crate::Result<Prepared> {
    let clone_dir = clone_dir(&app)?;
    Ok(engine.prepare_source(&source, &clone_dir).await?)
}

#[tauri::command]
pub async fn sync_connect(
    app: AppHandle,
    engine: State<'_, Arc<SettingsSync>>,
    request: ConnectRequest,
) -> crate::Result<SyncStatus> {
    connect_app(&app, &request).await?;
    match run_sync(&app, Trigger::Manual).await {
        SyncResult::AlreadySyncing => Err(Error::Other("already syncing".to_string())),
        SyncResult::Paused | SyncResult::Completed(_) => Ok(engine.status()),
    }
}
