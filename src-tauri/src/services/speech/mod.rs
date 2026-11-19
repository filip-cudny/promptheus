mod clip_store;
pub mod capture;
pub mod claude_desktop;
pub mod composer;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod pill_previews;
mod recorder;
pub mod reminder;
pub mod retry;
mod screenshot;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub mod selection;
pub mod session;
mod transcriber;
pub mod widget;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use tokio::sync::watch;

use recorder::{find_input_device, negotiate_sample_rate};

use crate::models::speech::{DictationBusy, DictationBy};
use crate::services::dialog::{CONVERSATION_DIALOG_LABEL, SETTINGS_DIALOG_LABEL};

pub use clip_store::{AudioClipStore, ClipStoreError};
pub use recorder::encode_wav;
pub use retry::transcribe_with_retry;
pub use transcriber::{transcribe, SttOptions};

const TOGGLE_DEBOUNCE_MS: u128 = 250;

pub const DICTATION_ID: &str = "__dictation__";
pub const DICTATION_LABEL: &str = "Dictation";
pub const WEBVIEW_TARGET_PREFIX: &str = "webview:";

pub fn webview_target_id(provider_id: &str) -> String {
    format!("{WEBVIEW_TARGET_PREFIX}{provider_id}")
}

pub fn webview_provider_of(target_id: &str) -> Option<&str> {
    target_id.strip_prefix(WEBVIEW_TARGET_PREFIX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOrigin {
    ShiftClick,
    Pill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPrompt {
    pub id: String,
    pub name: String,
    pub origin: PendingOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetLocked;

/// Whether a failed transcription is worth sending again unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FailureKind {
    Transient,
    Permanent,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("Already recording")]
    AlreadyRecording,
    #[error("Not recording")]
    NotRecording,
    #[error("No input device found")]
    NoInputDevice,
    #[error("No supported audio configuration found")]
    NoSupportedConfig,
    #[error("Failed to build audio stream: {0}")]
    StreamBuild(String),
    #[error("WAV encoding error: {0}")]
    WavEncode(String),
    #[error("{message}")]
    Transcription {
        message: String,
        kind: FailureKind,
        retry_after_secs: Option<u64>,
    },
    #[error("API key not configured")]
    ApiKeyMissing,
    #[error("No speech detected")]
    NoSpeechDetected,
    #[error("Recording failed: {0}")]
    RecordingFailed(String),
    #[error("No audio available for this transcription")]
    AudioUnavailable,
    #[error("Audio storage error: {0}")]
    ClipStore(#[from] ClipStoreError),
    #[error("Cancelled")]
    Cancelled,
    #[error("{0}")]
    Busy(String),
}

impl SpeechError {
    pub fn transient(message: impl Into<String>) -> Self {
        Self::Transcription {
            message: message.into(),
            kind: FailureKind::Transient,
            retry_after_secs: None,
        }
    }

    pub fn transient_after(message: impl Into<String>, retry_after_secs: Option<u64>) -> Self {
        Self::Transcription {
            message: message.into(),
            kind: FailureKind::Transient,
            retry_after_secs,
        }
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self::Transcription {
            message: message.into(),
            kind: FailureKind::Permanent,
            retry_after_secs: None,
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Transcription {
                kind: FailureKind::Transient,
                ..
            }
        )
    }

    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::Transcription {
                retry_after_secs, ..
            } => *retry_after_secs,
            _ => None,
        }
    }
}

/// A composer (main chat or settings agent) that records and transcribes as its own session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerOwner {
    pub label: String,
    pub tab_id: Option<String>,
    by: DictationBy,
}

impl ComposerOwner {
    pub fn new(label: &str, tab_id: Option<String>) -> Option<Self> {
        let by = match label {
            CONVERSATION_DIALOG_LABEL => DictationBy::Chat,
            SETTINGS_DIALOG_LABEL => DictationBy::Settings,
            _ => return None,
        };
        Some(Self { label: label.to_string(), tab_id, by })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionOwner {
    Pill,
    Composer(ComposerOwner),
}

struct HeldRecording {
    tab_id: Option<String>,
    wav: Vec<u8>,
}

fn busy_message(by: DictationBy) -> &'static str {
    match by {
        DictationBy::Pill => "The recording pill is already in use",
        DictationBy::Chat => "Dictation is already running in Chat",
        DictationBy::Settings => "Dictation is already running in Settings",
    }
}

pub struct SpeechService {
    owner: SessionOwner,
    held: HashMap<String, HeldRecording>,
    is_recording: bool,
    is_transcribing: bool,
    recording_action_id: Option<String>,
    pending: Option<PendingPrompt>,
    taken: Option<PendingPrompt>,
    target_locked: bool,
    stop_chat: bool,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    stop_sender: Option<mpsc::Sender<()>>,
    last_toggle: Option<Instant>,
    session: u64,
    started_at: Option<Instant>,
    paused: Arc<AtomicBool>,
    paused_at: Option<Instant>,
    paused_total: Duration,
    retrying_entries: HashSet<String>,
    retry_abort: Option<watch::Sender<bool>>,
}

trait RecorderStream {
    fn play(&self) -> Result<(), cpal::PlayStreamError>;
}

impl RecorderStream for cpal::Stream {
    fn play(&self) -> Result<(), cpal::PlayStreamError> {
        StreamTrait::play(self)
    }
}

/// The stream is built on the recorder thread because it is not `Send` on every
/// platform; the thread reports its start once before this returns.
fn spawn_recorder<S: RecorderStream>(
    build: impl FnOnce() -> Result<S, cpal::BuildStreamError> + Send + 'static,
    stop_rx: mpsc::Receiver<()>,
) -> Result<(), SpeechError> {
    let (started_tx, started_rx) = mpsc::sync_channel(1);

    std::thread::spawn(move || {
        let stream = match build() {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("Failed to build audio stream: {e}");
                let _ = started_tx.send(Err(SpeechError::StreamBuild(e.to_string())));
                return;
            }
        };
        if let Err(e) = stream.play() {
            log::error!("Failed to start audio stream: {e}");
            let _ = started_tx.send(Err(SpeechError::RecordingFailed(e.to_string())));
            return;
        }
        let _ = started_tx.send(Ok(()));
        let _ = stop_rx.recv();
    });

    started_rx.recv().unwrap_or_else(|_| {
        log::error!("Recorder thread ended before reporting its start");
        Err(SpeechError::RecordingFailed(
            "recorder thread ended before starting".into(),
        ))
    })
}

fn append_unless_paused(buffer: &Mutex<Vec<i16>>, paused: &AtomicBool, data: &[i16]) {
    if paused.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut buf) = buffer.lock() {
        buf.extend_from_slice(data);
    }
}

fn elapsed_excluding_pauses(
    now: Instant,
    started_at: Instant,
    paused_total: Duration,
    paused_at: Option<Instant>,
) -> Duration {
    let current_pause = paused_at.map_or(Duration::ZERO, |t| now.saturating_duration_since(t));
    now.saturating_duration_since(started_at)
        .saturating_sub(paused_total)
        .saturating_sub(current_pause)
}

impl SpeechService {
    pub fn new() -> Self {
        Self {
            owner: SessionOwner::Pill,
            held: HashMap::new(),
            is_recording: false,
            is_transcribing: false,
            recording_action_id: None,
            pending: None,
            taken: None,
            target_locked: false,
            stop_chat: false,
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 16000,
            stop_sender: None,
            last_toggle: None,
            session: 0,
            started_at: None,
            paused: Arc::new(AtomicBool::new(false)),
            paused_at: None,
            paused_total: Duration::ZERO,
            retrying_entries: HashSet::new(),
            retry_abort: None,
        }
    }

    pub fn start_recording(&mut self, action_id: Option<String>) -> Result<u32, SpeechError> {
        if self.is_recording {
            return Err(SpeechError::AlreadyRecording);
        }

        self.reset_target_for_start(action_id.as_deref());

        let device = find_input_device()?;
        let sample_rate = negotiate_sample_rate(&device)?;

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let buffer_clone = Arc::clone(&buffer);
        let paused = Arc::new(AtomicBool::new(false));
        let paused_clone = Arc::clone(&paused);

        self.start_with(action_id, sample_rate, buffer, paused, move || {
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    append_unless_paused(&buffer_clone, &paused_clone, data);
                },
                |err| {
                    log::error!("Audio stream error: {err}");
                },
                None,
            )
        })
    }

    fn start_with<S: RecorderStream>(
        &mut self,
        action_id: Option<String>,
        sample_rate: u32,
        buffer: Arc<Mutex<Vec<i16>>>,
        paused: Arc<AtomicBool>,
        build: impl FnOnce() -> Result<S, cpal::BuildStreamError> + Send + 'static,
    ) -> Result<u32, SpeechError> {
        let (stop_tx, stop_rx) = mpsc::channel();
        spawn_recorder(build, stop_rx)?;

        self.audio_buffer = buffer;
        self.paused = paused;
        self.paused_at = None;
        self.paused_total = Duration::ZERO;
        self.sample_rate = sample_rate;
        self.is_recording = true;
        self.owner = SessionOwner::Pill;
        self.recording_action_id = action_id;
        self.stop_sender = Some(stop_tx);
        self.session = self.session.wrapping_add(1);
        self.started_at = Some(Instant::now());

        Ok(sample_rate)
    }

    pub fn stop_recording_raw(&mut self) -> Result<(Vec<i16>, u32), SpeechError> {
        if !self.is_recording {
            return Err(SpeechError::NotRecording);
        }

        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }

        let samples = {
            let mut buf = self.audio_buffer.lock().unwrap();
            std::mem::take(&mut *buf)
        };

        let sample_rate = self.sample_rate;
        self.is_recording = false;
        self.recording_action_id = None;
        self.started_at = None;
        self.paused.store(false, Ordering::Relaxed);
        self.paused_at = None;
        self.paused_total = Duration::ZERO;

        Ok((samples, sample_rate))
    }

    pub fn cancel_recording(&mut self) -> Result<(Vec<i16>, u32), SpeechError> {
        let audio = self.stop_recording_raw()?;
        self.clear_pending_prompt();
        Ok(audio)
    }

    pub fn busy(&self) -> Option<DictationBusy> {
        if !self.is_recording && !self.is_transcribing {
            return None;
        }
        let by = match &self.owner {
            SessionOwner::Pill => DictationBy::Pill,
            SessionOwner::Composer(owner) => owner.by,
        };
        Some(DictationBusy { by })
    }

    /// The single guard every pill entry point asks first: the window whose composer
    /// owns the session, `None` when the pill may act.
    pub fn composer_window(&self) -> Option<&'static str> {
        match self.busy()?.by {
            DictationBy::Pill => None,
            DictationBy::Chat => Some("Chat"),
            DictationBy::Settings => Some("Settings"),
        }
    }

    pub fn composer_session(&self, label: &str) -> Option<ComposerOwner> {
        match &self.owner {
            SessionOwner::Composer(owner)
                if owner.label == label && (self.is_recording || self.is_transcribing) =>
            {
                Some(owner.clone())
            }
            _ => None,
        }
    }

    fn ensure_idle(&self) -> Result<(), SpeechError> {
        match self.busy() {
            Some(busy) => Err(SpeechError::Busy(busy_message(busy.by).to_string())),
            None => Ok(()),
        }
    }

    pub fn start_composer_recording(&mut self, owner: ComposerOwner) -> Result<u32, SpeechError> {
        self.ensure_idle()?;
        self.held.remove(&owner.label);
        let sample_rate = self.start_recording(None)?;
        self.owner = SessionOwner::Composer(owner);
        Ok(sample_rate)
    }

    /// Starts transcribing the held audio of `label`; the returned session id tells a late
    /// result of an earlier transcription apart.
    pub fn begin_held_transcription(&mut self, label: &str) -> Result<(ComposerOwner, u64, Vec<u8>), SpeechError> {
        self.ensure_idle()?;
        let held = self.held.remove(label).ok_or(SpeechError::AudioUnavailable)?;
        let owner = ComposerOwner::new(label, held.tab_id).ok_or(SpeechError::AudioUnavailable)?;
        self.owner = SessionOwner::Composer(owner.clone());
        self.is_transcribing = true;
        self.session = self.session.wrapping_add(1);
        Ok((owner, self.session, held.wav))
    }

    pub fn owns_composer_transcription(&self, owner: &ComposerOwner, session: u64) -> bool {
        self.is_transcribing && self.session == session && self.owner == SessionOwner::Composer(owner.clone())
    }

    pub fn release_composer(&mut self) {
        self.is_transcribing = false;
        self.owner = SessionOwner::Pill;
        self.retry_abort = None;
    }

    pub fn hold_recording(&mut self, owner: &ComposerOwner, wav: Vec<u8>) {
        self.held.insert(owner.label.clone(), HeldRecording { tab_id: owner.tab_id.clone(), wav });
    }

    pub fn drop_held_recording(&mut self, label: &str) {
        self.held.remove(label);
    }

    /// Ends the composer session of `label`, discarding its audio, and returns its owner when
    /// a recording or transcription was running. A held recording of the label is dropped too.
    pub fn cancel_composer(&mut self, label: &str) -> Option<ComposerOwner> {
        self.held.remove(label);
        let owner = self.composer_session(label)?;
        if self.is_recording {
            let _ = self.cancel_recording();
        } else {
            self.cancel_retry();
        }
        self.release_composer();
        Some(owner)
    }

    pub fn arm_retry_abort(&mut self, tx: watch::Sender<bool>) {
        self.retry_abort = Some(tx);
    }

    pub fn cancel_retry(&mut self) -> bool {
        match &self.retry_abort {
            Some(tx) => {
                let _ = tx.send(true);
                true
            }
            None => false,
        }
    }

    pub fn disarm_retry_abort(&mut self) {
        self.retry_abort = None;
    }

    pub fn pause(&mut self) -> Result<(), SpeechError> {
        if !self.is_recording {
            return Err(SpeechError::NotRecording);
        }
        if self.paused_at.is_none() {
            self.paused.store(true, Ordering::Relaxed);
            self.paused_at = Some(Instant::now());
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), SpeechError> {
        if !self.is_recording {
            return Err(SpeechError::NotRecording);
        }
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_total += paused_at.elapsed();
            self.paused.store(false, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    /// Recording time without paused spans.
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|started_at| {
            elapsed_excluding_pauses(Instant::now(), started_at, self.paused_total, self.paused_at)
        })
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording
    }

    /// Monotonic id of the current recording, used by background tasks to tell
    /// whether the session they were started for is still the active one.
    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn audio_buffer(&self) -> Arc<Mutex<Vec<i16>>> {
        Arc::clone(&self.audio_buffer)
    }

    pub fn is_transcribing(&self) -> bool {
        self.is_transcribing
    }

    pub fn set_transcribing(&mut self, value: bool) {
        self.is_transcribing = value;
    }

    pub fn recording_action_id(&self) -> Option<&str> {
        self.recording_action_id.as_deref()
    }

    pub fn set_pending_prompt(&mut self, id: String, name: String, origin: PendingOrigin) {
        self.pending = Some(PendingPrompt { id, name, origin });
    }

    pub fn clear_pending_prompt(&mut self) {
        self.pending = None;
    }

    #[cfg(test)]
    pub fn pending_prompt(&self) -> Option<&PendingPrompt> {
        self.pending.as_ref()
    }

    pub fn shown_target(&self) -> Option<&PendingPrompt> {
        if self.target_locked {
            self.taken.as_ref()
        } else {
            self.pending.as_ref()
        }
    }

    pub fn target_locked(&self) -> bool {
        self.target_locked
    }

    /// Keeps only the Shift+click skill that started this very recording.
    pub fn reset_target_for_start(&mut self, action_id: Option<&str>) {
        let keep = self.pending.as_ref().is_some_and(|pending| {
            pending.origin == PendingOrigin::ShiftClick && Some(pending.id.as_str()) == action_id
        });
        if !keep {
            self.pending = None;
        }
        self.target_locked = false;
        self.stop_chat = false;
    }

    pub fn request_stop_chat(&mut self) -> Result<(), TargetLocked> {
        if self.target_locked {
            return Err(TargetLocked);
        }
        self.stop_chat = true;
        Ok(())
    }

    pub fn select_target(&mut self, id: &str, name: &str) -> Result<(), TargetLocked> {
        if self.target_locked {
            return Err(TargetLocked);
        }
        self.pending = (id != DICTATION_ID).then(|| PendingPrompt {
            id: id.to_string(),
            name: name.to_string(),
            origin: PendingOrigin::Pill,
        });
        Ok(())
    }

    /// Takes the pending skill and locks the target in one step, so a pill
    /// selection cannot slip in between.
    pub fn take_pending_prompt(&mut self) -> (Option<PendingPrompt>, bool) {
        self.target_locked = true;
        self.taken = self.pending.take();
        (self.taken.clone(), self.stop_chat)
    }

    /// Manual retries are tracked per history entry rather than through
    /// `is_transcribing`, so replaying an old clip never blocks a new recording.
    pub fn begin_retry(&mut self, entry_id: &str) -> bool {
        self.retrying_entries.insert(entry_id.to_string())
    }

    pub fn end_retry(&mut self, entry_id: &str) {
        self.retrying_entries.remove(entry_id);
    }

    pub fn is_retrying(&self, entry_id: &str) -> bool {
        self.retrying_entries.contains(entry_id)
    }

    pub fn retrying_entries(&self) -> HashSet<String> {
        self.retrying_entries.clone()
    }

    pub fn mark_toggle(&mut self) {
        self.last_toggle = Some(Instant::now());
    }

    pub fn is_debouncing(&self) -> bool {
        self.last_toggle
            .map(|t| t.elapsed().as_millis() < TOGGLE_DEBOUNCE_MS)
            .unwrap_or(false)
    }
}

/// Locks the speech service only while `session` is still the active recording.
async fn lock_active_session(
    speech: &Arc<tokio::sync::Mutex<SpeechService>>,
    session: u64,
) -> Option<tokio::sync::MutexGuard<'_, SpeechService>> {
    let s = speech.lock().await;
    (s.is_recording() && s.session() == session).then_some(s)
}

/// The speech service and its audio buffer, or `None` once `session` is no longer the active recording.
async fn start_session_task(
    app: &tauri::AppHandle,
    session: u64,
) -> Option<(Arc<tokio::sync::Mutex<SpeechService>>, Arc<Mutex<Vec<i16>>>)> {
    use tauri::Manager;

    let speech = app
        .try_state::<Arc<tokio::sync::Mutex<SpeechService>>>()?
        .inner()
        .clone();
    let buffer = lock_active_session(&speech, session).await?.audio_buffer();
    Some((speech, buffer))
}

/// RMS of the samples recorded since `cursor`, which then moves to the end of the buffer.
fn tail_rms(buffer: &Mutex<Vec<i16>>, cursor: &mut usize) -> f64 {
    let buf = buffer.lock().unwrap_or_else(|e| e.into_inner());
    if buf.len() < *cursor {
        *cursor = buf.len();
    }
    let tail = &buf[*cursor..];
    *cursor = buf.len();
    reminder::rms(tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_service_is_not_recording() {
        let service = SpeechService::new();
        assert!(!service.is_recording());
        assert!(service.recording_action_id().is_none());
    }

    struct FakeStream {
        play_result: fn() -> Result<(), cpal::PlayStreamError>,
    }

    impl RecorderStream for FakeStream {
        fn play(&self) -> Result<(), cpal::PlayStreamError> {
            (self.play_result)()
        }
    }

    fn start_fake(
        service: &mut SpeechService,
        build: impl FnOnce() -> Result<FakeStream, cpal::BuildStreamError> + Send + 'static,
    ) -> Result<u32, SpeechError> {
        service.start_with(
            None,
            16000,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
            build,
        )
    }

    #[test]
    fn a_stream_build_error_fails_the_start_and_leaves_the_service_idle() {
        let mut service = SpeechService::new();
        let result = start_fake(&mut service, || Err(cpal::BuildStreamError::DeviceNotAvailable));
        assert!(matches!(result, Err(SpeechError::StreamBuild(_))));
        assert!(!service.is_recording());
        assert!(service.stop_sender.is_none());
    }

    #[test]
    fn a_play_error_fails_the_start_and_leaves_the_service_idle() {
        let mut service = SpeechService::new();
        let result = start_fake(&mut service, || {
            Ok(FakeStream {
                play_result: || Err(cpal::PlayStreamError::DeviceNotAvailable),
            })
        });
        assert!(matches!(result, Err(SpeechError::RecordingFailed(_))));
        assert!(!service.is_recording());
    }

    #[test]
    fn a_panicking_recorder_thread_fails_the_start() {
        let mut service = SpeechService::new();
        let result = start_fake(&mut service, || panic!("builder panicked"));
        assert!(matches!(result, Err(SpeechError::RecordingFailed(_))));
        assert!(!service.is_recording());
    }

    #[test]
    fn a_playing_stream_starts_the_recording() {
        let mut service = SpeechService::new();
        let result = start_fake(&mut service, || {
            Ok(FakeStream {
                play_result: || Ok(()),
            })
        });
        assert_eq!(result.unwrap(), 16000);
        assert!(service.is_recording());
        assert!(service.stop_recording_raw().is_ok());
    }

    #[test]
    fn stop_without_recording_returns_error() {
        let mut service = SpeechService::new();
        let result = service.stop_recording_raw();
        assert!(result.is_err());
    }

    #[test]
    fn append_unless_paused_drops_samples_while_paused() {
        let buffer = Mutex::new(Vec::new());
        let paused = AtomicBool::new(true);
        append_unless_paused(&buffer, &paused, &[1, 2, 3]);
        assert!(buffer.lock().unwrap().is_empty());

        paused.store(false, Ordering::Relaxed);
        append_unless_paused(&buffer, &paused, &[4, 5]);
        assert_eq!(*buffer.lock().unwrap(), vec![4, 5]);
    }

    #[test]
    fn elapsed_excludes_paused_span() {
        let started = Instant::now();
        let now = started + Duration::from_secs(5);
        let elapsed = elapsed_excluding_pauses(now, started, Duration::from_secs(2), None);
        assert_eq!(elapsed, Duration::from_secs(3));
    }

    #[test]
    fn elapsed_excludes_current_pause() {
        let started = Instant::now();
        let now = started + Duration::from_secs(5);
        let paused_at = Some(started + Duration::from_secs(3));
        let elapsed = elapsed_excluding_pauses(now, started, Duration::ZERO, paused_at);
        assert_eq!(elapsed, Duration::from_secs(3));
    }

    #[test]
    fn cancel_recording_returns_the_recorded_samples() {
        let mut service = SpeechService::new();
        start_fake(&mut service, || Ok(FakeStream { play_result: || Ok(()) })).unwrap();
        service.audio_buffer().lock().unwrap().extend_from_slice(&[1, 2, 3]);
        service.set_pending_prompt("prompt-1".into(), "Prompt One".into(), PendingOrigin::ShiftClick);

        assert_eq!(service.cancel_recording().unwrap(), (vec![1, 2, 3], 16000));
        assert!(!service.is_recording());
        assert!(!service.is_transcribing());
        assert_eq!(service.take_pending_prompt().0, None);
    }

    #[test]
    fn retry_abort_fires_only_while_armed() {
        let mut service = SpeechService::new();
        assert!(!service.cancel_retry());

        let (tx, rx) = watch::channel(false);
        service.arm_retry_abort(tx);
        assert!(!rx.has_changed().unwrap());
        assert!(service.cancel_retry());
        assert!(rx.has_changed().unwrap());
        assert!(*rx.borrow());

        service.disarm_retry_abort();
        assert!(!service.cancel_retry());
    }

    #[test]
    fn pause_and_resume_without_recording_return_error() {
        let mut service = SpeechService::new();
        assert!(service.pause().is_err());
        assert!(service.resume().is_err());
    }

    #[test]
    fn pause_and_resume_toggle_paused_state() {
        let mut service = SpeechService::new();
        service.is_recording = true;
        service.started_at = Some(Instant::now());

        service.pause().unwrap();
        assert!(service.is_paused());
        assert!(service.is_recording());
        service.pause().unwrap();
        assert!(service.is_paused());

        service.resume().unwrap();
        assert!(!service.is_paused());
        service.resume().unwrap();
        assert!(!service.is_paused());
    }

    #[test]
    fn not_debouncing_on_new_service() {
        let service = SpeechService::new();
        assert!(!service.is_debouncing());
    }

    #[test]
    fn debouncing_right_after_toggle() {
        let mut service = SpeechService::new();
        service.mark_toggle();
        assert!(service.is_debouncing());
    }

    #[test]
    fn retry_guard_is_per_entry() {
        let mut service = SpeechService::new();
        assert!(service.begin_retry("entry-1"));
        assert!(!service.begin_retry("entry-1"));
        assert!(service.begin_retry("entry-2"));
        assert!(service.is_retrying("entry-1"));

        service.end_retry("entry-1");
        assert!(!service.is_retrying("entry-1"));
        assert!(service.is_retrying("entry-2"));
    }

    #[test]
    fn retry_guard_does_not_touch_transcribing_flag() {
        let mut service = SpeechService::new();
        service.begin_retry("entry-1");
        assert!(!service.is_transcribing());
    }

    #[test]
    fn pending_prompt_lifecycle() {
        let mut service = SpeechService::new();
        assert_eq!(service.take_pending_prompt().0, None);

        service.set_pending_prompt("prompt-1".into(), "Prompt One".into(), PendingOrigin::ShiftClick);
        let pending = service.take_pending_prompt().0.unwrap();
        assert_eq!(pending.id, "prompt-1");
        assert_eq!(pending.name, "Prompt One");
        assert_eq!(pending.origin, PendingOrigin::ShiftClick);
        assert_eq!(service.take_pending_prompt().0, None);
    }

    #[test]
    fn selecting_a_skill_sets_the_slot_with_pill_origin() {
        let mut service = SpeechService::new();
        service.select_target("fix", "Fix").unwrap();
        assert_eq!(
            service.pending_prompt(),
            Some(&PendingPrompt {
                id: "fix".into(),
                name: "Fix".into(),
                origin: PendingOrigin::Pill
            })
        );
    }

    #[test]
    fn a_webview_target_is_locked_by_the_take_and_keeps_its_shown_name() {
        let mut service = SpeechService::new();
        service.select_target("webview:claude", "Claude").unwrap();
        service.take_pending_prompt();
        assert_eq!(service.select_target("other", "Other"), Err(TargetLocked));
        assert_eq!(service.shown_target().map(|p| p.name.as_str()), Some("Claude"));
    }

    fn composer(label: &str) -> ComposerOwner {
        ComposerOwner::new(label, Some("tab-1".into())).unwrap()
    }

    fn service_recording_for(owner: SessionOwner) -> SpeechService {
        let mut service = SpeechService::new();
        start_fake(&mut service, || Ok(FakeStream { play_result: || Ok(()) })).unwrap();
        service.owner = owner;
        service
    }

    #[test]
    fn busy_names_who_records_or_transcribes() {
        let mut service = SpeechService::new();
        assert_eq!(service.busy(), None);

        let by = |service: &SpeechService| service.busy().map(|busy| busy.by);
        let mut pill = service_recording_for(SessionOwner::Pill);
        assert_eq!(by(&pill), Some(DictationBy::Pill));
        pill.stop_recording_raw().unwrap();
        assert_eq!(by(&pill), None);
        pill.set_transcribing(true);
        assert_eq!(by(&pill), Some(DictationBy::Pill));

        for (label, expected) in [
            (CONVERSATION_DIALOG_LABEL, DictationBy::Chat),
            (SETTINGS_DIALOG_LABEL, DictationBy::Settings),
        ] {
            let mut service = service_recording_for(SessionOwner::Composer(composer(label)));
            assert_eq!(by(&service), Some(expected));
            service.stop_recording_raw().unwrap();
            service.set_transcribing(true);
            assert_eq!(by(&service), Some(expected));
            service.release_composer();
            assert_eq!(by(&service), None);
        }
    }

    #[test]
    fn the_guard_names_the_window_of_a_composer_session_and_lets_the_pill_through() {
        let mut service = SpeechService::new();
        assert_eq!(service.composer_window(), None);

        let pill = service_recording_for(SessionOwner::Pill);
        assert_eq!(pill.composer_window(), None);

        let mut chat = service_recording_for(SessionOwner::Composer(composer(CONVERSATION_DIALOG_LABEL)));
        assert_eq!(chat.composer_window(), Some("Chat"));
        chat.stop_recording_raw().unwrap();
        chat.set_transcribing(true);
        assert_eq!(chat.composer_window(), Some("Chat"));

        service = service_recording_for(SessionOwner::Composer(composer(SETTINGS_DIALOG_LABEL)));
        assert_eq!(service.composer_window(), Some("Settings"));
    }

    #[test]
    fn a_composer_start_is_refused_during_a_pill_session_and_during_another_composers() {
        let mut pill = service_recording_for(SessionOwner::Pill);
        let refused = pill.start_composer_recording(composer(CONVERSATION_DIALOG_LABEL));
        assert!(matches!(refused, Err(SpeechError::Busy(_))));
        assert_eq!(pill.busy().map(|busy| busy.by), Some(DictationBy::Pill));

        let mut chat = service_recording_for(SessionOwner::Composer(composer(CONVERSATION_DIALOG_LABEL)));
        let refused = chat.start_composer_recording(composer(SETTINGS_DIALOG_LABEL));
        assert!(matches!(&refused, Err(SpeechError::Busy(message)) if message.contains("Chat")));
        assert_eq!(chat.busy().map(|busy| busy.by), Some(DictationBy::Chat));

        chat.stop_recording_raw().unwrap();
        chat.set_transcribing(true);
        let refused = chat.start_composer_recording(composer(SETTINGS_DIALOG_LABEL));
        assert!(matches!(refused, Err(SpeechError::Busy(_))));
    }

    #[test]
    fn only_the_two_composer_windows_can_own_a_session() {
        assert!(ComposerOwner::new("palette", None).is_none());
        assert!(ComposerOwner::new(CONVERSATION_DIALOG_LABEL, None).is_some());
        assert!(ComposerOwner::new(SETTINGS_DIALOG_LABEL, None).is_some());
    }

    #[test]
    fn a_pill_start_takes_the_session_back_from_a_finished_composer() {
        let mut service = service_recording_for(SessionOwner::Composer(composer(CONVERSATION_DIALOG_LABEL)));
        service.cancel_composer(CONVERSATION_DIALOG_LABEL).unwrap();
        start_fake(&mut service, || Ok(FakeStream { play_result: || Ok(()) })).unwrap();
        assert_eq!(service.busy().map(|busy| busy.by), Some(DictationBy::Pill));
    }

    #[test]
    fn selecting_plain_dictation_clears_the_slot() {
        let mut service = SpeechService::new();
        service.select_target("fix", "Fix").unwrap();
        service.select_target(DICTATION_ID, DICTATION_LABEL).unwrap();
        assert_eq!(service.pending_prompt(), None);
    }

    #[test]
    fn selection_is_rejected_after_the_take() {
        let mut service = SpeechService::new();
        service.select_target("fix", "Fix").unwrap();
        service.take_pending_prompt();
        assert!(service.target_locked());
        assert_eq!(service.select_target("other", "Other"), Err(TargetLocked));
        assert_eq!(service.pending_prompt(), None);
    }

    #[test]
    fn reset_without_shift_click_drops_the_previous_pending_skill() {
        let mut service = SpeechService::new();
        service.select_target("fix", "Fix").unwrap();
        service.take_pending_prompt();
        service.set_pending_prompt("stale".into(), "Stale".into(), PendingOrigin::Pill);
        service.reset_target_for_start(None);
        assert_eq!(service.pending_prompt(), None);
        assert!(!service.target_locked());
    }

    #[test]
    fn reset_with_the_matching_shift_click_keeps_it() {
        let mut service = SpeechService::new();
        service.set_pending_prompt("fix".into(), "Fix".into(), PendingOrigin::ShiftClick);
        service.reset_target_for_start(Some("fix"));
        assert_eq!(service.pending_prompt().map(|p| p.id.as_str()), Some("fix"));
    }

    #[test]
    fn reset_drops_a_pill_selection_and_a_shift_click_for_another_skill() {
        let mut service = SpeechService::new();
        service.select_target("fix", "Fix").unwrap();
        service.reset_target_for_start(Some("fix"));
        assert_eq!(service.pending_prompt(), None);

        service.set_pending_prompt("fix".into(), "Fix".into(), PendingOrigin::ShiftClick);
        service.reset_target_for_start(Some("other"));
        assert_eq!(service.pending_prompt(), None);
    }

    #[test]
    fn the_stop_chat_flag_is_reset_at_a_start_and_returned_by_the_take() {
        let mut service = SpeechService::new();
        service.request_stop_chat().unwrap();
        service.reset_target_for_start(None);
        assert!(!service.take_pending_prompt().1);

        service.reset_target_for_start(None);
        service.request_stop_chat().unwrap();
        assert!(service.take_pending_prompt().1);
    }

    #[test]
    fn stop_chat_is_rejected_after_the_take() {
        let mut service = SpeechService::new();
        service.take_pending_prompt();
        assert_eq!(service.request_stop_chat(), Err(TargetLocked));
        assert!(!service.take_pending_prompt().1);
    }
}
