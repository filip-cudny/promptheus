use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State, Webview};
use tokio::sync::Mutex;

use crate::commands::settings::apply_settings_reload;
use crate::models::speech::AudioClipInfo;
use crate::services::config::{ConfigService, KeytermsDoc};
use crate::models::speech::DictationBusy;
use crate::services::speech::capture::{self, Owner, WindowOwner};
use crate::services::speech::composer::{self, AppEffects};
use crate::services::speech::{session, AudioClipStore, ComposerOwner, SpeechError, SpeechService};
use crate::services::sqlite_history::SqliteHistoryService;
use crate::Error;

#[tauri::command]
pub async fn toggle_speech_recording(
    app: AppHandle,
    action_id: Option<String>,
) -> crate::Result<()> {
    session::toggle_speech_recording(app, action_id).await
}

#[tauri::command]
pub async fn retry_transcription(app: AppHandle, entry_id: String) -> crate::Result<()> {
    session::retry_transcription(app, entry_id).await
}

#[tauri::command]
pub async fn pause_speech_recording(app: AppHandle) -> crate::Result<()> {
    session::pause_speech_recording(app).await
}

#[tauri::command]
pub async fn resume_speech_recording(app: AppHandle) -> crate::Result<()> {
    session::resume_speech_recording(app).await
}

#[tauri::command]
pub async fn cancel_speech_recording(app: AppHandle) -> crate::Result<()> {
    session::cancel_speech_recording(app).await
}

#[tauri::command]
pub async fn select_recording_widget_target(app: AppHandle, id: String) -> crate::Result<()> {
    session::select_recording_widget_target(app, id).await
}

#[tauri::command]
pub async fn set_recording_widget_dropdown(
    app: AppHandle,
    open: bool,
    rows: u32,
    separator: bool,
) -> crate::Result<&'static str> {
    session::set_recording_widget_dropdown(app, open, rows, separator).await
}

#[tauri::command]
pub async fn set_recording_widget_preview(
    app: AppHandle,
    open: bool,
    index: Option<u32>,
    text: Option<String>,
) -> crate::Result<&'static str> {
    session::set_recording_widget_preview(app, open, index, text).await
}

#[tauri::command]
pub async fn set_recording_widget_tooltip(
    app: AppHandle,
    open: bool,
    text: Option<String>,
    center_x: Option<f64>,
) -> crate::Result<&'static str> {
    session::set_recording_widget_tooltip(app, open, text, center_x).await
}

#[tauri::command]
pub async fn recording_widget_overlay_rendered(app: AppHandle, seq: u64) -> crate::Result<()> {
    session::recording_widget_overlay_rendered(app, seq).await
}

#[tauri::command]
pub async fn remove_recording_widget_item(
    app: AppHandle,
    index: u32,
    context_version: u64,
) -> crate::Result<()> {
    session::remove_recording_widget_item(app, index, context_version).await
}

#[tauri::command]
pub async fn stop_chat_speech_recording(app: AppHandle) -> crate::Result<()> {
    session::stop_chat_speech_recording(app).await
}

#[tauri::command]
pub async fn paste_into_recording_widget(app: AppHandle) -> crate::Result<()> {
    session::paste_into_recording_widget(app).await
}

#[tauri::command]
pub async fn capture_screenshot_into_recording_widget(app: AppHandle) -> crate::Result<()> {
    session::capture_screenshot_into_recording_widget(app).await
}

#[tauri::command]
pub async fn capture_selection_into_recording_widget(app: AppHandle) -> crate::Result<()> {
    session::capture_selection_into_recording_widget(app).await
}

#[tauri::command]
pub fn cancel_recording_widget_selection() {
    session::cancel_recording_widget_selection();
}

fn window_owner(owner: &str) -> crate::Result<WindowOwner> {
    WindowOwner::from_label(owner).ok_or_else(|| Error::Other(format!("unknown capture owner: {owner}")))
}

#[tauri::command]
pub fn capture_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        crate::services::gnome_shell::is_gnome_wayland()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

#[tauri::command]
pub async fn capture_screenshot_for_window(app: AppHandle, owner: String) -> crate::Result<Option<String>> {
    let owner = Owner::Window(window_owner(&owner)?);
    Ok(capture::screenshot(&app, &owner).await?.map(capture::png_base64))
}

#[tauri::command]
pub async fn capture_selection_for_window(app: AppHandle, owner: String) -> crate::Result<Option<String>> {
    let owner = Owner::Window(window_owner(&owner)?);
    Ok(capture::selection(&app, &owner).await)
}

#[tauri::command]
pub async fn cancel_window_capture(owner: String) -> crate::Result<()> {
    capture::cancel_owned_by(window_owner(&owner)?.label());
    Ok(())
}

fn composer_owner(webview: &Webview, tab_id: Option<String>) -> crate::Result<ComposerOwner> {
    ComposerOwner::new(webview.label(), tab_id)
        .ok_or_else(|| Error::Other("Dictation is not available in this window".into()))
}

#[tauri::command]
pub async fn get_dictation_busy(
    speech: State<'_, Arc<Mutex<SpeechService>>>,
) -> crate::Result<Option<DictationBusy>> {
    Ok(speech.lock().await.busy())
}

#[tauri::command]
pub async fn start_composer_dictation(
    app: AppHandle,
    webview: Webview,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
    tab_id: Option<String>,
) -> crate::Result<()> {
    let owner = composer_owner(&webview, tab_id)?;
    composer::start(&AppEffects(app), &speech, owner).await?;
    Ok(())
}

#[tauri::command]
pub async fn finish_composer_dictation(
    app: AppHandle,
    webview: Webview,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
) -> crate::Result<()> {
    let effects = AppEffects(app);
    if let Some(job) = composer::finish(&effects, &speech, webview.label()).await? {
        let speech = Arc::clone(&*speech);
        tauri::async_runtime::spawn(async move { composer::run(&effects, &speech, job).await });
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_composer_dictation(
    app: AppHandle,
    webview: Webview,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
) -> crate::Result<()> {
    composer::cancel(&AppEffects(app), &speech, webview.label()).await;
    Ok(())
}

#[tauri::command]
pub async fn retry_composer_dictation(
    app: AppHandle,
    webview: Webview,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
) -> crate::Result<()> {
    let effects = AppEffects(app);
    let job = composer::retry(&effects, &speech, webview.label()).await?;
    let speech = Arc::clone(&*speech);
    tauri::async_runtime::spawn(async move { composer::run(&effects, &speech, job).await });
    Ok(())
}

#[tauri::command]
pub async fn dismiss_composer_dictation(
    webview: Webview,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
) -> crate::Result<()> {
    composer::dismiss(&speech, webview.label()).await;
    Ok(())
}

#[derive(Serialize)]
pub struct RecordingState {
    is_recording: bool,
    is_transcribing: bool,
    action_id: Option<String>,
}

#[tauri::command]
pub async fn get_audio_clip_info(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    clips: State<'_, Arc<AudioClipStore>>,
    speech: State<'_, Arc<Mutex<SpeechService>>>,
    entry_id: String,
) -> crate::Result<AudioClipInfo> {
    let clip = {
        let history = history.lock().await;
        clips.find_by_entry(history.conn(), &entry_id)
    };

    let is_retrying = speech.lock().await.is_retrying(&entry_id);

    Ok(match clip {
        Some(clip) if clip.path.exists() => AudioClipInfo {
            has_audio: true,
            expires_at: Some(clip.expires_at),
            duration_secs: Some(clip.duration_secs),
            bytes: Some(clip.bytes),
            is_retrying,
        },
        _ => AudioClipInfo {
            is_retrying,
            ..AudioClipInfo::absent()
        },
    })
}

#[tauri::command]
pub async fn export_audio_clip(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    clips: State<'_, Arc<AudioClipStore>>,
    entry_id: String,
    destination: String,
) -> crate::Result<()> {
    let clip = {
        let history = history.lock().await;
        clips.find_by_entry(history.conn(), &entry_id)
    };

    let Some(clip) = clip else {
        return Err(Error::Speech(SpeechError::AudioUnavailable));
    };

    let clips = Arc::clone(&*clips);
    tokio::task::spawn_blocking(move || clips.copy_to(&clip, &PathBuf::from(destination)))
        .await
        .map_err(|e| Error::Other(e.to_string()))?
        .map_err(SpeechError::from)?;

    log::info!("audio clip exported for entry {entry_id}");
    Ok(())
}

#[tauri::command]
pub async fn discard_audio_clip(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    clips: State<'_, Arc<AudioClipStore>>,
    entry_id: String,
) -> crate::Result<()> {
    let history = history.lock().await;
    clips
        .delete_by_entry(history.conn(), &entry_id)
        .map_err(SpeechError::from)?;
    log::debug!("audio clip discarded for entry {entry_id}");
    Ok(())
}

#[tauri::command]
pub async fn get_recording_state(
    speech: State<'_, Arc<Mutex<SpeechService>>>,
) -> crate::Result<RecordingState> {
    let s = speech.lock().await;
    Ok(RecordingState {
        is_recording: s.is_recording(),
        is_transcribing: s.is_transcribing(),
        action_id: s.recording_action_id().map(String::from),
    })
}

#[tauri::command]
pub async fn get_stt_keyterms(
    config: State<'_, Arc<Mutex<ConfigService>>>,
) -> crate::Result<KeytermsDoc> {
    let svc = config.lock().await;
    Ok(svc.read_stt_keyterms_file()?)
}

#[tauri::command]
pub async fn save_stt_keyterms(
    app: AppHandle,
    config: State<'_, Arc<Mutex<ConfigService>>>,
    content: String,
) -> crate::Result<KeytermsDoc> {
    let mutated = config.lock().await.mutate(|c| {
        let path_was_unset = c.settings().surfaces.speech_to_text.keyterms_file.is_none();
        c.write_stt_keyterms_file(&content)
            .map(|doc| (doc, path_was_unset))
    })?;
    if mutated.reloaded {
        apply_settings_reload(&app, "reload").await?;
    }
    let (doc, path_was_unset) = mutated.value?;
    if path_was_unset {
        let _ = app.emit("settings-changed", serde_json::json!({}));
    }
    let _ = app.emit("stt-keyterms-changed", ());
    Ok(doc)
}
