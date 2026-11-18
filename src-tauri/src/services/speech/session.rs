use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Mutex};

use crate::commands::clipboard::store_pending_copy;
use crate::models::history::{HistoryEntryType, InvocationSource, OutputRoute};
use crate::models::settings::{ClaudeTarget, ModelConfig, StopChatTarget};
use crate::models::speech::{
    SpeechRecordingStartedEvent, SpeechRecordingStoppedEvent, SpeechTranscriptionError,
    SpeechTranscriptionRetry,
};
use crate::services::clipboard::{ClipboardFingerprint, ClipboardService, PastedItem};
use crate::services::context::ContextManagerService;
use crate::services::config::ConfigService;
use crate::services::execution::skill_executor::run_skill_stream;
use crate::services::notification::{NotificationLevel, NotificationService};
use crate::services::skill::SkillService;
use crate::services::speech::widget::{
    configured_claude_target, configured_providers, configured_stop_chat_target, pill_targets, RecordingWidget,
};
use crate::services::speech::capture::{self, Owner};
use crate::services::speech::{
    self, claude_desktop, composer, screenshot, selection, webview_provider_of, AudioClipStore, PendingOrigin, PendingPrompt, SpeechError,
    SpeechService, SttOptions,
};
use crate::services::sqlite_history::SqliteHistoryService;
use crate::Error;

pub(super) const MIN_RECORDING_SAMPLES_SECS: f64 = 1.0;

#[derive(Clone, Serialize)]
struct TranscriptionComplete {
    text: String,
    duration_secs: f64,
    entry_id: Option<String>,
}

#[derive(Clone, Serialize)]
struct AlternativeExecutePayload {
    skill_id: String,
    skill_name: String,
    text: String,
}


pub(super) struct SttRuntime {
    pub(super) model: ModelConfig,
    pub(super) options: SttOptions,
    pub(super) auto_retry_attempts: u32,
    audio_retention_hours: u32,
    keep_audio_on_success: bool,
}

pub(super) async fn load_stt_runtime(
    config_state: &Arc<Mutex<ConfigService>>,
) -> Result<SttRuntime, SpeechError> {
    let guard = config_state.lock().await;
    let stt_prompt = guard.stt_prompt();
    let keyterms = guard.stt_keyterms();
    let surface = guard.settings().surfaces.speech_to_text.clone();
    let model = guard
        .resolve_stt_model()
        .cloned()
        .ok_or(SpeechError::ApiKeyMissing)?;

    Ok(SttRuntime {
        model,
        options: SttOptions {
            language: surface.language,
            no_verbatim: surface.no_verbatim,
            prompt: stt_prompt,
            keyterms,
        },
        auto_retry_attempts: surface.auto_retry_attempts,
        audio_retention_hours: surface.audio_retention_hours,
        keep_audio_on_success: surface.keep_audio_on_success,
    })
}

fn format_clip_duration(duration_secs: f64) -> String {
    let total = duration_secs.round().max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn failure_input_content(duration_secs: f64) -> String {
    format!(
        "Voice note · {} — transcription failed",
        format_clip_duration(duration_secs)
    )
}

fn cancelled_input_content(duration_secs: f64) -> String {
    format!(
        "Voice note · {} — cancelled",
        format_clip_duration(duration_secs)
    )
}

fn add_failed_entry(
    history: &SqliteHistoryService,
    clips: &AudioClipStore,
    clip_id: Option<&str>,
    input: String,
    error: String,
) -> (Option<String>, bool) {
    let entry_id = history.add_entry(
        input,
        HistoryEntryType::Speech,
        None,
        None,
        false,
        Some(error),
        false,
        None,
        true,
        None,
    );

    if let (Some(clip_id), Some(entry_id)) = (clip_id, &entry_id) {
        if let Err(e) = clips.attach_to_entry(history.conn(), clip_id, entry_id) {
            log::warn!("failed to attach audio clip to history entry: {e}");
        }
    }
    let has_audio = clip_id.is_some() && entry_id.is_some();
    (entry_id, has_audio)
}

fn emit_retry(
    app: &AppHandle,
    entry_id: Option<String>,
    notice: speech::retry::RetryNotice,
    reason: String,
) {
    let _ = app.emit(
        "speech-transcription-retry",
        SpeechTranscriptionRetry {
            attempt: notice.attempt,
            max_attempts: notice.max_attempts,
            next_in_secs: notice.delay_secs,
            reason,
            entry_id,
        },
    );
}

async fn notify_retry_started(app: &AppHandle, notice: speech::retry::RetryNotice) {
    NotificationService::notify_now(
        app,
        "speech_transcription_retry",
        NotificationLevel::Warning,
        "Retrying transcription",
        Some(format!(
            "Attempt {} of {} failed — retrying in {}s",
            notice.attempt, notice.max_attempts, notice.delay_secs
        )),
    )
    .await;
}

#[derive(Debug, PartialEq)]
struct ChatRequest {
    skill_id: String,
    initial_input: String,
    skill_model: Option<String>,
    auto_send: bool,
    source: InvocationSource,
}

#[derive(Debug, PartialEq)]
enum TranscriptPlan {
    AlternativeExecute,
    NoSpeech,
    SkillRun { skill_id: String },
    Dictation,
    Chat(ChatRequest),
    Webview { provider_id: String },
    ClaudeDesktop,
}

fn transcript_plan(
    pending: Option<&PendingPrompt>,
    stop_chat: bool,
    text: &str,
    skill_model: Option<String>,
    claude_target: ClaudeTarget,
    stop_chat_target: StopChatTarget,
) -> TranscriptPlan {
    let chat = |skill_id: String, skill_model| {
        TranscriptPlan::Chat(ChatRequest {
            skill_id,
            initial_input: text.to_string(),
            skill_model,
            auto_send: true,
            source: InvocationSource::Pill,
        })
    };
    match pending {
        Some(p) if p.origin == PendingOrigin::ShiftClick => TranscriptPlan::AlternativeExecute,
        Some(_) if text.trim().is_empty() => TranscriptPlan::NoSpeech,
        _ if stop_chat && stop_chat_target == StopChatTarget::Claude => match claude_target {
            ClaudeTarget::Desktop => TranscriptPlan::ClaudeDesktop,
            ClaudeTarget::Webview => TranscriptPlan::Webview { provider_id: "claude".to_string() },
        },
        Some(p) if webview_provider_of(&p.id) == Some("claude") && claude_target == ClaudeTarget::Desktop => {
            TranscriptPlan::ClaudeDesktop
        }
        Some(p) if webview_provider_of(&p.id).is_some() => TranscriptPlan::Webview {
            provider_id: webview_provider_of(&p.id).unwrap_or_default().to_string(),
        },
        Some(p) if stop_chat => chat(p.id.clone(), skill_model),
        Some(p) => TranscriptPlan::SkillRun { skill_id: p.id.clone() },
        None if stop_chat => chat(String::new(), None),
        None => TranscriptPlan::Dictation,
    }
}

fn speech_output_route(plan: &TranscriptPlan) -> Option<String> {
    match plan {
        TranscriptPlan::Dictation => Some(OutputRoute::Clipboard.as_str().to_string()),
        TranscriptPlan::Chat(_) => Some(OutputRoute::Chat.as_str().to_string()),
        TranscriptPlan::ClaudeDesktop => Some("claude_desktop".to_string()),
        TranscriptPlan::Webview { provider_id } => Some(format!("webview:{provider_id}")),
        TranscriptPlan::AlternativeExecute
        | TranscriptPlan::NoSpeech
        | TranscriptPlan::SkillRun { .. } => None,
    }
}

async fn open_chat(app: &AppHandle, request: ChatRequest, copy_token: String) {
    log::debug!("stop chat: opening the chat, skill={}", request.skill_id);
    if let Err(e) = crate::commands::conversation_dialog::open_conversation_dialog(
        app.clone(),
        request.skill_id,
        String::new(),
        request.skill_model,
        None,
        Some(request.initial_input),
        Some(request.auto_send),
        None,
        Some(request.source),
        Some(copy_token),
    )
    .await
    {
        log::warn!("stop chat: opening the chat failed: {e}");
    }
}

async fn snapshot_clipboard(app: &AppHandle) -> String {
    let service = (*app.state::<ClipboardService>()).clone();
    let fingerprint = tokio::task::spawn_blocking(move || service.fingerprint())
        .await
        .unwrap_or(ClipboardFingerprint::Unreadable);
    store_pending_copy(fingerprint)
}

async fn record_speech_entry(
    app: &AppHandle,
    history: &Arc<Mutex<SqliteHistoryService>>,
    text: &str,
    output_route: Option<&str>,
) -> Option<String> {
    let added_id = history.lock().await.add_entry(
        text.to_string(),
        HistoryEntryType::Speech,
        Some(text.to_string()),
        None,
        true,
        None,
        false,
        None,
        true,
        output_route,
    );
    let _ = crate::services::history_events::emit_history_changed(app, added_id.clone(), None);
    added_id
}

async fn run_pill_skill(app: &AppHandle, skill_id: String, text: String, generation: u64) {
    let widget = app.state::<RecordingWidget>();
    log::debug!(
        "pill skill run: start skill={skill_id} route={} generation={generation}",
        OutputRoute::Clipboard.as_str()
    );
    let started = std::time::Instant::now();
    let result = run_skill_stream(
        app,
        skill_id.clone(),
        Some(text),
        InvocationSource::Pill,
        OutputRoute::Clipboard,
        false,
        Channel::new(|_| Ok(())),
    )
    .await;
    log::debug!(
        "pill skill run: end skill={skill_id} ok={} elapsed_ms={} generation={generation}",
        matches!(result, Ok(Some(_))),
        started.elapsed().as_millis()
    );

    match result {
        Ok(Some(_)) => widget.done_if_current(app, generation).await,
        Ok(None) => widget.close_if_current(app, generation).await,
        Err(e) => {
            log::warn!("pill skill run failed: {e}");
            widget.close_if_current(app, generation).await;
        }
    }
}

async fn notify_no_speech(app: &AppHandle) {
    NotificationService::notify_now(
        app,
        "speech_transcription_success",
        NotificationLevel::Error,
        "No speech detected",
        Some("No speech was detected in the recording"),
    )
    .await;
}

fn emit_empty_transcription(app: &AppHandle, duration_secs: f64) {
    let _ = app.emit(
        "speech-transcription-complete",
        TranscriptionComplete { text: String::new(), duration_secs, entry_id: None },
    );
}

async fn abort_transcription(app: &AppHandle) {
    app.state::<RecordingWidget>().close(app).await;
    let speech_state = app.state::<Arc<Mutex<SpeechService>>>();
    {
        let mut s = speech_state.lock().await;
        s.set_transcribing(false);
        s.clear_pending_prompt();
    }
    composer::emit_busy_changed(app).await;
}

async fn end_transcription(app: &AppHandle, speech: &Mutex<SpeechService>) {
    speech.lock().await.set_transcribing(false);
    composer::emit_busy_changed(app).await;
}

async fn warn_composer_running(app: &AppHandle, window: &str) {
    log::debug!("pill entry point refused: dictation is running in {window}");
    NotificationService::notify_now(
        app,
        "speech_recording_start",
        NotificationLevel::Warning,
        format!("Dictation is running in {window}"),
        None::<String>,
    )
    .await;
}

/// Warns and returns `true` while a composer owns the session, so the pill entry point stops.
async fn refused_during_composer(app: &AppHandle) -> bool {
    let window = app.state::<Arc<Mutex<SpeechService>>>().lock().await.composer_window();
    match window {
        Some(window) => {
            warn_composer_running(app, window).await;
            true
        }
        None => false,
    }
}

pub async fn toggle_speech_recording(
    app: AppHandle,
    action_id: Option<String>,
) -> crate::Result<()> {
    let speech_state = app.state::<Arc<Mutex<SpeechService>>>();
    let config_state = app.state::<Arc<Mutex<ConfigService>>>();
    let history_state = app.state::<Arc<Mutex<SqliteHistoryService>>>();
    let clip_state = app.state::<Arc<AudioClipStore>>();
    let widget = app.state::<RecordingWidget>();

    let was_recording;
    let raw_audio;
    let started_action_id;
    let started_session;

    {
        let mut s = speech_state.lock().await;

        if let Some(window) = s.composer_window() {
            warn_composer_running(&app, window).await;
            return Ok(());
        }

        if s.is_transcribing() {
            log::debug!("toggle_speech_recording: ignored, transcription in progress");
            NotificationService::notify_now(
                &app,
                "speech_recording_start",
                NotificationLevel::Warning,
                "Transcription in progress",
                Some("Wait until it finishes"),
            )
            .await;
            return Ok(());
        }

        was_recording = s.is_recording();

        if !was_recording {
            if s.is_debouncing() {
                log::debug!("toggle_speech_recording: ignored, debounced");
                return Ok(());
            }
            s.mark_toggle();
        }

        if was_recording {
            let audio = s.stop_recording_raw()?;
            s.set_transcribing(true);
            started_action_id = None;
            raw_audio = Some(audio);
            started_session = None;
        } else {
            raw_audio = None;
            s.start_recording(action_id.clone())?;
            started_action_id = action_id;
            started_session = Some(s.session());
        }
    }

    if !was_recording {
        composer::emit_busy_changed(&app).await;
        let _ = app.emit(
            "speech-recording-started",
            SpeechRecordingStartedEvent {
                action_id: started_action_id,
            },
        );
        let (reminder_settings, shortcut_hint) = {
            let guard = config_state.lock().await;
            let settings = guard.settings();
            (
                settings.notifications.recording_reminder.clone(),
                crate::services::hotkeys::shortcut_for_action(settings, "speech_to_text_toggle"),
            )
        };
        let widget_shown = match started_session {
            Some(session) => widget.show(&app, session).await,
            None => false,
        };
        if !widget_shown {
            NotificationService::notify_now(
                &app,
                "speech_recording_start",
                NotificationLevel::Info,
                "Recording started",
                Some("Click Speech to Text again to stop."),
            )
            .await;
        }
        if let Some(session) = started_session {
            speech::reminder::spawn(
                app.clone(),
                session,
                reminder_settings,
                shortcut_hint,
            );
        }
        return Ok(());
    }

    let (samples, sample_rate) = raw_audio.unwrap();
    widget.processing(&app).await;
    let generation = widget.generation().await;
    let had_audio = !samples.is_empty();
    let sample_count = samples.len();
    let duration_secs = sample_count as f64 / sample_rate.max(1) as f64;

    let _ = app.emit(
        "speech-recording-stopped",
        SpeechRecordingStoppedEvent { had_audio },
    );

    let min_samples = (sample_rate as f64 * MIN_RECORDING_SAMPLES_SECS) as usize;
    if sample_count < min_samples {
        log::debug!(
            "toggle_speech_recording: recording too short ({} samples, need {}), discarding",
            sample_count,
            min_samples,
        );
        emit_empty_transcription(&app, 0.0);
        abort_transcription(&app).await;
        return Ok(());
    }

    let wav_bytes = match tokio::task::spawn_blocking(move || speech::encode_wav(&samples, sample_rate))
        .await
        .map_err(|e| Error::Other(e.to_string()))?
    {
        Ok(bytes) => bytes,
        Err(e) => {
            abort_transcription(&app).await;
            return Err(e.into());
        }
    };

    let runtime = match load_stt_runtime(&config_state).await {
        Ok(runtime) => {
            if !widget.is_shown().await {
                NotificationService::notify_now(
                    &app,
                    "speech_recording_stop",
                    NotificationLevel::Info,
                    "Processing audio",
                    Some("Transcribing your speech to text"),
                )
                .await;
            }
            runtime
        }
        Err(e) => {
            abort_transcription(&app).await;
            return Err(Error::Speech(e));
        }
    };

    let clip_id = if runtime.audio_retention_hours > 0 {
        let history = history_state.lock().await;
        match clip_state.store(
            history.conn(),
            &wav_bytes,
            sample_rate,
            duration_secs,
            runtime.audio_retention_hours,
        ) {
            Ok(id) => Some(id),
            Err(e) => {
                log::warn!("failed to persist audio clip, retry will be unavailable: {e}");
                None
            }
        }
    } else {
        None
    };

    let app_clone = app.clone();
    let speech_inner = Arc::clone(&*speech_state);
    let history_inner = Arc::clone(&*history_state);
    let clip_inner = Arc::clone(&*clip_state);

    tokio::spawn(async move {
        let widget_inner = app_clone.state::<RecordingWidget>();
        let start = std::time::Instant::now();

        let (abort_tx, abort_rx) = watch::channel(false);
        let mut abort_tx = Some(abort_tx);
        let mut attempts_used = 1u32;

        let result = speech::transcribe_with_retry(
            wav_bytes,
            &runtime.model,
            &runtime.options,
            runtime.auto_retry_attempts,
            Some(abort_rx),
            |notice, error| {
                attempts_used = notice.attempt + 1;
                emit_retry(&app_clone, None, notice, error.to_string());
                let unarmed = abort_tx.take();
                let app = app_clone.clone();
                let speech = Arc::clone(&speech_inner);
                async move {
                    if let Some(tx) = unarmed {
                        speech.lock().await.arm_retry_abort(tx);
                    }
                    let shown = app
                        .state::<RecordingWidget>()
                        .retrying_if_current(
                            &app,
                            generation,
                            notice.attempt + 1,
                            notice.max_attempts,
                            (notice.delay_secs * 1000) as u32,
                        )
                        .await;
                    if !shown {
                        notify_retry_started(&app, notice).await;
                    }
                }
            },
        )
        .await;

        speech_inner.lock().await.disarm_retry_abort();
        drop(abort_tx);

        match result {
            Ok(text) => {
                let duration_secs = start.elapsed().as_secs_f64();
                if attempts_used > 1 {
                    widget_inner.processing_if_current(&app_clone, generation).await;
                }
                let (pending, stop_chat) = speech_inner.lock().await.take_pending_prompt();
                widget_inner.refresh_model(&app_clone).await;
                let mut created_entry_id = None;

                let skill_model = match pending.as_ref() {
                    Some(p) if p.origin == PendingOrigin::Pill => {
                        let skills = app_clone.state::<Arc<Mutex<SkillService>>>();
                        let model = skills.lock().await.get_skill(&p.id).and_then(|s| s.model.clone());
                        model
                    }
                    _ => None,
                };

                let claude_target = configured_claude_target(&app_clone).await;
                let stop_chat_target = configured_stop_chat_target(&app_clone).await;
                let plan = transcript_plan(
                    pending.as_ref(),
                    stop_chat,
                    &text,
                    skill_model,
                    claude_target,
                    stop_chat_target,
                );
                let output_route = speech_output_route(&plan);
                match plan {
                    TranscriptPlan::AlternativeExecute => {
                        let pending = pending.expect("plan implies a pending prompt");
                        widget_inner.close(&app_clone).await;
                        let _ = app_clone.emit(
                            "speech-alternative-execute",
                            AlternativeExecutePayload {
                                skill_id: pending.id,
                                skill_name: pending.name,
                                text: text.clone(),
                            },
                        );
                    }
                    TranscriptPlan::NoSpeech => {
                        widget_inner.close_if_current(&app_clone, generation).await;
                        notify_no_speech(&app_clone).await;
                    }
                    TranscriptPlan::SkillRun { skill_id } => {
                        run_pill_skill(&app_clone, skill_id, text.clone(), generation).await;
                    }
                    TranscriptPlan::Dictation => {
                        if let Err(e) = crate::services::clipboard::write_text(&app_clone, &text) {
                            log::error!("Failed to copy transcription to clipboard: {e}");
                        }

                        created_entry_id = record_speech_entry(&app_clone, &history_inner, &text, output_route.as_deref()).await;

                        if widget_inner.is_shown().await {
                            widget_inner.done(&app_clone).await;
                        } else {
                            let duration_display = if attempts_used > 1 {
                                format!(
                                    "Processed in {:.1}s, attempt {attempts_used} of {}",
                                    duration_secs,
                                    runtime.auto_retry_attempts.max(1)
                                )
                            } else {
                                format!("Processed in {:.1}s", duration_secs)
                            };
                            NotificationService::notify_now(
                                &app_clone,
                                "speech_transcription_success",
                                NotificationLevel::Success,
                                "Speech transcribed",
                                Some(duration_display),
                            )
                            .await;
                        }
                    }
                    TranscriptPlan::Chat(request) => {
                        created_entry_id = record_speech_entry(&app_clone, &history_inner, &text, output_route.as_deref()).await;
                        widget_inner.close_if_current(&app_clone, generation).await;
                        let copy_token = snapshot_clipboard(&app_clone).await;
                        open_chat(&app_clone, request, copy_token).await;
                    }
                    TranscriptPlan::Webview { provider_id } => {
                        created_entry_id = record_speech_entry(&app_clone, &history_inner, &text, output_route.as_deref()).await;
                        widget_inner.close_if_current(&app_clone, generation).await;
                        log::debug!("transcript: sending to the provider chat, provider={provider_id}");
                        crate::services::ai_webview::send_to_provider_chat(
                            &app_clone,
                            &provider_id,
                            text.clone(),
                        )
                        .await;
                    }
                    TranscriptPlan::ClaudeDesktop => {
                        created_entry_id = record_speech_entry(&app_clone, &history_inner, &text, output_route.as_deref()).await;
                        widget_inner.close_if_current(&app_clone, generation).await;
                        tauri::async_runtime::spawn(claude_desktop::deliver(app_clone.clone(), text.clone()));
                    }
                }

                let _ = app_clone.emit(
                    "speech-transcription-complete",
                    TranscriptionComplete {
                        text,
                        duration_secs,
                        entry_id: created_entry_id.clone(),
                    },
                );

                if let Some(clip_id) = clip_id {
                    let history = history_inner.lock().await;
                    match (runtime.keep_audio_on_success, created_entry_id) {
                        (true, Some(entry_id)) => {
                            let _ =
                                clip_inner.attach_to_entry(history.conn(), &clip_id, &entry_id);
                        }
                        _ => {
                            let _ = clip_inner.delete(history.conn(), &clip_id);
                        }
                    }
                }

                end_transcription(&app_clone, &speech_inner).await;
            }
            Err(SpeechError::NoSpeechDetected) => {
                widget_inner.close(&app_clone).await;
                emit_empty_transcription(&app_clone, start.elapsed().as_secs_f64());

                if let Some(clip_id) = clip_id {
                    let history = history_inner.lock().await;
                    let _ = clip_inner.delete(history.conn(), &clip_id);
                }

                let mut s = speech_inner.lock().await;
                let had_pending = s.take_pending_prompt().0.is_some();

                let title = if had_pending {
                    "Speech execution cancelled"
                } else {
                    "No speech detected"
                };
                let body = if had_pending {
                    "No speech detected — prompt execution cancelled"
                } else {
                    "No speech was detected in the recording"
                };

                NotificationService::notify_now(
                    &app_clone,
                    "speech_transcription_success",
                    NotificationLevel::Info,
                    title,
                    Some(body),
                )
                .await;

                s.set_transcribing(false);
                drop(s);
                composer::emit_busy_changed(&app_clone).await;
            }
            Err(SpeechError::Cancelled) => {
                let (entry_id, has_audio) = add_failed_entry(
                    &*history_inner.lock().await,
                    &clip_inner,
                    clip_id.as_deref(),
                    cancelled_input_content(duration_secs),
                    SpeechError::Cancelled.to_string(),
                );
                let _ = crate::services::history_events::emit_history_changed(
                    &app_clone,
                    entry_id.clone(),
                    None,
                );

                let pill_shown = widget_inner.is_shown().await;
                widget_inner.close_if_current(&app_clone, generation).await;
                speech_inner.lock().await.clear_pending_prompt();

                if !pill_shown {
                    let title = if has_audio {
                        "Transcription stopped — audio kept, retry from the menu"
                    } else {
                        "Transcription stopped"
                    };
                    NotificationService::notify_now(
                        &app_clone,
                        "speech_transcription_cancel",
                        NotificationLevel::Info,
                        title,
                        None::<String>,
                    )
                    .await;
                }

                end_transcription(&app_clone, &speech_inner).await;
                let _ = app_clone.emit(
                    "speech-transcription-error",
                    SpeechTranscriptionError {
                        message: SpeechError::Cancelled.to_string(),
                        recoverable: has_audio,
                        entry_id,
                        has_audio,
                        attempts: attempts_used,
                    },
                );
            }
            Err(e) => {
                widget_inner.close(&app_clone).await;
                let message = e.to_string();
                speech_inner.lock().await.clear_pending_prompt();

                let (entry_id, has_audio) = add_failed_entry(
                    &*history_inner.lock().await,
                    &clip_inner,
                    clip_id.as_deref(),
                    failure_input_content(duration_secs),
                    message.clone(),
                );
                let _ = crate::services::history_events::emit_history_changed(
                    &app_clone,
                    entry_id.clone(),
                    None,
                );

                let _ = app_clone.emit(
                    "speech-transcription-error",
                    SpeechTranscriptionError {
                        message: message.clone(),
                        recoverable: has_audio,
                        entry_id,
                        has_audio,
                        attempts: attempts_used,
                    },
                );

                let body = if has_audio {
                    format!(
                        "{message} — audio kept for {}h, retry from the menu or history",
                        runtime.audio_retention_hours
                    )
                } else {
                    message
                };
                NotificationService::notify_now(
                    &app_clone,
                    "speech_transcription_success",
                    NotificationLevel::Error,
                    "Transcription error",
                    Some(body),
                )
                .await;

                end_transcription(&app_clone, &speech_inner).await;
            }
        }
    });

    Ok(())
}

/// Replays a stored recording for an entry whose transcription failed.
/// The result only updates the entry and the clipboard — it is never pasted,
/// because the originating context is long gone by the time a retry happens.
pub async fn retry_transcription(app: AppHandle, entry_id: String) -> crate::Result<()> {
    let speech_state = app.state::<Arc<Mutex<SpeechService>>>();
    let config_state = app.state::<Arc<Mutex<ConfigService>>>();
    let history_state = app.state::<Arc<Mutex<SqliteHistoryService>>>();
    let clip_state = app.state::<Arc<AudioClipStore>>();

    if !speech_state.lock().await.begin_retry(&entry_id) {
        log::debug!("retry_transcription: already retrying {entry_id}");
        return Ok(());
    }

    let clip = {
        let history = history_state.lock().await;
        clip_state.find_by_entry(history.conn(), &entry_id)
    };

    let Some(clip) = clip else {
        speech_state.lock().await.end_retry(&entry_id);
        return Err(Error::Speech(SpeechError::AudioUnavailable));
    };

    let runtime = match load_stt_runtime(&config_state).await {
        Ok(runtime) => runtime,
        Err(e) => {
            speech_state.lock().await.end_retry(&entry_id);
            return Err(Error::Speech(e));
        }
    };

    let clip_for_read = clip.clone();
    let clip_store_for_read = Arc::clone(&*clip_state);
    let wav_bytes = match tokio::task::spawn_blocking(move || {
        clip_store_for_read.read_bytes(&clip_for_read)
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?
    {
        Ok(bytes) => bytes,
        Err(e) => {
            speech_state.lock().await.end_retry(&entry_id);
            log::warn!("retry_transcription: clip unreadable for {entry_id}: {e}");
            return Err(Error::Speech(SpeechError::AudioUnavailable));
        }
    };

    let app_clone = app.clone();
    let speech_inner = Arc::clone(&*speech_state);
    let history_inner = Arc::clone(&*history_state);
    let clip_inner = Arc::clone(&*clip_state);

    tokio::spawn(async move {
        let attempts = runtime.auto_retry_attempts.max(1);

        emit_retry(
            &app_clone,
            Some(entry_id.clone()),
            speech::retry::RetryNotice {
                attempt: 0,
                max_attempts: attempts,
                delay_secs: 0,
            },
            "Manual retry started".to_string(),
        );

        let event_entry_id = entry_id.clone();
        let result = speech::transcribe_with_retry(
            wav_bytes,
            &runtime.model,
            &runtime.options,
            attempts,
            None,
            |notice, error| {
                emit_retry(
                    &app_clone,
                    Some(event_entry_id.clone()),
                    notice,
                    error.to_string(),
                );
                async {}
            },
        )
        .await;

        match result {
            Ok(text) => {
                if let Err(e) = crate::services::clipboard::write_text(&app_clone, &text) {
                    log::error!("Failed to copy retried transcription to clipboard: {e}");
                }

                {
                    let history = history_inner.lock().await;
                    if let Err(e) = history.update_entry_result(
                        &entry_id,
                        Some(text.clone()),
                        Some(text.clone()),
                        true,
                        None,
                    ) {
                        log::error!("failed to update retried history entry: {e}");
                    }
                    let _ = clip_inner.delete_by_entry(history.conn(), &entry_id);
                }

                let _ = crate::services::history_events::emit_history_changed(
                    &app_clone,
                    None,
                    None,
                );
                let _ = app_clone.emit(
                    "speech-transcription-complete",
                    TranscriptionComplete {
                        text,
                        duration_secs: clip.duration_secs,
                        entry_id: Some(entry_id.clone()),
                    },
                );

                NotificationService::notify_now(
                    &app_clone,
                    "speech_transcription_success",
                    NotificationLevel::Success,
                    "Transcription recovered",
                    Some("Text copied to clipboard"),
                )
                .await;
            }
            Err(e) => {
                let message = e.to_string();
                {
                    let history = history_inner.lock().await;
                    if let Err(e) = history.update_entry_result(
                        &entry_id,
                        None,
                        None,
                        false,
                        Some(message.clone()),
                    ) {
                        log::error!("failed to update failed retry entry: {e}");
                    }
                }
                let _ = crate::services::history_events::emit_history_changed(
                    &app_clone,
                    None,
                    None,
                );
                let _ = app_clone.emit(
                    "speech-transcription-error",
                    SpeechTranscriptionError {
                        message: message.clone(),
                        recoverable: true,
                        entry_id: Some(entry_id.clone()),
                        has_audio: true,
                        attempts,
                    },
                );
                NotificationService::notify_now(
                    &app_clone,
                    "speech_transcription_success",
                    NotificationLevel::Error,
                    "Retry failed",
                    Some(message),
                )
                .await;
            }
        }

        speech_inner.lock().await.end_retry(&entry_id);
    });

    Ok(())
}

pub async fn pause_speech_recording(app: AppHandle) -> crate::Result<()> {
    if refused_during_composer(&app).await {
        return Ok(());
    }
    let speech_state = app.state::<Arc<Mutex<SpeechService>>>();
    {
        let mut s = speech_state.lock().await;
        s.pause()?;
        log::info!("pause_speech_recording: session={}", s.session());
    }
    app.state::<RecordingWidget>().pause(&app).await;
    Ok(())
}

pub async fn resume_speech_recording(app: AppHandle) -> crate::Result<()> {
    if refused_during_composer(&app).await {
        return Ok(());
    }
    let speech_state = app.state::<Arc<Mutex<SpeechService>>>();
    {
        let mut s = speech_state.lock().await;
        s.resume()?;
        log::info!("resume_speech_recording: session={}", s.session());
    }
    app.state::<RecordingWidget>().resume(&app).await;
    Ok(())
}

pub async fn cancel_speech_recording(app: AppHandle) -> crate::Result<()> {
    if refused_during_composer(&app).await {
        return Ok(());
    }
    let speech_state = app.state::<Arc<Mutex<SpeechService>>>();
    let (samples, sample_rate) = {
        let mut s = speech_state.lock().await;
        if s.cancel_retry() {
            log::info!("cancel_speech_recording: transcription retry cancelled");
            return Ok(());
        }
        if !s.is_recording() || s.is_transcribing() {
            log::debug!("cancel_speech_recording: ignored, not recording");
            return Ok(());
        }
        let audio = s.cancel_recording()?;
        log::info!("cancel_speech_recording: session={}", s.session());
        audio
    };

    composer::emit_busy_changed(&app).await;
    let widget = app.state::<RecordingWidget>();
    let pill_shown = widget.is_shown().await;
    widget.close(&app).await;
    let _ = app.emit(
        "speech-recording-stopped",
        SpeechRecordingStoppedEvent { had_audio: false },
    );
    emit_empty_transcription(&app, 0.0);
    keep_cancelled_recording(&app, samples, sample_rate, pill_shown).await;
    Ok(())
}

async fn keep_cancelled_recording(
    app: &AppHandle,
    samples: Vec<i16>,
    sample_rate: u32,
    pill_shown: bool,
) {
    let retention_hours = app
        .state::<Arc<Mutex<ConfigService>>>()
        .lock()
        .await
        .settings()
        .surfaces
        .speech_to_text
        .audio_retention_hours;
    let duration_secs = samples.len() as f64 / sample_rate.max(1) as f64;
    if retention_hours == 0 || duration_secs < MIN_RECORDING_SAMPLES_SECS {
        log::debug!(
            "cancel_speech_recording: audio discarded, retention_hours={retention_hours} duration_secs={duration_secs:.1}"
        );
        return;
    }

    let wav_bytes = match tokio::task::spawn_blocking(move || speech::encode_wav(&samples, sample_rate))
        .await
        .map_err(|e| e.to_string())
        .and_then(|encoded| encoded.map_err(|e| e.to_string()))
    {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("cancel_speech_recording: encoding the audio failed, nothing kept: {e}");
            return;
        }
    };

    let history_state = app.state::<Arc<Mutex<SqliteHistoryService>>>();
    let clips = app.state::<Arc<AudioClipStore>>();
    let history = history_state.lock().await;
    let clip_id = match clips.store(
        history.conn(),
        &wav_bytes,
        sample_rate,
        duration_secs,
        retention_hours,
    ) {
        Ok(id) => id,
        Err(e) => {
            log::warn!("cancel_speech_recording: storing the audio failed, nothing kept: {e}");
            return;
        }
    };
    let (entry_id, _) = add_failed_entry(
        &history,
        &clips,
        Some(&clip_id),
        cancelled_input_content(duration_secs),
        SpeechError::Cancelled.to_string(),
    );
    if entry_id.is_none() {
        log::warn!("cancel_speech_recording: history entry not created, dropping the clip");
        let _ = clips.delete(history.conn(), &clip_id);
        return;
    }
    drop(history);

    let _ = crate::services::history_events::emit_history_changed(app, entry_id, None);
    if pill_shown {
        return;
    }
    NotificationService::notify_now(
        app,
        "speech_recording_cancel",
        NotificationLevel::Info,
        "Recording cancelled — audio kept, transcribe from the menu",
        None::<String>,
    )
    .await;
}

async fn recording_widget_shown(app: &AppHandle, intent: &str) -> bool {
    let shown = app.state::<RecordingWidget>().is_shown().await;
    if !shown {
        log::debug!("recording widget intent {intent} ignored: widget not shown");
    }
    shown
}

pub async fn select_recording_widget_target(app: AppHandle, id: String) -> crate::Result<()> {
    log::debug!("recording widget intent: select target id={id}");
    if !recording_widget_shown(&app, "select target").await {
        return Ok(());
    }
    let providers = configured_providers(&app).await;
    let claude_target = configured_claude_target(&app).await;
    let label = match app.try_state::<Arc<Mutex<SkillService>>>() {
        Some(skills) => pill_targets(skills.lock().await.list_skills(), &providers, claude_target)
            .into_iter()
            .find(|(target_id, _)| *target_id == id)
            .map(|(_, label)| label),
        None => None,
    };
    let Some(label) = label else {
        log::debug!("recording widget intent select target ignored: unknown id={id}");
        return Ok(());
    };
    let selected = app
        .state::<Arc<Mutex<SpeechService>>>()
        .lock()
        .await
        .select_target(&id, &label);
    match selected {
        Ok(()) => app.state::<RecordingWidget>().refresh_model(&app).await,
        Err(_) => log::debug!("recording widget intent select target ignored: target locked"),
    }
    Ok(())
}

pub async fn set_recording_widget_dropdown(
    app: AppHandle,
    open: bool,
    rows: u32,
    separator: bool,
) -> crate::Result<&'static str> {
    log::debug!("recording widget intent: dropdown open={open} rows={rows} separator={separator}");
    let widget = app.state::<RecordingWidget>();
    if !open {
        widget.close_dropdown(&app).await;
        return Ok("down");
    }
    let up = widget.open_dropdown(&app, rows, separator).await?;
    Ok(if up { "up" } else { "down" })
}

pub async fn set_recording_widget_preview(
    app: AppHandle,
    open: bool,
    index: Option<u32>,
    text: Option<String>,
) -> crate::Result<&'static str> {
    log::debug!("recording widget intent: preview open={open} index={index:?}");
    let widget = app.state::<RecordingWidget>();
    if !open {
        widget.close_preview(&app).await;
        return Ok("down");
    }
    let up = widget.open_preview(&app, index, text).await?;
    Ok(if up { "up" } else { "down" })
}

pub async fn set_recording_widget_tooltip(
    app: AppHandle,
    open: bool,
    text: Option<String>,
    center_x: Option<f64>,
) -> crate::Result<&'static str> {
    log::debug!("recording widget intent: tooltip open={open} center_x={center_x:?}");
    let widget = app.state::<RecordingWidget>();
    if !open {
        widget.close_tooltip(&app).await;
        return Ok("down");
    }
    let up = widget.open_tooltip(&app, text, center_x).await?;
    Ok(if up { "up" } else { "down" })
}

pub async fn recording_widget_overlay_rendered(app: AppHandle, seq: u64) -> crate::Result<()> {
    app.state::<RecordingWidget>().overlay_rendered(&app, seq).await;
    Ok(())
}

pub async fn remove_recording_widget_item(
    app: AppHandle,
    index: u32,
    context_version: u64,
) -> crate::Result<()> {
    log::debug!("recording widget intent: remove item index={index} version={context_version}");
    if !recording_widget_shown(&app, "remove item").await {
        return Ok(());
    }
    crate::commands::context::remove_item_for_pill(&app, index as usize, context_version).await
}

pub async fn stop_chat_speech_recording(app: AppHandle) -> crate::Result<()> {
    log::debug!("recording widget intent: stop chat");
    if refused_during_composer(&app).await {
        return Ok(());
    }
    if !recording_widget_shown(&app, "stop chat").await {
        return Ok(());
    }
    let (recording, requested) = {
        let speech = app.state::<Arc<Mutex<SpeechService>>>();
        let mut s = speech.lock().await;
        (s.is_recording(), s.request_stop_chat())
    };
    if requested.is_err() {
        log::debug!("recording widget intent stop chat ignored: target locked");
        return Ok(());
    }
    log::debug!("stop chat requested, recording={recording}");
    if recording {
        toggle_speech_recording(app, None).await?;
    }
    Ok(())
}

pub async fn paste_into_recording_widget(app: AppHandle) -> crate::Result<()> {
    log::debug!("recording widget intent: paste");
    if !recording_widget_shown(&app, "paste").await {
        return Ok(());
    }
    if target_locked(&app).await {
        log::debug!("recording widget intent paste ignored: target locked");
        return Ok(());
    }
    crate::commands::context::paste_item_for_pill(&app).await
}

pub async fn capture_screenshot_into_recording_widget(app: AppHandle) -> crate::Result<()> {
    log::debug!("recording widget intent: screenshot");
    let widget = app.state::<RecordingWidget>();
    let Some((transport, generation)) = widget.shown_generation().await else {
        log::debug!("recording widget intent screenshot ignored: widget not shown");
        return Ok(());
    };
    if !screenshot::keep_capture(generation, Some(generation), target_locked(&app).await) {
        log::debug!("recording widget intent screenshot ignored: target locked");
        return Ok(());
    }

    let Some(bytes) = capture::screenshot(&app, &Owner::Pill { transport, generation }).await? else {
        return Ok(());
    };
    app.state::<Arc<Mutex<ContextManagerService>>>()
        .lock()
        .await
        .append_item(PastedItem::Image(capture::png_base64(bytes), "image/png".into()));
    crate::commands::context::emit_context_changed(&app)
}

pub async fn capture_selection_into_recording_widget(app: AppHandle) -> crate::Result<()> {
    log::debug!("recording widget intent: select text");
    let widget = app.state::<RecordingWidget>();
    let Some((transport, generation)) = widget.shown_generation().await else {
        log::debug!("recording widget intent select text ignored: widget not shown");
        return Ok(());
    };
    if !screenshot::keep_capture(generation, Some(generation), target_locked(&app).await) {
        log::debug!("recording widget intent select text ignored: target locked");
        return Ok(());
    }

    let Some(text) = capture::selection(&app, &Owner::Pill { transport, generation }).await else {
        return Ok(());
    };
    app.state::<Arc<Mutex<ContextManagerService>>>()
        .lock()
        .await
        .append_item(PastedItem::Text(text));
    crate::commands::context::emit_context_changed(&app)
}

pub fn cancel_recording_widget_selection() {
    log::debug!("recording widget intent: cancel select text");
    selection::cancel();
}

pub(super) async fn target_locked(app: &AppHandle) -> bool {
    app.state::<Arc<Mutex<SpeechService>>>().lock().await.target_locked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pill_target() -> PendingPrompt {
        PendingPrompt {
            id: "summarize".into(),
            name: "Summarize".into(),
            origin: PendingOrigin::Pill,
        }
    }

    fn pill_chat(skill_id: &str, model: Option<&str>) -> TranscriptPlan {
        TranscriptPlan::Chat(ChatRequest {
            skill_id: skill_id.into(),
            initial_input: "hello".into(),
            skill_model: model.map(String::from),
            auto_send: true,
            source: InvocationSource::Pill,
        })
    }

    #[test]
    fn plan_skill_target_with_stop_chat_opens_chat_without_skill_run() {
        let plan = transcript_plan(Some(&pill_target()), true, "hello", Some("m1".into()), ClaudeTarget::Webview, StopChatTarget::Chat);
        assert_eq!(plan, pill_chat("summarize", Some("m1")));
    }

    #[test]
    fn plan_dictation_with_stop_chat_opens_chat_without_clipboard_write() {
        assert_eq!(transcript_plan(None, true, "hello", None, ClaudeTarget::Webview, StopChatTarget::Chat), pill_chat("", None));
    }

    #[test]
    fn plan_skill_target_without_stop_chat_runs_skill() {
        assert_eq!(
            transcript_plan(Some(&pill_target()), false, "hello", None, ClaudeTarget::Webview, StopChatTarget::Chat),
            TranscriptPlan::SkillRun { skill_id: "summarize".into() }
        );
    }

    #[test]
    fn plan_dictation_without_stop_chat_writes_clipboard_and_done() {
        assert_eq!(
            transcript_plan(None, false, "hello", None, ClaudeTarget::Webview, StopChatTarget::Chat),
            TranscriptPlan::Dictation
        );
    }

    #[test]
    fn plan_empty_transcript_with_skill_and_stop_chat_opens_no_chat() {
        assert_eq!(
            transcript_plan(Some(&pill_target()), true, "  ", None, ClaudeTarget::Webview, StopChatTarget::Chat),
            TranscriptPlan::NoSpeech
        );
    }

    fn webview_target() -> PendingPrompt {
        PendingPrompt { id: "webview:claude".into(), name: "Claude".into(), ..pill_target() }
    }

    #[test]
    fn plan_webview_target_routes_to_the_provider_with_and_without_stop_chat() {
        for stop_chat in [true, false] {
            assert_eq!(
                transcript_plan(Some(&webview_target()), stop_chat, "hello", None, ClaudeTarget::Webview, StopChatTarget::Chat),
                TranscriptPlan::Webview { provider_id: "claude".into() }
            );
        }
    }

    #[test]
    fn plan_webview_target_with_an_empty_transcript_is_no_speech() {
        for text in ["", "  "] {
            assert_eq!(
                transcript_plan(Some(&webview_target()), true, text, None, ClaudeTarget::Webview, StopChatTarget::Chat),
                TranscriptPlan::NoSpeech
            );
        }
    }

    #[test]
    fn plan_claude_desktop_webview_target_follows_the_claude_target_setting() {
        for stop_chat in [true, false] {
            assert_eq!(
                transcript_plan(Some(&webview_target()), stop_chat, "hello", None, ClaudeTarget::Webview, StopChatTarget::Chat),
                TranscriptPlan::Webview { provider_id: "claude".into() }
            );
            assert_eq!(
                transcript_plan(Some(&webview_target()), stop_chat, "hello", None, ClaudeTarget::Desktop, StopChatTarget::Chat),
                TranscriptPlan::ClaudeDesktop
            );
        }
    }

    #[test]
    fn plan_claude_desktop_empty_transcript_is_no_speech() {
        assert_eq!(
            transcript_plan(Some(&webview_target()), true, "  ", None, ClaudeTarget::Desktop, StopChatTarget::Chat),
            TranscriptPlan::NoSpeech
        );
    }

    #[test]
    fn plan_claude_desktop_setting_does_not_change_the_chatgpt_target() {
        let pending = PendingPrompt { id: "webview:chatgpt".into(), name: "ChatGPT".into(), ..pill_target() };
        assert_eq!(
            transcript_plan(Some(&pending), true, "hello", None, ClaudeTarget::Desktop, StopChatTarget::Chat),
            TranscriptPlan::Webview { provider_id: "chatgpt".into() }
        );
    }

    #[test]
    fn plan_shift_click_emits_alternative_execute() {
        let pending = PendingPrompt { origin: PendingOrigin::ShiftClick, ..pill_target() };
        assert_eq!(
            transcript_plan(Some(&pending), true, "hello", None, ClaudeTarget::Webview, StopChatTarget::Chat),
            TranscriptPlan::AlternativeExecute
        );
    }

    fn claude_stop_chat_plan(pending: Option<&PendingPrompt>, claude_target: ClaudeTarget) -> TranscriptPlan {
        transcript_plan(pending, true, "hello", None, claude_target, StopChatTarget::Claude)
    }

    fn assert_stop_chat_claude_follows_claude_target(pending: Option<&PendingPrompt>) {
        assert_eq!(claude_stop_chat_plan(pending, ClaudeTarget::Desktop), TranscriptPlan::ClaudeDesktop);
        assert_eq!(
            claude_stop_chat_plan(pending, ClaudeTarget::Webview),
            TranscriptPlan::Webview { provider_id: "claude".into() }
        );
    }

    #[test]
    fn plan_stop_chat_claude_without_pending_follows_the_claude_target() {
        assert_stop_chat_claude_follows_claude_target(None);
    }

    #[test]
    fn plan_stop_chat_claude_wins_over_a_pending_skill() {
        assert_stop_chat_claude_follows_claude_target(Some(&pill_target()));
    }

    #[test]
    fn plan_stop_chat_claude_wins_over_a_pending_chatgpt_target() {
        let pending = PendingPrompt { id: "webview:chatgpt".into(), name: "ChatGPT".into(), ..pill_target() };
        assert_stop_chat_claude_follows_claude_target(Some(&pending));
    }

    #[test]
    fn plan_stop_chat_claude_keeps_shift_click_alternative_execute() {
        let pending = PendingPrompt { origin: PendingOrigin::ShiftClick, ..pill_target() };
        assert_eq!(
            claude_stop_chat_plan(Some(&pending), ClaudeTarget::Desktop),
            TranscriptPlan::AlternativeExecute
        );
    }

    #[test]
    fn plan_stop_chat_claude_without_stop_chat_keeps_dictation_and_skill_run() {
        assert_eq!(
            transcript_plan(None, false, "hello", None, ClaudeTarget::Desktop, StopChatTarget::Claude),
            TranscriptPlan::Dictation
        );
        assert_eq!(
            transcript_plan(Some(&pill_target()), false, "hello", None, ClaudeTarget::Desktop, StopChatTarget::Claude),
            TranscriptPlan::SkillRun { skill_id: "summarize".into() }
        );
    }

    #[test]
    fn speech_output_route_names_the_destination_of_each_plan() {
        let route = |plan: TranscriptPlan| speech_output_route(&plan);
        assert_eq!(route(TranscriptPlan::Dictation), Some("clipboard".into()));
        assert_eq!(route(pill_chat("", None)), Some("chat".into()));
        assert_eq!(route(TranscriptPlan::ClaudeDesktop), Some("claude_desktop".into()));
        assert_eq!(
            route(TranscriptPlan::Webview { provider_id: "chatgpt".into() }),
            Some("webview:chatgpt".into())
        );
        assert_eq!(route(TranscriptPlan::AlternativeExecute), None);
        assert_eq!(route(TranscriptPlan::NoSpeech), None);
        assert_eq!(route(TranscriptPlan::SkillRun { skill_id: "summarize".into() }), None);
    }

    #[test]
    fn clip_duration_is_formatted_as_minutes_and_seconds() {
        assert_eq!(format_clip_duration(0.0), "0:00");
        assert_eq!(format_clip_duration(9.4), "0:09");
        assert_eq!(format_clip_duration(125.0), "2:05");
    }

    #[test]
    fn cancelled_entry_content_shows_duration() {
        assert_eq!(cancelled_input_content(42.0), "Voice note · 0:42 — cancelled");
    }

    #[test]
    fn failure_entry_keeps_duration_visible() {
        let content = failure_input_content(184.0);
        assert!(content.contains("3:04"));
        assert!(content.contains("transcription failed"));
    }
}
