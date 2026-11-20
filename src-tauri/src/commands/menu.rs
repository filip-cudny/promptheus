use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::models::history::HistoryEntryType;
use crate::models::menu::{MenuItem, MenuItemType};
use crate::models::skill::SkillSummary;
use crate::services::config::ConfigService;
use crate::services::context::ContextManagerService;
use crate::services::execution::PromptExecutionService;
use crate::services::menu_coordinator::MenuCoordinator;
use crate::services::dialog::window;
use crate::services::monitor::{clamp_origin, cursor_work_area, CursorWorkArea};
use crate::services::skill::{strip_skill_prefix, SkillService};
use crate::services::speech::{AudioClipStore, PendingOrigin, SpeechError, SpeechService};
use crate::services::sqlite_history::SqliteHistoryService;

pub const CONTEXT_MENU_TITLE: &str = "Promptheus Context Menu";

#[derive(Serialize, Clone)]
struct ShowMenuPayload {
    #[serde(flatten)]
    area: CursorWorkArea,
    shell_placement: bool,
}

#[cfg(target_os = "linux")]
fn payload_from_pointer(pointer: (i32, i32, i32, i32, i32, i32)) -> ShowMenuPayload {
    let (cursor_x, cursor_y, work_x, work_y, work_width, work_height) = pointer;
    ShowMenuPayload {
        area: CursorWorkArea {
            cursor_x: cursor_x as f64,
            cursor_y: cursor_y as f64,
            work_x: work_x as f64,
            work_y: work_y as f64,
            work_width: work_width as f64,
            work_height: work_height as f64,
        },
        shell_placement: true,
    }
}

#[cfg(target_os = "linux")]
async fn shell_pointer_payload() -> Option<ShowMenuPayload> {
    let t = std::time::Instant::now();
    let result = async {
        crate::services::gnome_shell::proxy()
            .await?
            .get_pointer()
            .await
    }
    .await;
    match result {
        Ok(pointer) => {
            log::debug!(
                target: "app_lib::commands::menu",
                "show_context_menu_window: GetPointer OK in {:?}",
                t.elapsed(),
            );
            Some(payload_from_pointer(pointer))
        }
        Err(e) => {
            log::warn!(
                target: "app_lib::commands::menu",
                "show_context_menu_window: GetPointer failed, falling back to cursor_position: {e}",
            );
            None
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

/// Chip payloads carry previews only. The clipboard value is resolved on the
/// Rust side by `copy_last_interaction`, so a full response never crosses IPC
/// just to render the menu.
#[tauri::command]
pub async fn get_context_menu_items(
    config: State<'_, Arc<Mutex<ConfigService>>>,
    context: State<'_, Arc<Mutex<ContextManagerService>>>,
    menu_coordinator: State<'_, Arc<Mutex<MenuCoordinator>>>,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
    prompt_execution: State<'_, Arc<Mutex<PromptExecutionService>>>,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    skill_service: State<'_, Arc<Mutex<SkillService>>>,
    clips: State<'_, Arc<AudioClipStore>>,
) -> crate::Result<Vec<MenuItem>> {
    let context_items = context.lock().await.get_items();
    let mut menu_coordinator = menu_coordinator.lock().await;
    menu_coordinator.state.context_items = context_items;

    let (is_recording, retrying_entries) = {
        let s = speech.lock().await;
        (s.is_recording(), s.retrying_entries())
    };
    let is_executing = prompt_execution.lock().await.is_busy();
    menu_coordinator.state.is_recording = is_recording;
    menu_coordinator.state.is_action_executing = is_executing;

    let (last_text, last_speech, last_speech_clip) = {
        let history = history.lock().await;
        let last_text = history.get_last_quick_action(HistoryEntryType::Text);
        let last_speech = history.get_last_quick_action(HistoryEntryType::Speech);
        let clip = last_speech
            .as_ref()
            .and_then(|e| clips.find_by_entry(history.conn(), &e.id))
            .filter(|clip| clip.path.exists());
        (last_text, last_speech, clip)
    };

    let transcription_payload = last_speech.as_ref().map(|entry| {
        let status = if !entry.success {
            if retrying_entries.contains(&entry.id) {
                "retrying"
            } else {
                "failed"
            }
        } else {
            "ok"
        };
        serde_json::json!({
            "status": status,
            "entry_id": entry.id,
            "preview": entry.output_preview.as_deref().map(|c| truncate(c, 200)),
            "error": entry.error,
            "cancelled": entry.error.as_deref() == Some(SpeechError::Cancelled.to_string().as_str()),
            "has_audio": last_speech_clip.is_some(),
            "expires_at": last_speech_clip.as_ref().map(|c| c.expires_at.clone()),
        })
    });

    let config = config.lock().await;
    let skill_service = skill_service.lock().await;

    let skill_summaries: Vec<SkillSummary> = skill_service
        .list_skills()
        .iter()
        .map(SkillSummary::from)
        .collect();
    log::debug!("context menu: refreshing {} skills", skill_summaries.len());
    menu_coordinator.state.skills = skill_summaries;

    let mut items = menu_coordinator.get_menu_items(&config);

    for item in &mut items {
        if item.item_type == MenuItemType::LastInteraction {
            item.data = Some(serde_json::json!({
                "input": last_text.as_ref().map(|e| {
                    let raw_input = strip_skill_prefix(&e.input_preview, &skill_service);
                    serde_json::json!({ "preview": truncate(raw_input, 200) })
                }),
                "output": last_text.as_ref().and_then(|e| {
                    e.output_preview.as_ref().map(|c| serde_json::json!({ "preview": truncate(c, 200) }))
                }),
                "transcription": transcription_payload.clone(),
            }));
        }
    }

    Ok(items)
}

#[tauri::command]
pub async fn execute_menu_item(
    app: AppHandle,
    skill_service: State<'_, Arc<Mutex<SkillService>>>,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
    item_id: String,
    shift_pressed: bool,
) -> crate::Result<()> {
    log::debug!("execute_menu_item: id={item_id}, shift={shift_pressed}");

    if item_id == "system_speech_to_text" {
        return super::speech::toggle_speech_recording(app, None).await;
    }

    if shift_pressed {
        let display_name = skill_service
            .lock()
            .await
            .get_skill(&item_id)
            .map(|sk| sk.display_name.clone());

        if let Some(display_name) = display_name {
            let mut s = speech.lock().await;
            if s.is_recording() {
                drop(s);
                return super::speech::toggle_speech_recording(app, None).await;
            }
            s.set_pending_prompt(item_id.clone(), display_name, PendingOrigin::ShiftClick);
            drop(s);
            return super::speech::toggle_speech_recording(app, Some(item_id)).await;
        }
    }

    log::warn!("unhandled menu item: {item_id}");
    Ok(())
}

#[tauri::command]
pub async fn show_context_menu_window(app: tauri::AppHandle) -> crate::Result<()> {
    log::debug!(target: "app_lib::commands::menu", "show_context_menu_window: ENTER");
    let win = window(&app, "context-menu")?;

    #[cfg(target_os = "linux")]
    if crate::services::gnome_shell::is_gnome_wayland() {
        if let Some(payload) = shell_pointer_payload().await {
            app.emit_to("context-menu", "show-context-menu", payload)?;
            return Ok(());
        }
    }

    let t0 = std::time::Instant::now();
    let area = cursor_work_area(&win)?;
    log::debug!(
        target: "app_lib::commands::menu",
        "show_context_menu_window: cursor_work_area OK in {:?}",
        t0.elapsed(),
    );

    let payload = ShowMenuPayload {
        area,
        shell_placement: false,
    };

    log::debug!(
        "show context menu: cursor=({}, {}), work_area=({}, {}, {}x{})",
        payload.area.cursor_x, payload.area.cursor_y,
        payload.area.work_x, payload.area.work_y, payload.area.work_width, payload.area.work_height,
    );

    let t2 = std::time::Instant::now();
    app.emit_to("context-menu", "show-context-menu", payload)?;
    log::debug!(
        target: "app_lib::commands::menu",
        "show_context_menu_window: emit_to OK in {:?}",
        t2.elapsed(),
    );

    Ok(())
}

#[tauri::command]
pub async fn show_context_menu_panel(app: tauri::AppHandle) -> crate::Result<()> {
    let win = window(&app, "context-menu")?;

    #[cfg(target_os = "macos")]
    {
        crate::services::macos_panel::on_main(
            &win,
            crate::services::macos_panel::show_panel_without_activating,
        )
        .await
    }

    #[cfg(not(target_os = "macos"))]
    {
        win.show()?;
        Ok(())
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn clamp_menu_origin(
    anchor_x: f64,
    anchor_y: f64,
    width: f64,
    height: f64,
    work_x: f64,
    work_y: f64,
    work_width: f64,
    work_height: f64,
) -> (f64, f64) {
    clamp_origin(
        (anchor_x, anchor_y),
        (width, height),
        (work_x, work_y, work_width, work_height),
    )
}

#[tauri::command]
pub async fn place_context_menu(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    first: bool,
) -> crate::Result<()> {
    #[cfg(target_os = "linux")]
    if crate::services::gnome_shell::is_gnome_wayland() {
        return place_context_menu_through_shell(&app, x.round() as i32, y.round() as i32, first)
            .await;
    }
    let _ = (app, x, y, first);
    Ok(())
}

#[tauri::command]
pub async fn set_context_menu_input_region(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> crate::Result<()> {
    #[cfg(target_os = "linux")]
    if crate::services::gnome_shell::is_gnome_wayland() {
        let win = window(&app, "context-menu")?;
        crate::services::menu_input_region::set(&win.as_ref().window(), (x, y, width, height));
        return Ok(());
    }
    let _ = (app, x, y, width, height);
    Ok(())
}

#[cfg(target_os = "linux")]
async fn place_context_menu_through_shell(
    app: &tauri::AppHandle,
    x: i32,
    y: i32,
    first: bool,
) -> crate::Result<()> {
    let win = window(app, "context-menu")?;
    let show = first.then_some(&win);
    crate::services::gnome_shell::place_window(CONTEXT_MENU_TITLE, x, y, first, show).await?;
    Ok(())
}

#[tauri::command]
pub async fn hide_context_menu_panel(
    app: tauri::AppHandle,
    restore_focus: bool,
) -> crate::Result<()> {
    let win = window(&app, "context-menu")?;

    #[cfg(target_os = "macos")]
    {
        crate::services::macos_panel::on_main(&win, move |win| {
            crate::services::macos_panel::hide_panel(win, restore_focus)
        })
        .await
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = restore_focus;
        win.hide()?;
        Ok(())
    }
}

#[tauri::command]
pub async fn focus_context_menu(app: tauri::AppHandle) -> crate::Result<()> {
    log::debug!(target: "app_lib::commands::menu", "focus_context_menu: ENTER");
    let win = window(&app, "context-menu")?;

    #[cfg(target_os = "macos")]
    {
        crate::services::macos_panel::on_main(
            &win,
            crate::services::macos_panel::make_key_without_activating,
        )
        .await
    }

    #[cfg(not(target_os = "macos"))]
    {
        win.set_focus()?;
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn pointer_tuple_maps_to_shell_placement_payload() {
        let payload = payload_from_pointer((100, 200, 0, 32, 1920, 1048));
        let area = payload.area;
        assert_eq!((area.cursor_x, area.cursor_y), (100.0, 200.0));
        assert_eq!(
            (area.work_x, area.work_y, area.work_width, area.work_height),
            (0.0, 32.0, 1920.0, 1048.0),
        );
        assert!(payload.shell_placement);
    }
}
