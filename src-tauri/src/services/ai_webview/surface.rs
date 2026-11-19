use std::time::Instant;

use tauri::Manager;

use crate::commands::conversation_dialog::conversation_dialog_config;
use crate::models::settings::WebviewProvider;
use crate::services::dialog::{self, focus_host_window, CONVERSATION_DIALOG_LABEL};
use crate::services::dock::DockManager;

use super::provider_swap::{activate_tab, show_native_view};
use super::unload::unload_tab;
use super::AiWebviewState;

pub enum SurfaceTarget {
    KeepActive,
    NativeView,
    NewProviderTab(WebviewProvider),
}

impl SurfaceTarget {
    fn name(&self) -> String {
        match self {
            Self::KeepActive => "keep-active".into(),
            Self::NativeView => "native-view".into(),
            Self::NewProviderTab(provider) => format!("new-provider-tab:{}", provider.id),
        }
    }
}

pub async fn surface(
    app: &tauri::AppHandle,
    entry: &str,
    target: SurfaceTarget,
) -> Result<(), String> {
    log::debug!(
        target: "app_lib::services::ai_webview",
        "surface: entry={entry} target={}",
        target.name(),
    );
    let result = surface_inner(app, target).await;
    if let Err(reason) = &result {
        log::warn!(
            target: "app_lib::services::ai_webview",
            "surface failed: entry={entry}: {reason}",
        );
    }
    result
}

async fn surface_inner(app: &tauri::AppHandle, target: SurfaceTarget) -> Result<(), String> {
    match app.get_window(CONVERSATION_DIALOG_LABEL) {
        None => {
            dialog::open_or_focus(app, &conversation_dialog_config()).await?;
        }
        Some(host) => show_if_hidden(app, &host)?,
    }
    match target {
        SurfaceTarget::KeepActive => activate_kept(app)?,
        SurfaceTarget::NativeView => show_native_view(app)?,
        SurfaceTarget::NewProviderTab(provider) => {
            let state = app
                .try_state::<AiWebviewState>()
                .ok_or("ai webview state missing")?;
            let tab_id = state.with_tabs(|tabs| tabs.push_provider(&provider, Instant::now()));
            activate_tab(app, &tab_id)?;
        }
    }
    focus_host_window(app, CONVERSATION_DIALOG_LABEL)
}

fn show_if_hidden(app: &tauri::AppHandle, host: &tauri::Window) -> Result<(), String> {
    if host.is_visible().map_err(|e| e.to_string())? {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    app.show().map_err(|e| e.to_string())?;
    host.show().map_err(|e| e.to_string())?;
    app.state::<DockManager>().host_surfaced(app);
    Ok(())
}

fn activate_kept(app: &tauri::AppHandle) -> Result<(), String> {
    let active = app
        .try_state::<AiWebviewState>()
        .and_then(|state| state.with_tabs(|tabs| tabs.active_id().map(str::to_string)));
    match active {
        Some(tab_id) => activate_tab(app, &tab_id),
        None => show_native_view(app),
    }
}

pub fn hide_host(app: &tauri::AppHandle) {
    let Some(host) = app.get_window(CONVERSATION_DIALOG_LABEL) else {
        return;
    };
    #[cfg(target_os = "macos")]
    if host.is_fullscreen().unwrap_or(false) {
        if let Err(e) = host.set_fullscreen(false) {
            log::warn!(
                target: "app_lib::services::ai_webview",
                "leaving fullscreen before hide failed: {e}",
            );
        }
    }
    if let Err(e) = host.hide() {
        log::warn!(
            target: "app_lib::services::ai_webview",
            "hide chat window failed: {e}",
        );
        return;
    }
    if let Some(state) = app.try_state::<AiWebviewState>() {
        for tab_id in state.with_tabs(|tabs| tabs.live_provider_tabs()) {
            unload_tab(app, &tab_id);
        }
    }
    crate::services::speech::composer::cancel_owned_by(app, CONVERSATION_DIALOG_LABEL);
    crate::services::speech::capture::cancel_owned_by(CONVERSATION_DIALOG_LABEL);
    app.state::<DockManager>().host_hidden(app);
    log::info!(
        target: "app_lib::services::ai_webview",
        "chat window hidden",
    );
}
