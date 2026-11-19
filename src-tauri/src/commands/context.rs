use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

use crate::models::context::ContextItem;
use crate::services::clipboard::{read_clipboard_item, ClipboardError, ClipboardService, PastedItem};
use crate::services::context::ContextManagerService;
use crate::services::speech::widget::RecordingWidget;

static CONTEXT_VERSION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Clone)]
struct ContextChangedEvent {
    version: u64,
}

pub fn current_context_version() -> u64 {
    CONTEXT_VERSION.load(Ordering::Relaxed)
}

pub(crate) fn emit_context_changed(app: &AppHandle) -> crate::Result<()> {
    let version = CONTEXT_VERSION.fetch_add(1, Ordering::Relaxed) + 1;
    app.emit("context-changed", ContextChangedEvent { version })?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<RecordingWidget>().refresh_model(&app).await;
    });
    Ok(())
}

fn remove_item_at_version(
    context: &mut ContextManagerService,
    index: usize,
    version: u64,
    current_version: u64,
) -> bool {
    version == current_version && context.remove_item(index)
}

pub async fn remove_item_for_pill(app: &AppHandle, index: usize, version: u64) -> crate::Result<()> {
    let removed = {
        let context = app.state::<Arc<Mutex<ContextManagerService>>>();
        let mut context = context.lock().await;
        remove_item_at_version(&mut context, index, version, current_context_version())
    };
    if removed {
        emit_context_changed(app)?;
    } else {
        log::debug!("pill item removal ignored: index={index} version={version} (stale or out of range)");
    }
    Ok(())
}

/// Returns whether the context changed; the caller emits `context-changed`.
pub(crate) async fn write_clipboard_to_context(
    app: &AppHandle,
    write: fn(&mut ContextManagerService, PastedItem),
) -> crate::Result<bool> {
    let clipboard = app.state::<ClipboardService>();
    let Some(item) = read_clipboard_item(&clipboard).await? else {
        log::debug!("clipboard holds files: context left unchanged");
        return Ok(false);
    };
    let context = app.state::<Arc<Mutex<ContextManagerService>>>();
    write(&mut *context.lock().await, item);
    Ok(true)
}

async fn write_clipboard_and_emit(
    app: &AppHandle,
    write: fn(&mut ContextManagerService, PastedItem),
) -> crate::Result<()> {
    if write_clipboard_to_context(app, write).await? {
        emit_context_changed(app)?;
    }
    Ok(())
}

pub async fn paste_item_for_pill(app: &AppHandle) -> crate::Result<()> {
    match write_clipboard_and_emit(app, ContextManagerService::append_item).await {
        Err(crate::Error::Clipboard(ClipboardError::Unavailable(_))) => {
            log::debug!("pill paste ignored: nothing pasteable on the clipboard");
            Ok(())
        }
        Err(crate::Error::Clipboard(e)) => {
            log::warn!("pill paste failed reading the clipboard: {e}");
            Ok(())
        }
        result => result,
    }
}

#[tauri::command]
pub async fn get_context_items(
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
) -> crate::Result<Vec<ContextItem>> {
    Ok(context.lock().await.get_items())
}

#[tauri::command]
pub async fn get_context_text(
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
) -> crate::Result<Option<String>> {
    Ok(context.lock().await.get_context())
}

#[tauri::command]
pub async fn get_labelled_context(
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
) -> crate::Result<Option<String>> {
    Ok(context.lock().await.labelled_context())
}

#[tauri::command]
pub async fn has_context(
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
) -> crate::Result<bool> {
    Ok(context.lock().await.has_context())
}

#[tauri::command]
pub async fn append_context(
    app: AppHandle,
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
    value: String,
) -> crate::Result<()> {
    context.lock().await.append_context(value);
    emit_context_changed(&app)?;
    Ok(())
}

#[tauri::command]
pub async fn clear_context(
    app: AppHandle,
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
) -> crate::Result<()> {
    context.lock().await.clear();
    emit_context_changed(&app)?;
    Ok(())
}

#[tauri::command]
pub async fn remove_context_item(
    app: AppHandle,
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
    index: usize,
) -> crate::Result<bool> {
    let removed = context.lock().await.remove_item(index);
    if removed {
        emit_context_changed(&app)?;
    }
    Ok(removed)
}

#[tauri::command]
pub async fn append_context_image(
    app: AppHandle,
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
    data: String,
    media_type: String,
) -> crate::Result<()> {
    context
        .lock()
        .await
        .append_context_image(data, media_type);
    emit_context_changed(&app)?;
    Ok(())
}

#[tauri::command]
pub async fn append_context_from_clipboard(app: AppHandle) -> crate::Result<()> {
    write_clipboard_and_emit(&app, ContextManagerService::append_item).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_with_two_texts() -> ContextManagerService {
        let mut service = ContextManagerService::new();
        service.append_context("a".into());
        service.append_context("b".into());
        service
    }

    #[test]
    fn removal_with_a_stale_version_removes_nothing() {
        let mut service = service_with_two_texts();
        assert!(!remove_item_at_version(&mut service, 0, 3, 4));
        assert_eq!(service.item_count(), 2);
    }

    #[test]
    fn removal_with_the_current_version_removes_the_item() {
        let mut service = service_with_two_texts();
        assert!(remove_item_at_version(&mut service, 0, 4, 4));
        assert_eq!(service.get_context().as_deref(), Some("b"));
    }
}
