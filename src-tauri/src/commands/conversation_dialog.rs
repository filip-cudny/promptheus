use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::models::history::InvocationSource;
use crate::services::ai_webview::{self, SurfaceTarget};
use crate::services::dialog::DialogConfig;
use crate::Error;

#[derive(Serialize, Clone)]
pub struct NewConversationEvent {
    pub reason: String,
}

#[derive(Serialize, Clone)]
struct RestoreHistoryEvent {
    entry_id: String,
}

#[derive(Serialize, Clone)]
struct VoiceInputEvent {
    text: String,
    skill_id: String,
    skill_model: Option<String>,
    auto_send: bool,
    invocation_source: Option<InvocationSource>,
    copy_token: Option<String>,
}

#[derive(Serialize, Clone)]
struct OpenForSkillEvent {
    skill_id: String,
    skill_name: String,
    skill_model: Option<String>,
}

static PENDING: Mutex<Option<DialogInitParams>> = Mutex::new(None);

#[derive(Serialize)]
pub struct DialogInitParams {
    skill_id: String,
    skill_name: String,
    skill_model: Option<String>,
    history_entry_id: Option<String>,
    initial_input: Option<String>,
    auto_send_input: bool,
    new_chat: bool,
    invocation_source: Option<InvocationSource>,
    copy_token: Option<String>,
}

pub fn conversation_dialog_config() -> DialogConfig {
    DialogConfig {
        label: "conversation-dialog".into(),
        url: "conversation-dialog.html".into(),
        title: "Promptheus — chat".into(),
        default_width: 700.0,
        default_height: 600.0,
        geometry_key: "conversation-dialog".into(),
        added_width: 0.0,
    }
}

#[tauri::command]
pub async fn open_conversation_dialog(
    app: tauri::AppHandle,
    skill_id: String,
    skill_name: String,
    skill_model: Option<String>,
    history_entry_id: Option<String>,
    initial_input: Option<String>,
    auto_send_input: Option<bool>,
    new_chat: Option<bool>,
    invocation_source: Option<InvocationSource>,
    copy_token: Option<String>,
) -> crate::Result<()> {
    let label = "conversation-dialog";
    let auto_send_input = auto_send_input.unwrap_or(false);
    let new_chat = new_chat.unwrap_or(false);

    if app.get_window(label).is_some() {
        emit_reuse_event(
            &app,
            label,
            history_entry_id,
            initial_input,
            auto_send_input,
            &skill_id,
            &skill_name,
            skill_model.as_deref(),
            new_chat,
            invocation_source,
            copy_token,
        )?;
        ai_webview::surface(&app, "open_conversation_dialog", SurfaceTarget::NativeView)
            .await?;
        return Ok(());
    }

    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(DialogInitParams {
        skill_id: skill_id.clone(),
        skill_name: skill_name.clone(),
        skill_model: skill_model.clone(),
        history_entry_id: history_entry_id.clone(),
        initial_input: initial_input.clone(),
        auto_send_input,
        new_chat,
        invocation_source,
        copy_token,
    });

    Ok(ai_webview::surface(&app, "open_conversation_dialog", SurfaceTarget::NativeView).await?)
}

#[tauri::command]
pub async fn focus_or_open_chat(app: tauri::AppHandle) -> crate::Result<()> {
    Ok(ai_webview::surface(&app, "focus_or_open_chat", SurfaceTarget::KeepActive).await?)
}

#[tauri::command]
pub fn get_dialog_init_params() -> Option<DialogInitParams> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[tauri::command]
pub fn get_chat_corners() -> bool {
    crate::services::chat_corners::is_rounded()
}

fn emit_reuse_event(
    app: &tauri::AppHandle,
    label: &str,
    history_entry_id: Option<String>,
    initial_input: Option<String>,
    auto_send_input: bool,
    skill_id: &str,
    skill_name: &str,
    skill_model: Option<&str>,
    new_chat: bool,
    invocation_source: Option<InvocationSource>,
    copy_token: Option<String>,
) -> crate::Result<()> {
    if app.get_webview(label).is_none() {
        return Err(Error::Other(format!(
            "conversation dialog window '{label}' missing"
        )));
    }

    if new_chat {
        app.emit_to(
            label,
            "new-conversation",
            NewConversationEvent {
                reason: "reopen".into(),
            },
        )?;
    } else if let Some(entry_id) = history_entry_id {
        app.emit_to(
            label,
            "restore-history",
            RestoreHistoryEvent { entry_id },
        )?;
    } else if let Some(text) = initial_input {
        app.emit_to(
            label,
            "voice-input",
            VoiceInputEvent {
                text,
                skill_id: skill_id.to_string(),
                skill_model: skill_model.map(|s| s.to_string()),
                auto_send: auto_send_input,
                invocation_source,
                copy_token,
            },
        )?;
    } else if !skill_id.is_empty() {
        app.emit_to(
            label,
            "open-for-skill",
            OpenForSkillEvent {
                skill_id: skill_id.to_string(),
                skill_name: skill_name.to_string(),
                skill_model: skill_model.map(|s| s.to_string()),
            },
        )?;
    }
    Ok(())
}
