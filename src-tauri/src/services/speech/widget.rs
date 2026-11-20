use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tokio::sync::Mutex;

use super::{
    lock_active_session, start_session_task, tail_rms, webview_target_id, PendingPrompt,
    SpeechService, DICTATION_ID, DICTATION_LABEL,
};
use crate::commands::context::current_context_version;
use crate::models::context::ContextItem;
use crate::models::settings::{ClaudeTarget, PillAnchor, Skin, StopChatTarget, WebviewProvider};
use crate::models::skill::{is_offered_in, OfferedIn, Skill};
use crate::services::config::ConfigService;
use crate::services::context::ContextManagerService;
use crate::services::monitor::{clamp_origin, find_monitor_at};
use crate::services::skill::SkillService;
use crate::services::ui_state::UiStateService;

pub const WINDOW_LABEL: &str = "recording-widget";
pub const OVERLAY_WINDOW_LABEL: &str = "recording-widget-overlay";
pub const POINTER_EVENT: &str = "recording-widget-pointer";
pub const OVERLAY_POINTER_EVENT: &str = "recording-widget-overlay-pointer";
pub const WINDOW_POSITION_KEY: &str = "recording_widget.window_position";
#[cfg(target_os = "linux")]
pub const SHELL_POSITION_KEY: &str = "recording_widget.shell_position";

const STATE_EVENT: &str = "recording-widget-state";
const MODEL_EVENT: &str = "recording-widget-model";
const OVERLAY_EVENT: &str = "recording-widget-overlay";
const PREVIEW_FRAME_HEIGHT: f64 = 166.0;
const TOOLTIP_FRAME_HEIGHT: f64 = 30.0;
const DROPDOWN_CLOSED_EVENT: &str = "recording-widget-dropdown-closed";
pub(super) const TICK: Duration = Duration::from_millis(50);
const DONE_VISIBLE: Duration = Duration::from_secs(1);
const MOVE_DEBOUNCE: Duration = Duration::from_millis(500);
const FULL_SCALE_RMS: f64 = 32768.0;
const FLOOR_DB: f64 = -50.0;
const CEIL_DB: f64 = -20.0;
const LEVEL_ATTACK: f64 = 0.6;
const LEVEL_RELEASE: f64 = 0.2;
const BOTTOM_MARGIN: f64 = 24.0;
const PILL_WIDTH: f64 = 264.0;
const PREVIEW_MAX_WIDTH: f64 = PILL_WIDTH;
const PREVIEW_MAX_HEIGHT: f64 = 160.0;
const PREVIEW_MIN_BOX: u32 = 32;
const PILL_HEIGHT: f64 = 72.0;
const MAX_ITEM_ROWS: u32 = 3;
const ITEM_ROW_HEIGHT: f64 = 24.0;
const ITEM_ROW_GAP: f64 = 4.0;
const ITEMS_ROW_PADDING: f64 = 10.0;
const IMAGE_COLUMNS: usize = 3;
const PREVIEW_CHARS: usize = 300;
const DROPDOWN_ROW_HEIGHT: f64 = 28.0;
const DROPDOWN_PADDING: f64 = 4.0;
const DROPDOWN_SEPARATOR_HEIGHT: f64 = 9.0;

type Rect = (i32, i32, u32, u32);
type WorkArea = (f64, f64, f64, f64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub origin: (f64, f64),
    pub work_area: WorkArea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    #[cfg(target_os = "linux")]
    Shell,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    #[cfg(target_os = "linux")]
    GnomeWayland,
    #[cfg(target_os = "linux")]
    OtherWayland,
    X11OrMacos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WidgetState {
    Recording,
    Paused,
    Processing,
    Retrying { attempt: u32, max_attempts: u32, delay_ms: u32 },
    Done,
}

impl WidgetState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Paused => "paused",
            Self::Processing => "processing",
            Self::Retrying { .. } => "retrying",
            Self::Done => "done",
        }
    }

    fn is_live(self) -> bool {
        matches!(self, Self::Recording | Self::Paused)
    }
}

fn transition_allowed(from: WidgetState, to: WidgetState) -> bool {
    match to {
        WidgetState::Paused | WidgetState::Recording => from.is_live(),
        WidgetState::Processing => from != WidgetState::Done,
        WidgetState::Retrying { .. } => {
            matches!(from, WidgetState::Processing | WidgetState::Retrying { .. })
        }
        WidgetState::Done => true,
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetPayload {
    state: &'static str,
    level: f64,
    elapsed_ms: u32,
    monochromatic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delay_ms: Option<u32>,
}

fn widget_payload(
    state: WidgetState,
    level: f64,
    elapsed_ms: u32,
    monochromatic: bool,
) -> WidgetPayload {
    let (attempt, max_attempts, delay_ms) = match state {
        WidgetState::Retrying { attempt, max_attempts, delay_ms } => {
            (Some(attempt), Some(max_attempts), Some(delay_ms))
        }
        _ => (None, None, None),
    };
    WidgetPayload {
        state: state.as_str(),
        level,
        elapsed_ms,
        monochromatic,
        attempt,
        max_attempts,
        delay_ms,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PillItem {
    pub index: u32,
    pub kind: String,
    pub label: String,
    pub preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PillModel {
    #[serde(serialize_with = "serialize_targets")]
    pub targets: Vec<(String, String)>,
    pub selected_id: String,
    pub selected_label: String,
    pub target_locked: bool,
    pub items: Vec<PillItem>,
    pub item_rows: u32,
    pub context_version: u64,
    pub stop_chat_destination: String,
    #[serde(skip)]
    pub x: i32,
    #[serde(skip)]
    pub y: i32,
}

#[derive(Serialize)]
struct TargetView<'a> {
    id: &'a str,
    label: &'a str,
}

fn serialize_targets<S: serde::Serializer>(
    targets: &[(String, String)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(targets.iter().map(|(id, label)| TargetView { id, label }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Dropdown,
    Preview,
    Tooltip,
}

impl OverlayKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dropdown => "dropdown",
            Self::Preview => "preview",
            Self::Tooltip => "tooltip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlay {
    pub kind: OverlayKind,
    pub up: bool,
    pub height: f64,
    pub seq: u64,
    pub shown: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayStep {
    InPlace,
    Reframe { hide: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayPlan {
    pub seq: u64,
    pub up: bool,
    pub frame: ((f64, f64), (f64, f64)),
    pub step: OverlayStep,
    pub replaced: Option<OverlayKind>,
}

#[derive(Debug, Clone, Default)]
pub struct OverlayContent {
    pub index: Option<u32>,
    pub text: Option<String>,
    pub center_x: Option<f64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayDescriptor {
    seq: u64,
    kind: &'static str,
    up: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    center_x: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub enum OverlayRequest {
    Open(Overlay),
    Close(OverlayKind),
}

pub fn next_overlay(
    current: Option<Overlay>,
    request: OverlayRequest,
) -> (Option<Overlay>, Option<OverlayKind>) {
    match request {
        OverlayRequest::Open(opened) => {
            let replaced = current.map(|c| c.kind).filter(|kind| *kind != opened.kind);
            (Some(opened), replaced)
        }
        OverlayRequest::Close(kind) => match current {
            Some(c) if c.kind == kind => (None, Some(kind)),
            _ => (current, None),
        },
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct WidgetSettings {
    enabled: bool,
    monochromatic: bool,
    at_cursor: bool,
    anchor: PillAnchor,
}

struct Inner {
    transport: Option<Transport>,
    session: u64,
    state: WidgetState,
    generation: u64,
    elapsed_ms: u32,
    monochromatic: bool,
    update_failed: bool,
    placement: Option<Placement>,
    item_rows: u32,
    scale: f64,
    #[cfg(target_os = "linux")]
    shell_scale: f64,
    preview_sizes: Vec<Option<(u32, u32)>>,
    grows_down: bool,
    overlay: Option<Overlay>,
    overlay_seq: u64,
    #[cfg(target_os = "linux")]
    shell: Option<crate::services::gnome_shell::ShellProxy<'static>>,
}

impl Inner {
    fn is_current(&self, generation: u64) -> bool {
        self.transport.is_some() && self.generation == generation
    }

    fn begin_overlay(&mut self, kind: OverlayKind, logical_height: f64) -> Result<OverlayPlan, String> {
        let placement = match (self.transport, self.placement) {
            (Some(Transport::Window), Some(placement)) => placement,
            _ => return Err("recording widget window is not shown".into()),
        };
        let pill = physical_size(pill_size(self.item_rows), self.scale);
        let pill = (pill.0 as f64, pill.1 as f64);
        let up = dropdown_opens_up(placement.origin.1, pill.1, placement.work_area);
        let height = logical_height * self.scale;
        self.overlay_seq += 1;
        let seq = self.overlay_seq;
        let (origin, size) = overlay_frame(placement.origin, pill, height, up, placement.work_area);
        let frame = ((origin.0.round(), origin.1.round()), size);
        let current = self.overlay;
        let in_place = current.is_some_and(|c| c.shown && c.kind == kind && c.up == up && c.height == height);
        let step = if in_place {
            OverlayStep::InPlace
        } else {
            OverlayStep::Reframe { hide: current.is_some_and(|c| c.shown) }
        };
        let opened = Overlay { kind, up, height, seq, shown: in_place };
        let (overlay, replaced) = next_overlay(current, OverlayRequest::Open(opened));
        self.overlay = overlay;
        Ok(OverlayPlan { seq, up, frame, step, replaced })
    }

    fn finish_overlay(&mut self, seq: u64) -> bool {
        match self.overlay.as_mut() {
            Some(overlay) if overlay.seq == seq && !overlay.shown => {
                overlay.shown = true;
                true
            }
            _ => false,
        }
    }

    fn end_overlay(&mut self, kind: OverlayKind) -> Option<OverlayKind> {
        let (overlay, closed) = next_overlay(self.overlay, OverlayRequest::Close(kind));
        self.overlay = overlay;
        closed
    }
}

pub struct RecordingWidget {
    inner: Mutex<Inner>,
    move_seq: AtomicU64,
    overlay_visible: AtomicBool,
    programmatic_origin: std::sync::Mutex<Option<(i32, i32)>>,
    thumbnails: super::pill_previews::ThumbnailCache,
    pixel_sizes: super::pill_previews::PixelSizeCache,
}

pub fn choose_transport(enabled: bool, session: SessionKind) -> Option<Transport> {
    if !enabled {
        return None;
    }
    match session {
        #[cfg(target_os = "linux")]
        SessionKind::GnomeWayland => Some(Transport::Shell),
        #[cfg(target_os = "linux")]
        SessionKind::OtherWayland => None,
        SessionKind::X11OrMacos => Some(Transport::Window),
    }
}

fn current_session_kind() -> SessionKind {
    #[cfg(target_os = "linux")]
    {
        if crate::services::gnome_shell::is_gnome_wayland() {
            return SessionKind::GnomeWayland;
        }
        if std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland") {
            return SessionKind::OtherWayland;
        }
    }
    SessionKind::X11OrMacos
}

pub fn level_from_rms(rms: f64) -> f64 {
    if rms <= 0.0 {
        return 0.0;
    }
    let dbfs = 20.0 * (rms / FULL_SCALE_RMS).log10();
    ((dbfs - FLOOR_DB) / (CEIL_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

pub fn smooth_level(previous: f64, target: f64) -> f64 {
    let rate = if target > previous { LEVEL_ATTACK } else { LEVEL_RELEASE };
    previous + (target - previous) * rate
}

pub fn item_rows(images: usize, texts: usize) -> u32 {
    (images.div_ceil(IMAGE_COLUMNS) + texts).min(MAX_ITEM_ROWS as usize) as u32
}

pub fn items_height(rows: u32) -> f64 {
    if rows == 0 {
        return 0.0;
    }
    rows as f64 * ITEM_ROW_HEIGHT + (rows - 1) as f64 * ITEM_ROW_GAP + ITEMS_ROW_PADDING
}

pub fn rows_resize(old_rows: u32, new_rows: u32) -> bool {
    old_rows != new_rows
}

pub fn pill_size(rows: u32) -> (f64, f64) {
    (PILL_WIDTH, PILL_HEIGHT + items_height(rows))
}

pub fn text_preview(content: &str) -> String {
    let mut chars = content.chars();
    let head: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

pub fn anchored_origin(
    cursor: (f64, f64),
    size: (f64, f64),
    anchor: PillAnchor,
    work_area: WorkArea,
) -> (f64, f64) {
    let x = match anchor {
        PillAnchor::AboveRight | PillAnchor::BelowRight => cursor.0,
        PillAnchor::AboveCenter | PillAnchor::BelowCenter => cursor.0 - size.0 / 2.0,
        PillAnchor::AboveLeft | PillAnchor::BelowLeft => cursor.0 - size.0,
    };
    let y = match anchor {
        PillAnchor::AboveRight | PillAnchor::AboveCenter | PillAnchor::AboveLeft => cursor.1 - size.1,
        PillAnchor::BelowRight | PillAnchor::BelowCenter | PillAnchor::BelowLeft => cursor.1,
    };
    clamp_origin((x, y), size, work_area)
}

fn grows_down(settings: WidgetSettings) -> bool {
    settings.at_cursor
        && matches!(
            settings.anchor,
            PillAnchor::BelowRight | PillAnchor::BelowCenter | PillAnchor::BelowLeft
        )
}

pub fn regrow_origin(
    origin: (f64, f64),
    old_height: f64,
    new_height: f64,
    width: f64,
    grows_down: bool,
    work_area: WorkArea,
) -> (f64, f64) {
    let top = if grows_down {
        origin.1
    } else {
        origin.1 + old_height - new_height
    };
    clamp_origin(
        (origin.0, top),
        (width, new_height),
        work_area,
    )
}

pub fn dropdown_opens_up(pill_origin_y: f64, pill_height: f64, work_area: WorkArea) -> bool {
    pill_origin_y + pill_height / 2.0 > work_area.1 + work_area.3 / 2.0
}

pub fn dropdown_height(rows: u32, separator: bool) -> f64 {
    let separator_height = if separator { DROPDOWN_SEPARATOR_HEIGHT } else { 0.0 };
    rows as f64 * DROPDOWN_ROW_HEIGHT + 2.0 * DROPDOWN_PADDING + separator_height
}

pub fn overlay_frame(
    pill_origin: (f64, f64),
    pill_size: (f64, f64),
    frame_height: f64,
    up: bool,
    work_area: WorkArea,
) -> ((f64, f64), (f64, f64)) {
    let size = (pill_size.0, frame_height);
    let top = if up { pill_origin.1 - frame_height } else { pill_origin.1 + pill_size.1 };
    (clamp_origin((pill_origin.0, top), size, work_area), size)
}

pub fn pill_targets(
    skills: &[Skill],
    providers: &[WebviewProvider],
    claude_target: ClaudeTarget,
) -> Vec<(String, String)> {
    let dictation = (DICTATION_ID.to_string(), DICTATION_LABEL.to_string());
    let offered = skills
        .iter()
        .filter(|skill| is_offered_in(&skill.offered_in, OfferedIn::Pill))
        .map(|skill| (skill.name.clone(), skill.display_name.clone()));
    let webviews = providers
        .iter()
        .filter(|provider| matches!(provider.id.as_str(), "claude" | "chatgpt"))
        .map(|provider| {
            let label = match (provider.id.as_str(), claude_target) {
                ("claude", ClaudeTarget::Desktop) => "Claude Desktop".to_string(),
                _ => provider.name.clone(),
            };
            (webview_target_id(&provider.id), label)
        });
    std::iter::once(dictation).chain(webviews).chain(offered).collect()
}

pub async fn configured_providers(app: &AppHandle) -> Vec<WebviewProvider> {
    match app.try_state::<Arc<Mutex<ConfigService>>>() {
        Some(config) => config.lock().await.settings().webview_providers.clone(),
        None => Vec::new(),
    }
}

pub async fn configured_claude_target(app: &AppHandle) -> ClaudeTarget {
    match app.try_state::<Arc<Mutex<ConfigService>>>() {
        Some(config) => config.lock().await.settings().surfaces.speech_to_text.claude_target,
        None => ClaudeTarget::default(),
    }
}

pub async fn configured_stop_chat_target(app: &AppHandle) -> StopChatTarget {
    match app.try_state::<Arc<Mutex<ConfigService>>>() {
        Some(config) => config.lock().await.settings().surfaces.speech_to_text.stop_chat_target,
        None => StopChatTarget::default(),
    }
}

pub fn stop_chat_destination(stop_chat_target: StopChatTarget, claude_target: ClaudeTarget) -> &'static str {
    match (stop_chat_target, claude_target) {
        (StopChatTarget::Chat, _) => "",
        (StopChatTarget::Claude, ClaudeTarget::Desktop) => "Claude Desktop",
        (StopChatTarget::Claude, ClaudeTarget::Webview) => "Claude",
    }
}

pub fn pill_items(items: &[ContextItem]) -> Vec<PillItem> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let (kind, label, preview) = match item {
                ContextItem::Text { number, content } => ("text", format!("#{number}"), text_preview(content)),
                ContextItem::Image { number, .. } => ("image", format!("#{number}"), String::new()),
            };
            PillItem {
                index: index as u32,
                kind: kind.to_string(),
                label,
                preview,
                thumbnail: None,
                preview_width: None,
                preview_height: None,
            }
        })
        .collect()
}

pub fn build_model(
    targets: Vec<(String, String)>,
    pending: Option<&PendingPrompt>,
    target_locked: bool,
    context_items: &[ContextItem],
    context_version: u64,
    origin: (i32, i32),
    stop_chat_destination: &str,
) -> PillModel {
    let (selected_id, selected_label) = match pending {
        Some(pending) => (pending.id.clone(), pending.name.clone()),
        None => (DICTATION_ID.to_string(), DICTATION_LABEL.to_string()),
    };
    let images = context_items
        .iter()
        .filter(|item| matches!(item, ContextItem::Image { .. }))
        .count();
    PillModel {
        targets,
        selected_id,
        selected_label,
        target_locked,
        item_rows: item_rows(images, context_items.len() - images),
        items: pill_items(context_items),
        context_version,
        stop_chat_destination: stop_chat_destination.to_string(),
        x: origin.0,
        y: origin.1,
    }
}

async fn current_context_items(app: &AppHandle) -> Vec<ContextItem> {
    match app.try_state::<Arc<Mutex<ContextManagerService>>>() {
        Some(context) => context.lock().await.get_items(),
        None => Vec::new(),
    }
}

async fn gather_model(app: &AppHandle, origin: (i32, i32), items: &[ContextItem]) -> PillModel {
    let providers = configured_providers(app).await;
    let claude_target = configured_claude_target(app).await;
    let targets = match app.try_state::<Arc<Mutex<SkillService>>>() {
        Some(skills) => pill_targets(skills.lock().await.list_skills(), &providers, claude_target),
        None => pill_targets(&[], &providers, claude_target),
    };
    let speech = app.try_state::<Arc<Mutex<SpeechService>>>();
    let (pending, locked) = match &speech {
        Some(speech) => {
            let speech = speech.lock().await;
            (speech.shown_target().cloned(), speech.target_locked())
        }
        None => (None, false),
    };
    let destination = stop_chat_destination(configured_stop_chat_target(app).await, claude_target);
    build_model(targets, pending.as_ref(), locked, items, current_context_version(), origin, destination)
}

async fn warm_image_caches(app: &AppHandle, context_items: &[ContextItem], thumbnails: bool) {
    let images: Vec<(String, String)> = context_items
        .iter()
        .filter_map(|item| match item {
            ContextItem::Image { data, media_type, .. } => Some((data.clone(), media_type.clone())),
            ContextItem::Text { .. } => None,
        })
        .collect();
    let app = app.clone();
    let warmed = tauri::async_runtime::spawn_blocking(move || {
        let widget = app.state::<RecordingWidget>();
        widget.pixel_sizes.refresh(&images);
        if thumbnails {
            widget.thumbnails.refresh(&images);
        }
    })
    .await;
    if let Err(e) = warmed {
        log::warn!("the pill image cache task failed: {e}");
    }
}

pub fn preview_display_size(pixels: (u32, u32), scale: f64) -> (u32, u32) {
    // Deliberate simplification: the divisor is the scale of the pill's monitor, so an image captured
    // or copied on a monitor of another scale shows at size × the scale ratio. Upgrade path: items
    // carry their own scale. Stops being correct when items carry their own scale.
    let width = pixels.0 as f64 / scale;
    let height = pixels.1 as f64 / scale;
    let factor = (PREVIEW_MAX_WIDTH / width).min(PREVIEW_MAX_HEIGHT / height).min(1.0);
    ((width * factor).round().max(1.0) as u32, (height * factor).round().max(1.0) as u32)
}

pub fn preview_box(display: (u32, u32)) -> (u32, u32) {
    (display.0.max(PREVIEW_MIN_BOX), display.1.max(PREVIEW_MIN_BOX))
}

fn preview_sizes(
    context_items: &[ContextItem],
    scale: f64,
    pixel_size: impl Fn(&str) -> Option<(u32, u32)>,
) -> Vec<Option<(u32, u32)>> {
    context_items
        .iter()
        .map(|item| match item {
            ContextItem::Image { data, .. } => pixel_size(data).map(|pixels| preview_display_size(pixels, scale)),
            ContextItem::Text { .. } => None,
        })
        .collect()
}

fn preview_scale(inner: &Inner, transport: Transport) -> f64 {
    match transport {
        Transport::Window => inner.scale,
        #[cfg(target_os = "linux")]
        Transport::Shell => inner.shell_scale,
    }
}

fn attach_preview_sizes(
    app: &AppHandle,
    inner: &mut Inner,
    transport: Transport,
    items: &mut [PillItem],
    context_items: &[ContextItem],
) {
    let widget = app.state::<RecordingWidget>();
    let sizes = preview_sizes(context_items, preview_scale(inner, transport), |data| widget.pixel_sizes.get(data));
    for (item, size) in items.iter_mut().zip(&sizes) {
        item.preview_width = size.map(|(width, _)| width);
        item.preview_height = size.map(|(_, height)| height);
    }
    inner.preview_sizes = sizes;
}

#[cfg(target_os = "linux")]
type ShellItem = (u32, String, String, String, String, u32, u32);

#[cfg(target_os = "linux")]
fn shell_items(items: Vec<PillItem>, context_items: &[ContextItem], image_paths: Vec<String>) -> Vec<ShellItem> {
    let mut paths = image_paths.into_iter();
    items
        .into_iter()
        .zip(context_items)
        .map(|(item, source)| {
            let image_path = match source {
                ContextItem::Image { .. } => paths.next().unwrap_or_default(),
                ContextItem::Text { .. } => String::new(),
            };
            (
                item.index,
                item.kind,
                item.label,
                item.preview,
                image_path,
                item.preview_width.unwrap_or(0),
                item.preview_height.unwrap_or(0),
            )
        })
        .collect()
}

fn attach_thumbnails(app: &AppHandle, items: &mut [PillItem], context_items: &[ContextItem]) {
    let widget = app.state::<RecordingWidget>();
    for (item, source) in items.iter_mut().zip(context_items) {
        if let ContextItem::Image { data, .. } = source {
            item.thumbnail = widget.thumbnails.get(data);
        }
    }
}

fn placement_origin(inner: &Inner) -> (i32, i32) {
    inner
        .placement
        .map_or((0, 0), |p| (p.origin.0.round() as i32, p.origin.1.round() as i32))
}

fn default_origin(work_area: Rect, size: (u32, u32), scale: f64) -> (i32, i32) {
    let (x, y, width, height) = work_area;
    let left = x + (width as i32 - size.0 as i32) / 2;
    let top = y + height as i32 - size.1 as i32 - (BOTTOM_MARGIN * scale) as i32;
    (left, top)
}

fn physical_size(logical: (f64, f64), scale: f64) -> (u32, u32) {
    ((logical.0 * scale).round() as u32, (logical.1 * scale).round() as u32)
}

fn should_store_drag(at_cursor: bool) -> bool {
    !at_cursor
}

fn work_area_of(monitor: &tauri::Monitor) -> WorkArea {
    let work = monitor.work_area();
    (
        work.position.x as f64,
        work.position.y as f64,
        work.size.width as f64,
        work.size.height as f64,
    )
}

fn log_placement(
    source: &str,
    anchor: PillAnchor,
    origin: (f64, f64),
    size: (f64, f64),
    work_area: WorkArea,
) {
    log::debug!(
        "recording widget: placed from {source} anchor={anchor:?} origin={origin:?} size={size:?} work_area={work_area:?}"
    );
}

fn is_inside_any(point: (i32, i32), work_areas: &[Rect]) -> bool {
    work_areas.iter().any(|&(x, y, width, height)| {
        point.0 >= x
            && point.0 < x + width as i32
            && point.1 >= y
            && point.1 < y + height as i32
    })
}

pub(super) fn elapsed_ms(elapsed: Duration) -> u32 {
    elapsed.as_millis().min(u32::MAX as u128) as u32
}

async fn read_position(app: &AppHandle, key: &str) -> Option<(i32, i32)> {
    let ui_state = app.try_state::<Arc<Mutex<UiStateService>>>()?;
    let value = ui_state.lock().await.get(key)?;
    Some((value.get("x")?.as_i64()? as i32, value.get("y")?.as_i64()? as i32))
}

pub async fn store_position(app: &AppHandle, key: &str, x: i32, y: i32) {
    let Some(ui_state) = app.try_state::<Arc<Mutex<UiStateService>>>() else {
        return;
    };
    let stored = ui_state
        .lock()
        .await
        .set(key, serde_json::json!({ "x": x, "y": y }));
    match stored {
        Ok(()) => log::debug!("recording widget position stored: {key}=({x}, {y})"),
        Err(e) => log::warn!("failed to store the recording widget position {key}: {e}"),
    }
}

pub fn window_moved(app: &AppHandle, position: PhysicalPosition<i32>) {
    if let Some(widget) = app.try_state::<RecordingWidget>() {
        widget.record_move(app, (position.x, position.y));
    }
}

async fn widget_settings(app: &AppHandle) -> WidgetSettings {
    let Some(config) = app.try_state::<Arc<Mutex<ConfigService>>>() else {
        return WidgetSettings::default();
    };
    let config = config.lock().await;
    let speech = &config.settings().surfaces.speech_to_text;
    WidgetSettings {
        enabled: speech.show_recording_widget,
        monochromatic: config.settings().appearance.icons == Skin::Mono,
        at_cursor: speech.recording_widget_at_cursor,
        anchor: speech.recording_widget_anchor,
    }
}

#[cfg(target_os = "linux")]
async fn show_in_shell(
    app: &AppHandle,
    settings: WidgetSettings,
    size: (f64, f64),
) -> Result<(crate::services::gnome_shell::ShellProxy<'static>, Placement, f64), String> {
    let proxy = crate::services::gnome_shell::proxy()
        .await
        .map_err(|e| e.to_string())?;
    let stored = read_position(app, SHELL_POSITION_KEY).await;
    let (x, y, has_position) = if settings.at_cursor {
        let (px, py, work_x, work_y, work_width, work_height) =
            proxy.get_pointer().await.map_err(|e| e.to_string())?;
        let (x, y) = anchored_origin(
            (px as f64, py as f64),
            size,
            settings.anchor,
            (work_x as f64, work_y as f64, work_width as f64, work_height as f64),
        );
        (x.round() as i32, y.round() as i32, true)
    } else {
        let (x, y) = stored.unwrap_or((0, 0));
        (x, y, stored.is_some())
    };
    let (x, y, work_x, work_y, work_width, work_height, scale) = proxy
        .show_recording_widget(x, y, has_position, settings.monochromatic)
        .await
        .map_err(|e| e.to_string())?;
    let placement = Placement {
        origin: (x as f64, y as f64),
        work_area: (work_x as f64, work_y as f64, work_width as f64, work_height as f64),
    };
    let source = if settings.at_cursor {
        "cursor"
    } else if has_position && stored == Some((placement.origin.0 as i32, placement.origin.1 as i32)) {
        "stored position"
    } else {
        "default"
    };
    log_placement(source, settings.anchor, placement.origin, size, placement.work_area);
    Ok((proxy, placement, scale))
}

async fn work_areas(app: &AppHandle) -> Result<Vec<Rect>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    Ok(monitors
        .iter()
        .map(|m| {
            let work = m.work_area();
            (work.position.x, work.position.y, work.size.width, work.size.height)
        })
        .collect())
}

impl RecordingWidget {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                transport: None,
                session: 0,
                state: WidgetState::Recording,
                generation: 0,
                elapsed_ms: 0,
                monochromatic: true,
                update_failed: false,
                placement: None,
                item_rows: 0,
                scale: 1.0,
                #[cfg(target_os = "linux")]
                shell_scale: 1.0,
                preview_sizes: Vec::new(),
                grows_down: false,
                overlay: None,
                overlay_seq: 0,
                #[cfg(target_os = "linux")]
                shell: None,
            }),
            move_seq: AtomicU64::new(0),
            overlay_visible: AtomicBool::new(false),
            programmatic_origin: std::sync::Mutex::new(None),
            thumbnails: super::pill_previews::ThumbnailCache::default(),
            pixel_sizes: super::pill_previews::PixelSizeCache::default(),
        }
    }

    pub async fn is_shown(&self) -> bool {
        self.inner.lock().await.transport.is_some()
    }

    pub fn overlay_visible(&self) -> bool {
        self.overlay_visible.load(Ordering::Relaxed)
    }

    pub async fn show(&self, app: &AppHandle, session: u64) -> bool {
        let settings = widget_settings(app).await;
        let kind = current_session_kind();
        let transport = choose_transport(settings.enabled, kind);
        #[cfg(target_os = "linux")]
        if settings.enabled && kind == SessionKind::OtherWayland {
            log::warn!(
                "recording widget unavailable on this Wayland session without the GNOME Shell extension, using toasts"
            );
        }
        let Some(transport) = transport else {
            return false;
        };

        let context_items = current_context_items(app).await;
        let item_rows = gather_model(app, (0, 0), &context_items).await.item_rows;
        warm_image_caches(app, &context_items, transport == Transport::Window).await;
        let mut inner = self.inner.lock().await;
        inner.generation += 1;
        inner.session = session;
        inner.item_rows = item_rows;
        inner.scale = 1.0;
        #[cfg(target_os = "linux")]
        {
            inner.shell_scale = 1.0;
        }
        inner.grows_down = grows_down(settings);
        inner.state = WidgetState::Recording;
        inner.elapsed_ms = 0;
        inner.monochromatic = settings.monochromatic;
        inner.update_failed = false;
        inner.placement = None;
        if inner.overlay.take().is_some() {
            conceal_overlay(app);
        }
        inner.transport = None;
        #[cfg(target_os = "linux")]
        {
            inner.shell = None;
        }

        let shown = match transport {
            #[cfg(target_os = "linux")]
            Transport::Shell => show_in_shell(app, settings, pill_size(item_rows)).await.map(|(proxy, placement, scale)| {
                inner.shell = Some(proxy);
                inner.placement = Some(placement);
                inner.shell_scale = scale;
            }),
            Transport::Window => self.place_window(app, settings, item_rows).await.map(|(placement, scale)| {
                inner.placement = Some(placement);
                inner.scale = scale;
            }),
        };
        if let Err(e) = shown {
            log::warn!("showing the recording widget failed, using toasts: {e}");
            return false;
        }

        log::debug!(
            "recording widget: show session={session} transport={transport:?} monochromatic={}",
            settings.monochromatic
        );
        inner.transport = Some(transport);
        if transport == Transport::Window {
            push(app, &mut inner, 0.0).await;
        }
        push_current_model(app, &mut inner).await;
        if transport == Transport::Window {
            if let Err(e) = reveal_window(app) {
                log::warn!("showing the recording widget failed, using toasts: {e}");
                inner.transport = None;
                return false;
            }
        }
        spawn_level_task(app.clone(), session, inner.generation);
        true
    }

    pub async fn pause(&self, app: &AppHandle) {
        self.transition(app, "pause", WidgetState::Paused).await;
    }

    pub async fn resume(&self, app: &AppHandle) {
        self.transition(app, "resume", WidgetState::Recording).await;
    }

    pub async fn processing(&self, app: &AppHandle) {
        self.transition(app, "processing", WidgetState::Processing).await;
    }

    pub async fn retrying_if_current(
        &self,
        app: &AppHandle,
        generation: u64,
        attempt: u32,
        max_attempts: u32,
        delay_ms: u32,
    ) -> bool {
        let state = WidgetState::Retrying { attempt, max_attempts, delay_ms };
        self.transition_if_current(app, "retrying", generation, state).await
    }

    pub async fn processing_if_current(&self, app: &AppHandle, generation: u64) {
        self.transition_if_current(app, "processing", generation, WidgetState::Processing)
            .await;
    }

    pub async fn shown_generation(&self) -> Option<(Transport, u64)> {
        let inner = self.inner.lock().await;
        inner.transport.map(|transport| (transport, inner.generation))
    }

    pub async fn generation(&self) -> u64 {
        self.inner.lock().await.generation
    }

    pub async fn done(&self, app: &AppHandle) {
        self.done_if(app, None).await;
    }

    pub async fn done_if_current(&self, app: &AppHandle, generation: u64) {
        self.done_if(app, Some(generation)).await;
    }

    pub async fn close_if_current(&self, app: &AppHandle, generation: u64) {
        let mut inner = self.inner.lock().await;
        if inner.is_current(generation) {
            self.close_locked(app, &mut inner).await;
        } else {
            log::debug!("recording widget: close skipped, stale generation={generation}");
        }
    }

    async fn done_if(&self, app: &AppHandle, only_generation: Option<u64>) {
        let generation = {
            let mut inner = self.inner.lock().await;
            let skip = match only_generation {
                Some(g) => !inner.is_current(g),
                None => inner.transport.is_none(),
            };
            if skip {
                return;
            }
            log::debug!("recording widget: done session={}", inner.session);
            inner.state = WidgetState::Done;
            push(app, &mut inner, 0.0).await;
            inner.generation
        };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(DONE_VISIBLE).await;
            let widget = app.state::<RecordingWidget>();
            let mut inner = widget.inner.lock().await;
            if inner.generation == generation {
                widget.close_locked(&app, &mut inner).await;
            }
        });
    }

    pub async fn close(&self, app: &AppHandle) {
        let mut inner = self.inner.lock().await;
        self.close_locked(app, &mut inner).await;
    }

    async fn transition(&self, app: &AppHandle, name: &str, state: WidgetState) {
        let mut inner = self.inner.lock().await;
        if inner.transport.is_none() {
            return;
        }
        apply_transition(app, &mut inner, name, state).await;
    }

    async fn transition_if_current(
        &self,
        app: &AppHandle,
        name: &str,
        generation: u64,
        state: WidgetState,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        if !inner.is_current(generation) {
            log::debug!("recording widget: {name} skipped, stale generation={generation}");
            return false;
        }
        apply_transition(app, &mut inner, name, state).await
    }

    async fn close_locked(&self, app: &AppHandle, inner: &mut Inner) {
        let Some(transport) = inner.transport.take() else {
            return;
        };
        log::debug!("recording widget: close session={}", inner.session);
        inner.generation += 1;
        drop_overlay(app, inner);
        order_out_overlay(app);
        let result = match transport {
            #[cfg(target_os = "linux")]
            Transport::Shell => {
                clear_preview_files(app);
                match inner.shell.take() {
                    Some(proxy) => proxy.hide_recording_widget().await.map_err(|e| e.to_string()),
                    None => Ok(()),
                }
            }
            Transport::Window => match app.get_webview_window(WINDOW_LABEL) {
                Some(window) => window.hide().map_err(|e| e.to_string()),
                None => Ok(()),
            },
        };
        if let Err(e) = result {
            log::warn!("hiding the recording widget failed: {e}");
        }
    }

    async fn place_window(
        &self,
        app: &AppHandle,
        settings: WidgetSettings,
        item_rows: u32,
    ) -> Result<(Placement, f64), String> {
        let window = app
            .get_webview_window(WINDOW_LABEL)
            .ok_or("recording-widget window not found")?;
        let logical_size = pill_size(item_rows);
        let (origin, monitor, source) = if settings.at_cursor {
            let (cx, cy) = app
                .cursor_position()
                .map(|p| (p.x, p.y))
                .unwrap_or((0.0, 0.0));
            let monitor = find_monitor_at(app, cx as i32, cy as i32)?;
            ((cx, cy), monitor, "cursor")
        } else {
            let areas = work_areas(app).await?;
            let stored = read_position(app, WINDOW_POSITION_KEY)
                .await
                .filter(|&point| is_inside_any(point, &areas));
            match stored {
                Some((x, y)) => {
                    let monitor = find_monitor_at(app, x, y)?;
                    ((x as f64, y as f64), monitor, "stored position")
                }
                None => {
                    let (cx, cy) = app
                        .cursor_position()
                        .map(|p| (p.x as i32, p.y as i32))
                        .unwrap_or((0, 0));
                    let monitor = find_monitor_at(app, cx, cy)?;
                    let work = monitor.work_area();
                    let size = physical_size(logical_size, monitor.scale_factor());
                    let (x, y) = default_origin(
                        (work.position.x, work.position.y, work.size.width, work.size.height),
                        size,
                        monitor.scale_factor(),
                    );
                    ((x as f64, y as f64), monitor, "default")
                }
            }
        };
        let scale = monitor.scale_factor();
        let work_area = work_area_of(&monitor);
        let size = physical_size(logical_size, scale);
        window
            .set_size(PhysicalSize::new(size.0, size.1))
            .map_err(|e| e.to_string())?;
        let size_f = (size.0 as f64, size.1 as f64);
        let origin = if settings.at_cursor {
            anchored_origin(origin, size_f, settings.anchor, work_area)
        } else {
            clamp_origin(origin, size_f, work_area)
        };
        let origin = (origin.0.round(), origin.1.round());
        log_placement(source, settings.anchor, origin, (size.0 as f64, size.1 as f64), work_area);
        self.set_window_origin(&window, origin)?;
        Ok((Placement { origin, work_area }, scale))
    }

    fn mark_programmatic(&self, origin: (f64, f64)) {
        *self
            .programmatic_origin
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((origin.0 as i32, origin.1 as i32));
    }

    fn set_window_origin(&self, window: &tauri::WebviewWindow, origin: (f64, f64)) -> Result<(), String> {
        self.mark_programmatic(origin);
        window
            .set_position(PhysicalPosition::new(origin.0 as i32, origin.1 as i32))
            .map_err(|e| e.to_string())
    }

    pub async fn refresh_model(&self, app: &AppHandle) {
        let Some(transport) = self.inner.lock().await.transport else {
            return;
        };
        let context_items = current_context_items(app).await;
        warm_image_caches(app, &context_items, transport == Transport::Window).await;
        let mut inner = self.inner.lock().await;
        if inner.transport.is_none() {
            return;
        }
        let model = gather_model(app, placement_origin(&inner), &context_items).await;
        if rows_resize(inner.item_rows, model.item_rows) {
            self.resize_locked(app, &mut inner, model.item_rows);
        }
        push_model(app, &mut inner, model, &context_items).await;
    }

    fn resize_locked(&self, app: &AppHandle, inner: &mut Inner, item_rows: u32) {
        drop_overlay(app, inner);
        let scale = inner.scale;
        let (width, old_height) = pill_size(inner.item_rows);
        let new_height = pill_size(item_rows).1;
        let old_rows = std::mem::replace(&mut inner.item_rows, item_rows);
        let Some(placement) = inner.placement.as_mut() else {
            return;
        };
        placement.origin = regrow_origin(
            placement.origin,
            old_height * scale,
            new_height * scale,
            width * scale,
            inner.grows_down,
            placement.work_area,
        );
        log::debug!(
            "recording widget: resized rows {old_rows} -> {item_rows} origin={:?}",
            placement.origin
        );
        if inner.transport != Some(Transport::Window) {
            return;
        }
        let origin = placement.origin;
        let size = physical_size((width, new_height), scale);
        self.apply_frame(app, origin, (size.0 as f64, size.1 as f64), scale);
    }

    fn record_move(&self, app: &AppHandle, position: (i32, i32)) {
        let programmatic = *self
            .programmatic_origin
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if programmatic == Some(position) {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let widget = app.state::<RecordingWidget>();
            if let Ok(monitor) = find_monitor_at(&app, position.0, position.1) {
                let work_area = work_area_of(&monitor);
                let mut inner = widget.inner.lock().await;
                inner.scale = monitor.scale_factor();
                if drop_overlay(&app, &mut inner) {
                    log::debug!("recording widget: overlay closed by drag");
                }
                let origin = (position.0 as f64, position.1 as f64);
                inner.placement = Some(Placement { origin, work_area });
            }
            widget.store_drag_debounced(&app, WINDOW_POSITION_KEY, position).await;
        });
    }

    fn apply_frame(&self, app: &AppHandle, origin: (f64, f64), size: (f64, f64), scale: f64) {
        let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
            log::warn!("recording widget frame skipped: window not found");
            return;
        };
        #[cfg(target_os = "macos")]
        let applied = {
            self.mark_programmatic(origin);
            let primary_height_points = app
                .primary_monitor()
                .ok()
                .flatten()
                .map_or(0.0, |m| m.size().height as f64 / m.scale_factor());
            crate::services::macos_panel::set_frame_physical(&window, origin, size, scale, primary_height_points)
        };
        #[cfg(not(target_os = "macos"))]
        let applied = {
            let _ = scale;
            window
                .set_size(PhysicalSize::new(size.0 as u32, size.1 as u32))
                .map_err(|e| e.to_string())
                .and_then(|_| self.set_window_origin(&window, origin))
        };
        if let Err(e) = applied {
            log::warn!("setting the recording widget frame failed: {e}");
        }
    }

    pub async fn open_dropdown(&self, app: &AppHandle, rows: u32, separator: bool) -> Result<bool, String> {
        let height = dropdown_height(rows, separator);
        self.open_overlay(app, OverlayKind::Dropdown, height, OverlayContent::default()).await
    }

    pub async fn open_preview(
        &self,
        app: &AppHandle,
        index: Option<u32>,
        text: Option<String>,
    ) -> Result<bool, String> {
        let content = OverlayContent { index, text, center_x: None };
        self.open_overlay(app, OverlayKind::Preview, PREVIEW_FRAME_HEIGHT, content).await
    }

    pub async fn open_tooltip(
        &self,
        app: &AppHandle,
        text: Option<String>,
        center_x: Option<f64>,
    ) -> Result<bool, String> {
        let content = OverlayContent { index: None, text, center_x };
        self.open_overlay(app, OverlayKind::Tooltip, TOOLTIP_FRAME_HEIGHT, content).await
    }

    pub async fn close_dropdown(&self, app: &AppHandle) {
        self.close_overlay(app, OverlayKind::Dropdown).await;
    }

    pub async fn close_preview(&self, app: &AppHandle) {
        self.close_overlay(app, OverlayKind::Preview).await;
    }

    pub async fn close_tooltip(&self, app: &AppHandle) {
        self.close_overlay(app, OverlayKind::Tooltip).await;
    }

    async fn open_overlay(
        &self,
        app: &AppHandle,
        kind: OverlayKind,
        logical_height: f64,
        content: OverlayContent,
    ) -> Result<bool, String> {
        let mut inner = self.inner.lock().await;
        let plan = inner.begin_overlay(kind, logical_height)?;
        log::debug!(
            "recording widget: {kind:?} open up={} seq={} frame={:?} step={:?}",
            plan.up,
            plan.seq,
            plan.frame,
            plan.step
        );
        if let OverlayStep::Reframe { hide } = plan.step {
            if hide && !cfg!(target_os = "macos") {
                conceal_overlay(app);
            }
            apply_overlay_frame(app, plan.frame.0, plan.frame.1, inner.scale);
        }
        let descriptor = OverlayDescriptor {
            seq: plan.seq,
            kind: kind.as_str(),
            up: plan.up,
            index: content.index,
            text: content.text,
            center_x: content.center_x,
        };
        if let Err(e) = app.emit_to(OVERLAY_WINDOW_LABEL, OVERLAY_EVENT, descriptor) {
            log::warn!("sending the overlay content failed: {e}");
        }
        Ok(plan.up)
    }

    pub async fn overlay_rendered(&self, app: &AppHandle, seq: u64) {
        let mut inner = self.inner.lock().await;
        if !inner.finish_overlay(seq) {
            log::debug!("recording widget: stale overlay ack seq={seq}");
            return;
        }
        log::debug!("recording widget: overlay ack seq={seq}");
        match reveal_overlay_window(app) {
            Ok(()) => {
                self.overlay_visible.store(true, Ordering::Relaxed);
                log::debug!("recording widget: overlay reveal seq={seq}");
            }
            Err(e) => log::warn!("showing the recording widget overlay failed: {e}"),
        }
    }

    async fn close_overlay(&self, app: &AppHandle, kind: OverlayKind) {
        let mut inner = self.inner.lock().await;
        if inner.end_overlay(kind).is_none() {
            return;
        }
        log::debug!("recording widget: {kind:?} close");
        conceal_overlay(app);
        if kind == OverlayKind::Dropdown {
            let _ = app.emit_to(WINDOW_LABEL, DROPDOWN_CLOSED_EVENT, ());
        }
    }

    #[cfg(target_os = "macos")]
    pub async fn hide_for_capture(&self, app: &AppHandle, generation: u64) -> bool {
        let mut inner = self.inner.lock().await;
        if !inner.is_current(generation) || inner.transport != Some(Transport::Window) {
            return false;
        }
        drop_overlay(app, &mut inner);
        order_out_overlay(app);
        match app.get_webview_window(WINDOW_LABEL).map(|window| window.hide()) {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                log::warn!("hiding the recording widget for a capture failed: {e}");
                false
            }
            None => {
                log::warn!("hiding the recording widget for a capture failed: window not found");
                false
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub async fn restore_after_capture(&self, app: &AppHandle, generation: u64) {
        let inner = self.inner.lock().await;
        if !inner.is_current(generation) || inner.transport != Some(Transport::Window) {
            log::debug!("recording widget: restore after capture skipped, stale generation={generation}");
            return;
        }
        if let Err(e) = reveal_window(app) {
            log::warn!("showing the recording widget after a capture failed: {e}");
        }
    }

    #[cfg(target_os = "linux")]
    pub fn shell_moved(&self, app: &AppHandle, placement: Placement, scale: f64) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let widget = app.state::<RecordingWidget>();
            {
                let mut inner = widget.inner.lock().await;
                inner.placement = Some(placement);
                inner.shell_scale = scale;
            }
            let position = (placement.origin.0 as i32, placement.origin.1 as i32);
            widget.store_drag_debounced(&app, SHELL_POSITION_KEY, position).await;
        });
    }

    async fn store_drag_debounced(&self, app: &AppHandle, key: &str, position: (i32, i32)) {
        let seq = self.move_seq.fetch_add(1, Ordering::SeqCst) + 1;
        tokio::time::sleep(MOVE_DEBOUNCE).await;
        if self.move_seq.load(Ordering::SeqCst) == seq
            && should_store_drag(widget_settings(app).await.at_cursor)
        {
            store_position(app, key, position.0, position.1).await;
        }
    }
}

fn note_update_failure(inner: &mut Inner, result: Result<(), String>) {
    if let Err(e) = result {
        if !inner.update_failed {
            inner.update_failed = true;
            log::warn!("updating the recording widget failed: {e}");
        }
    }
}

fn reveal_window(app: &AppHandle) -> Result<(), String> {
    app.get_webview_window(WINDOW_LABEL)
        .ok_or("recording-widget window not found")?
        .show()
        .map_err(|e| e.to_string())
}

fn overlay_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    app.get_webview_window(OVERLAY_WINDOW_LABEL)
        .ok_or_else(|| "recording-widget-overlay window not found".to_string())
}

fn set_overlay_visible(app: &AppHandle, visible: bool) {
    app.state::<RecordingWidget>().overlay_visible.store(visible, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn reveal_overlay_window(app: &AppHandle) -> Result<(), String> {
    crate::services::macos_panel::reveal_overlay_panel(&overlay_window(app)?)
}

#[cfg(not(target_os = "macos"))]
fn reveal_overlay_window(app: &AppHandle) -> Result<(), String> {
    overlay_window(app)?.show().map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn conceal_overlay_window(app: &AppHandle) -> Result<(), String> {
    crate::services::macos_panel::conceal_overlay_panel(&overlay_window(app)?)
}

#[cfg(not(target_os = "macos"))]
fn conceal_overlay_window(app: &AppHandle) -> Result<(), String> {
    overlay_window(app)?.hide().map_err(|e| e.to_string())
}

fn conceal_overlay(app: &AppHandle) {
    set_overlay_visible(app, false);
    log::debug!("recording widget: overlay conceal");
    if let Err(e) = conceal_overlay_window(app) {
        log::warn!("hiding the recording widget overlay failed: {e}");
    }
}

#[cfg(target_os = "macos")]
fn order_out_overlay(app: &AppHandle) {
    conceal_overlay(app);
    if let Err(e) = overlay_window(app).and_then(|window| window.hide().map_err(|e| e.to_string())) {
        log::warn!("ordering out the recording widget overlay failed: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
fn order_out_overlay(_app: &AppHandle) {}

fn drop_overlay(app: &AppHandle, inner: &mut Inner) -> bool {
    if inner.overlay.take().is_none() {
        return false;
    }
    conceal_overlay(app);
    let _ = app.emit_to(WINDOW_LABEL, DROPDOWN_CLOSED_EVENT, ());
    true
}

fn apply_overlay_frame(app: &AppHandle, origin: (f64, f64), size: (f64, f64), scale: f64) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        log::warn!("recording widget overlay frame skipped: window not found");
        return;
    };
    #[cfg(target_os = "macos")]
    let applied = {
        let primary_height_points = app
            .primary_monitor()
            .ok()
            .flatten()
            .map_or(0.0, |m| m.size().height as f64 / m.scale_factor());
        set_overlay_visible(app, false);
        crate::services::macos_panel::prepare_overlay_frame(&window, origin, size, scale, primary_height_points)
    };
    #[cfg(not(target_os = "macos"))]
    let applied = {
        let _ = scale;
        window
            .set_size(PhysicalSize::new(size.0 as u32, size.1 as u32))
            .and_then(|_| window.set_position(PhysicalPosition::new(origin.0 as i32, origin.1 as i32)))
            .map_err(|e| e.to_string())
    };
    if let Err(e) = applied {
        log::warn!("setting the recording widget overlay frame failed: {e}");
    }
}

async fn apply_transition(
    app: &AppHandle,
    inner: &mut Inner,
    name: &str,
    state: WidgetState,
) -> bool {
    if !transition_allowed(inner.state, state) {
        log::debug!("recording widget: {name} refused session={}", inner.session);
        return false;
    }
    log::debug!("recording widget: {name} session={}", inner.session);
    inner.state = state;
    push(app, inner, 0.0).await;
    true
}

async fn push(app: &AppHandle, inner: &mut Inner, level: f64) {
    let Some(transport) = inner.transport else {
        return;
    };
    let result = match transport {
        #[cfg(target_os = "linux")]
        Transport::Shell => match &inner.shell {
            Some(proxy) => match inner.state {
                WidgetState::Retrying { attempt, max_attempts, delay_ms } => {
                    proxy.update_recording_widget_retry(attempt, max_attempts, delay_ms).await
                }
                state => {
                    proxy
                        .update_recording_widget(state.as_str(), level, inner.elapsed_ms)
                        .await
                }
            }
            .map_err(|e| e.to_string()),
            None => Ok(()),
        },
        Transport::Window => app
            .emit_to(
                WINDOW_LABEL,
                STATE_EVENT,
                widget_payload(inner.state, level, inner.elapsed_ms, inner.monochromatic),
            )
            .map_err(|e| e.to_string()),
    };
    note_update_failure(inner, result);
}

async fn push_current_model(app: &AppHandle, inner: &mut Inner) {
    let context_items = current_context_items(app).await;
    let model = gather_model(app, placement_origin(inner), &context_items).await;
    push_model(app, inner, model, &context_items).await;
}

#[cfg(target_os = "linux")]
fn preview_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_cache_dir()
        .map(|dir| dir.join(super::pill_previews::DIR_NAME))
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn clear_preview_files(app: &AppHandle) {
    match preview_dir(app) {
        Ok(dir) => {
            tauri::async_runtime::spawn_blocking(move || super::pill_previews::clear_previews(&dir));
        }
        Err(e) => log::warn!("resolving the pill preview directory failed: {e}"),
    }
}

#[cfg(target_os = "linux")]
async fn sync_preview_files(app: &AppHandle, context_items: &[ContextItem]) -> Vec<String> {
    let images: Vec<(String, String)> = context_items
        .iter()
        .filter_map(|item| match item {
            ContextItem::Image { data, media_type, .. } => Some((data.clone(), media_type.clone())),
            ContextItem::Text { .. } => None,
        })
        .collect();
    let failed = vec![String::new(); images.len()];
    let dir = match preview_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!("resolving the pill preview directory failed: {e}");
            return failed;
        }
    };
    let synced = {
        let dir = dir.clone();
        tauri::async_runtime::spawn_blocking(move || super::pill_previews::sync_previews(&dir, &images)).await
    };
    match synced {
        Ok(Ok(paths)) => {
            log::debug!("recording widget: synced {} preview files in {}", paths.len(), dir.display());
            paths.iter().map(|path| path.to_string_lossy().into_owned()).collect()
        }
        Ok(Err(e)) => {
            log::warn!("writing the pill previews in {} failed: {e}", dir.display());
            failed
        }
        Err(e) => {
            log::warn!("the pill preview task for {} failed: {e}", dir.display());
            failed
        }
    }
}

async fn push_model(app: &AppHandle, inner: &mut Inner, mut model: PillModel, context_items: &[ContextItem]) {
    let Some(transport) = inner.transport else {
        return;
    };
    if let Some(placement) = inner.placement {
        model.x = placement.origin.0.round() as i32;
        model.y = placement.origin.1.round() as i32;
    }
    attach_preview_sizes(app, inner, transport, &mut model.items, context_items);
    let result = match transport {
        #[cfg(target_os = "linux")]
        Transport::Shell => match &inner.shell {
            Some(proxy) => {
                let paths = sync_preview_files(app, context_items).await;
                let items = shell_items(model.items, context_items, paths);
                proxy
                    .update_recording_widget_model(
                        model.targets,
                        &model.selected_id,
                        &model.selected_label,
                        model.target_locked,
                        items,
                        model.item_rows,
                        model.context_version,
                        model.x,
                        model.y,
                        &model.stop_chat_destination,
                    )
                    .await
                    .map_err(|e| e.to_string())
            }
            None => Ok(()),
        },
        Transport::Window => {
            attach_thumbnails(app, &mut model.items, context_items);
            app.emit_to(WINDOW_LABEL, MODEL_EVENT, &model)
                .map_err(|e| e.to_string())
        }
    };
    note_update_failure(inner, result);
}

fn spawn_level_task(app: AppHandle, session: u64, generation: u64) {
    tauri::async_runtime::spawn(async move {
        let Some((speech, buffer)) = start_session_task(&app, session).await else {
            return;
        };
        let widget = app.state::<RecordingWidget>();
        let mut cursor = 0usize;
        let mut smoothed = 0.0;

        loop {
            tokio::time::sleep(TICK).await;

            let elapsed = {
                let Some(s) = lock_active_session(&speech, session).await else {
                    return;
                };
                s.elapsed().map_or(0, elapsed_ms)
            };
            let level = level_from_rms(tail_rms(&buffer, &mut cursor));

            let mut inner = widget.inner.lock().await;
            if inner.generation != generation || !inner.state.is_live() {
                return;
            }
            inner.elapsed_ms = elapsed;
            smoothed = if inner.state == WidgetState::Paused {
                0.0
            } else {
                smooth_level(smoothed, level)
            };
            push(&app, &mut inner, smoothed).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inner_with(transport: Option<Transport>, generation: u64, state: WidgetState) -> Inner {
        Inner {
            transport,
            session: 1,
            state,
            generation,
            elapsed_ms: 0,
            monochromatic: true,
            update_failed: false,
            placement: None,
            item_rows: 0,
            scale: 1.0,
            #[cfg(target_os = "linux")]
            shell_scale: 1.0,
            preview_sizes: Vec::new(),
            grows_down: false,
            overlay: None,
            overlay_seq: 0,
            #[cfg(target_os = "linux")]
            shell: None,
        }
    }

    #[test]
    fn a_stale_generation_is_not_current_and_leaves_a_newer_pill_unchanged() {
        let newer = inner_with(Some(Transport::Window), 5, WidgetState::Recording);
        assert!(!newer.is_current(4));
        assert!(newer.is_current(5));
        assert_eq!(newer.state, WidgetState::Recording);
        assert_eq!(newer.generation, 5);
    }

    #[test]
    fn retrying_payload_carries_attempt_max_and_delay() {
        let retrying = WidgetState::Retrying { attempt: 2, max_attempts: 3, delay_ms: 6000 };
        let value = serde_json::to_value(widget_payload(retrying, 0.0, 0, true)).unwrap();
        assert_eq!(value["state"], "retrying");
        assert_eq!(value["attempt"], 2);
        assert_eq!(value["maxAttempts"], 3);
        assert_eq!(value["delayMs"], 6000);

        let value =
            serde_json::to_value(widget_payload(WidgetState::Processing, 0.0, 0, true)).unwrap();
        for key in ["attempt", "maxAttempts", "delayMs"] {
            assert!(value.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn retrying_transitions() {
        let retrying = WidgetState::Retrying { attempt: 2, max_attempts: 3, delay_ms: 6000 };
        assert!(transition_allowed(WidgetState::Processing, retrying));
        assert!(transition_allowed(retrying, retrying));
        assert!(transition_allowed(retrying, WidgetState::Processing));
        assert!(transition_allowed(retrying, WidgetState::Done));
        assert!(!transition_allowed(WidgetState::Recording, retrying));
        assert!(!transition_allowed(WidgetState::Paused, retrying));
        assert!(!transition_allowed(WidgetState::Done, retrying));
    }

    #[test]
    fn a_stale_generation_does_not_reach_a_newer_pill_through_the_retrying_path() {
        let newer = inner_with(Some(Transport::Window), 5, WidgetState::Processing);
        assert!(!newer.is_current(4));
        assert_eq!(newer.state, WidgetState::Processing);
    }

    #[test]
    fn a_closed_pill_is_not_current() {
        let closed = inner_with(None, 5, WidgetState::Processing);
        assert!(!closed.is_current(5));
    }

    fn skill(name: &str, display_name: &str, offered_in: Option<Vec<OfferedIn>>) -> Skill {
        Skill {
            name: name.into(),
            display_name: display_name.into(),
            description: None,
            model: None,
            parameters: None,
            offered_in,
            body: String::new(),
            file_path: Default::default(),
            skill_version_id: None,
        }
    }

    fn text() -> ContextItem {
        numbered_text(1)
    }

    fn image() -> ContextItem {
        numbered_image(1)
    }

    fn numbered_text(number: u32) -> ContextItem {
        ContextItem::Text { number, content: "t".into() }
    }

    fn numbered_image(number: u32) -> ContextItem {
        ContextItem::Image { number, data: "d".into(), media_type: "image/png".into() }
    }

    #[test]
    fn item_rows_add_their_height_to_the_pill() {
        assert_eq!(pill_size(0), (264.0, 72.0));
        assert_eq!(pill_size(1), (264.0, 106.0));
        assert_eq!(pill_size(2), (264.0, 134.0));
        assert_eq!(pill_size(3), (264.0, 162.0));
    }

    #[test]
    fn images_share_rows_of_three_and_each_text_takes_a_row_up_to_three_rows() {
        assert_eq!(item_rows(0, 0), 0);
        assert_eq!(pill_size(item_rows(0, 0)), (264.0, 72.0));
        for images in 1..=3 {
            assert_eq!(item_rows(images, 0), 1);
        }
        assert_eq!(pill_size(item_rows(1, 0)), (264.0, 106.0));
        assert_eq!(item_rows(4, 0), 2);
        assert_eq!(item_rows(2, 1), 2);
        assert_eq!(item_rows(1, 2), 3);
        assert_eq!(item_rows(3, 3), 3);
    }

    #[test]
    fn build_model_sizes_the_pill_by_the_image_first_rule() {
        let items = [
            numbered_text(1),
            numbered_image(1),
            numbered_text(2),
            numbered_image(2),
            numbered_image(3),
        ];
        let model = build_model(vec![], None, false, &items, 0, (0, 0), "");
        assert_eq!(model.item_rows, 3);
        assert_eq!(model.items.len(), 5);
    }

    #[test]
    fn only_a_row_change_resizes() {
        assert!(!rows_resize(2, 2));
        assert!(rows_resize(2, 1));
    }

    #[test]
    fn a_long_text_preview_is_cut_on_chars() {
        let preview = text_preview(&"ą".repeat(400));
        assert_eq!(preview, format!("{}…", "ą".repeat(300)));
        assert_eq!(text_preview("a\nb"), "a\nb");
    }

    #[test]
    fn an_image_item_has_no_preview() {
        assert_eq!(pill_items(&[image()])[0].preview, "");
    }

    #[test]
    fn regrow_keeps_the_bottom_edge() {
        assert_eq!(
            regrow_origin((100.0, 500.0), 80.0, 112.0, 240.0, false, (0.0, 0.0, 1920.0, 1040.0)),
            (100.0, 468.0)
        );
    }

    #[test]
    fn regrow_from_one_to_three_rows_moves_up_by_the_difference() {
        assert_eq!(
            regrow_origin((100.0, 500.0), 112.0, 164.0, 240.0, false, (0.0, 0.0, 1920.0, 1040.0)),
            (100.0, 448.0)
        );
    }

    #[test]
    fn regrow_is_clamped_to_the_work_area_top() {
        assert_eq!(
            regrow_origin((100.0, 10.0), 80.0, 112.0, 240.0, false, (0.0, 0.0, 1920.0, 1040.0)),
            (100.0, 0.0)
        );
    }

    #[test]
    fn regrow_growing_down_keeps_the_top_edge() {
        let area = (0.0, 0.0, 1920.0, 1040.0);
        assert_eq!(regrow_origin((100.0, 500.0), 72.0, 106.0, 264.0, true, area), (100.0, 500.0));
        assert_eq!(regrow_origin((100.0, 1000.0), 72.0, 106.0, 264.0, true, area), (100.0, 934.0));
        assert_eq!(regrow_origin((100.0, 500.0), 72.0, 106.0, 264.0, false, area), (100.0, 466.0));
    }

    #[test]
    fn anchored_origin_places_the_pill_by_anchor() {
        let area = (0.0, 0.0, 1920.0, 1040.0);
        let at = |anchor, cursor| anchored_origin(cursor, (264.0, 72.0), anchor, area);
        assert_eq!(at(PillAnchor::AboveRight, (500.0, 500.0)), (500.0, 428.0));
        assert_eq!(at(PillAnchor::AboveCenter, (500.0, 500.0)), (368.0, 428.0));
        assert_eq!(at(PillAnchor::AboveLeft, (500.0, 500.0)), (236.0, 428.0));
        assert_eq!(at(PillAnchor::BelowRight, (500.0, 500.0)), (500.0, 500.0));
        assert_eq!(at(PillAnchor::BelowCenter, (500.0, 500.0)), (368.0, 500.0));
        assert_eq!(at(PillAnchor::BelowLeft, (500.0, 500.0)), (236.0, 500.0));
        assert_eq!(at(PillAnchor::AboveRight, (500.0, 30.0)), (500.0, 0.0));
        assert_eq!(at(PillAnchor::BelowCenter, (500.0, 1000.0)), (368.0, 968.0));
        assert_eq!(at(PillAnchor::AboveLeft, (100.0, 500.0)), (0.0, 428.0));
        assert_eq!(at(PillAnchor::BelowRight, (1800.0, 500.0)), (1656.0, 500.0));
    }

    const WORK_AREA: WorkArea = (0.0, 0.0, 3840.0, 2160.0);

    #[test]
    fn a_pill_in_the_upper_half_opens_the_dropdown_down() {
        assert!(!dropdown_opens_up(900.0, 160.0, WORK_AREA));
    }

    #[test]
    fn a_pill_in_the_lower_half_opens_the_dropdown_up() {
        assert!(dropdown_opens_up(1800.0, 160.0, WORK_AREA));
    }

    fn overlay(kind: OverlayKind) -> Overlay {
        Overlay { kind, up: false, height: 10.0, seq: 0, shown: false }
    }

    #[test]
    fn opening_a_preview_replaces_the_dropdown_and_reports_it_closed() {
        let current = Some(overlay(OverlayKind::Dropdown));
        let request = OverlayRequest::Open(overlay(OverlayKind::Preview));
        assert_eq!(
            next_overlay(current, request),
            (Some(overlay(OverlayKind::Preview)), Some(OverlayKind::Dropdown))
        );
    }

    #[test]
    fn opening_a_dropdown_replaces_the_preview_and_reports_it_closed() {
        let current = Some(overlay(OverlayKind::Preview));
        let request = OverlayRequest::Open(overlay(OverlayKind::Dropdown));
        assert_eq!(
            next_overlay(current, request),
            (Some(overlay(OverlayKind::Dropdown)), Some(OverlayKind::Preview))
        );
    }

    #[test]
    fn closing_a_preview_leaves_an_open_dropdown_unchanged() {
        let current = Some(overlay(OverlayKind::Dropdown));
        assert_eq!(
            next_overlay(current, OverlayRequest::Close(OverlayKind::Preview)),
            (current, None)
        );
    }

    #[test]
    fn opening_a_tooltip_replaces_the_preview_and_reports_it_closed() {
        let current = Some(overlay(OverlayKind::Preview));
        let request = OverlayRequest::Open(overlay(OverlayKind::Tooltip));
        assert_eq!(
            next_overlay(current, request),
            (Some(overlay(OverlayKind::Tooltip)), Some(OverlayKind::Preview))
        );
    }

    #[test]
    fn opening_a_preview_replaces_the_tooltip_and_reports_it_closed() {
        let current = Some(overlay(OverlayKind::Tooltip));
        let request = OverlayRequest::Open(overlay(OverlayKind::Preview));
        assert_eq!(
            next_overlay(current, request),
            (Some(overlay(OverlayKind::Preview)), Some(OverlayKind::Tooltip))
        );
    }

    #[test]
    fn opening_a_tooltip_replaces_the_dropdown_and_reports_it_closed() {
        let current = Some(overlay(OverlayKind::Dropdown));
        let request = OverlayRequest::Open(overlay(OverlayKind::Tooltip));
        assert_eq!(
            next_overlay(current, request),
            (Some(overlay(OverlayKind::Tooltip)), Some(OverlayKind::Dropdown))
        );
    }

    #[test]
    fn opening_a_dropdown_replaces_the_tooltip_and_reports_it_closed() {
        let current = Some(overlay(OverlayKind::Tooltip));
        let request = OverlayRequest::Open(overlay(OverlayKind::Dropdown));
        assert_eq!(
            next_overlay(current, request),
            (Some(overlay(OverlayKind::Dropdown)), Some(OverlayKind::Tooltip))
        );
    }

    #[test]
    fn closing_a_tooltip_leaves_an_open_preview_unchanged() {
        let current = Some(overlay(OverlayKind::Preview));
        assert_eq!(
            next_overlay(current, OverlayRequest::Close(OverlayKind::Tooltip)),
            (current, None)
        );
    }

    const OVERLAY_WORK_AREA: WorkArea = (0.0, 0.0, 1440.0, 900.0);
    const PILL: (f64, f64) = (264.0, 72.0);

    #[test]
    fn overlay_frame_puts_the_preview_above_a_pill_in_the_lower_half() {
        assert_eq!(
            overlay_frame((100.0, 600.0), PILL, 166.0, true, OVERLAY_WORK_AREA),
            ((100.0, 434.0), (264.0, 166.0))
        );
    }

    #[test]
    fn overlay_frame_puts_the_preview_below_a_pill_in_the_upper_half() {
        assert_eq!(
            overlay_frame((100.0, 100.0), PILL, 166.0, false, OVERLAY_WORK_AREA),
            ((100.0, 172.0), (264.0, 166.0))
        );
    }

    #[test]
    fn overlay_frame_puts_the_tooltip_above_the_pill() {
        assert_eq!(
            overlay_frame((100.0, 600.0), PILL, 30.0, true, OVERLAY_WORK_AREA),
            ((100.0, 570.0), (264.0, 30.0))
        );
    }

    #[test]
    fn overlay_frame_puts_the_dropdown_against_the_pill_without_a_gap() {
        let height = dropdown_height(3, true);
        assert_eq!(
            overlay_frame((100.0, 600.0), PILL, height, true, OVERLAY_WORK_AREA),
            ((100.0, 499.0), (264.0, 101.0))
        );
        assert_eq!(
            overlay_frame((100.0, 100.0), PILL, height, false, OVERLAY_WORK_AREA),
            ((100.0, 172.0), (264.0, 101.0))
        );
    }

    #[test]
    fn overlay_frame_above_is_clamped_to_the_work_area_top() {
        assert_eq!(
            overlay_frame((100.0, 100.0), PILL, 166.0, true, (0.0, 25.0, 1440.0, 875.0)),
            ((100.0, 25.0), (264.0, 166.0))
        );
    }

    #[test]
    fn overlay_frame_below_is_clamped_to_the_work_area_bottom() {
        assert_eq!(
            overlay_frame((100.0, 800.0), PILL, 166.0, false, OVERLAY_WORK_AREA),
            ((100.0, 734.0), (264.0, 166.0))
        );
    }

    #[test]
    fn overlay_transitions_follow_seq_and_ack_and_leave_the_pill_placement_unchanged() {
        let placement = Placement { origin: (100.0, 600.0), work_area: OVERLAY_WORK_AREA };
        let mut inner = inner_with(Some(Transport::Window), 1, WidgetState::Recording);
        inner.placement = Some(placement);
        let frame = ((100.0, 434.0), (264.0, 166.0));

        let plan = inner.begin_overlay(OverlayKind::Preview, 166.0).unwrap();
        assert_eq!((plan.seq, plan.up, plan.frame), (1, true, frame));
        assert_eq!(plan.step, OverlayStep::Reframe { hide: false });
        assert!(inner.finish_overlay(1));
        assert!(!inner.finish_overlay(1));
        assert_eq!(inner.placement, Some(placement));

        let plan = inner.begin_overlay(OverlayKind::Preview, 166.0).unwrap();
        assert_eq!((plan.seq, plan.step), (2, OverlayStep::InPlace));
        assert!(inner.overlay.is_some_and(|overlay| overlay.shown));
        assert_eq!(inner.placement, Some(placement));

        let plan = inner.begin_overlay(OverlayKind::Tooltip, 30.0).unwrap();
        assert_eq!(plan.seq, 3);
        assert_eq!(plan.step, OverlayStep::Reframe { hide: true });
        assert_eq!(plan.replaced, Some(OverlayKind::Preview));
        assert!(!inner.finish_overlay(2));
        assert!(inner.finish_overlay(3));
        assert_eq!(inner.placement, Some(placement));

        assert_eq!(inner.end_overlay(OverlayKind::Preview), None);
        assert!(inner.overlay.is_some_and(|overlay| overlay.kind == OverlayKind::Tooltip));
        assert_eq!(inner.end_overlay(OverlayKind::Tooltip), Some(OverlayKind::Tooltip));
        assert_eq!(inner.overlay, None);
        assert!(!inner.finish_overlay(3));
        assert_eq!(inner.placement, Some(placement));
    }

    #[test]
    fn overlay_transitions_refuse_an_open_without_a_window_pill_or_placement() {
        let mut closed = inner_with(None, 1, WidgetState::Recording);
        closed.placement = Some(Placement { origin: (100.0, 600.0), work_area: OVERLAY_WORK_AREA });
        assert!(closed.begin_overlay(OverlayKind::Preview, 166.0).is_err());
        let mut unplaced = inner_with(Some(Transport::Window), 1, WidgetState::Recording);
        assert!(unplaced.begin_overlay(OverlayKind::Preview, 166.0).is_err());
    }

    #[test]
    fn targets_start_with_plain_dictation() {
        let targets = pill_targets(&[skill("fix", "Fix", None)], &[], ClaudeTarget::Webview);
        assert_eq!(targets[0], ("__dictation__".to_string(), "Dictation".to_string()));
        assert_eq!(targets[1], ("fix".to_string(), "Fix".to_string()));
    }

    #[test]
    fn a_context_menu_only_skill_is_not_a_target() {
        let targets = pill_targets(&[skill("fix", "Fix", Some(vec![OfferedIn::ContextMenu]))], &[], ClaudeTarget::Webview);
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn a_skill_without_offered_in_is_a_target() {
        assert_eq!(pill_targets(&[skill("fix", "Fix", None)], &[], ClaudeTarget::Webview).len(), 2);
    }

    #[test]
    fn targets_follow_the_skill_order() {
        let targets = pill_targets(&[
            skill("b", "B", Some(vec![OfferedIn::Pill])),
            skill("a", "A", None),
        ], &[], ClaudeTarget::Webview);
        let ids: Vec<&str> = targets.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["__dictation__", "b", "a"]);
    }

    fn provider(id: &str, name: &str) -> WebviewProvider {
        WebviewProvider { id: id.into(), name: name.into(), url: String::new() }
    }

    #[test]
    fn webview_targets_come_before_the_skills_and_only_claude_and_chatgpt_are_offered() {
        let targets = pill_targets(
            &[skill("fix", "Fix", None)],
            &[provider("claude", "Claude"), provider("chatgpt", "ChatGPT"), provider("mistral", "Mistral")],
            ClaudeTarget::Webview,
        );
        let ids: Vec<&str> = targets.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["__dictation__", "webview:claude", "webview:chatgpt", "fix"]);
        assert_eq!(targets[1].1, "Claude");
        assert_eq!(targets[2].1, "ChatGPT");
    }

    #[test]
    fn the_dropdown_height_includes_the_separator_only_when_there_is_one() {
        assert_eq!(dropdown_height(4, false), 120.0);
        assert_eq!(dropdown_height(4, true), 129.0);
    }

    #[test]
    fn claude_target_desktop_labels_the_claude_entry_claude_desktop() {
        let providers = [provider("claude", "Claude"), provider("chatgpt", "ChatGPT")];
        let targets = pill_targets(&[], &providers, ClaudeTarget::Desktop);
        assert_eq!(targets[1], ("webview:claude".to_string(), "Claude Desktop".to_string()));
        assert_eq!(targets[2], ("webview:chatgpt".to_string(), "ChatGPT".to_string()));
    }

    #[test]
    fn claude_target_webview_labels_the_claude_entry_with_the_provider_name() {
        let providers = [provider("claude", "Claude"), provider("chatgpt", "ChatGPT")];
        let targets = pill_targets(&[], &providers, ClaudeTarget::Webview);
        assert_eq!(targets[1], ("webview:claude".to_string(), "Claude".to_string()));
    }

    #[test]
    fn no_webview_target_without_a_supported_provider() {
        for providers in [vec![], vec![provider("mistral", "Mistral")]] {
            let targets = pill_targets(&[], &providers, ClaudeTarget::Webview);
            assert!(targets.iter().all(|(id, _)| !id.starts_with("webview:")));
        }
    }

    #[test]
    fn items_are_labelled_per_kind_in_context_order() {
        let items = pill_items(&[numbered_text(1), numbered_image(1), numbered_text(2)]);
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["#1", "#1", "#2"]);
        assert_eq!(
            items[1],
            PillItem {
                index: 1,
                kind: "image".into(),
                label: "#1".into(),
                preview: String::new(),
                thumbnail: None,
                preview_width: None,
                preview_height: None,
            }
        );
    }

    #[test]
    fn labels_keep_item_numbers_after_a_removal() {
        let items = pill_items(&[numbered_text(1), numbered_text(3)]);
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["#1", "#3"]);
        assert_eq!(items[0].index, 0);
    }

    #[test]
    fn index_is_the_context_position_for_a_mix_of_texts_and_images() {
        let items = pill_items(&[numbered_text(1), numbered_image(1), numbered_text(2), numbered_image(2)]);
        let indexes: Vec<u32> = items.iter().map(|item| item.index).collect();
        let kinds: Vec<&str> = items.iter().map(|item| item.kind.as_str()).collect();
        assert_eq!(indexes, [0, 1, 2, 3]);
        assert_eq!(kinds, ["text", "image", "text", "image"]);
    }

    #[test]
    fn a_shift_click_skill_is_selected_even_when_not_a_target() {
        let pending = PendingPrompt {
            id: "summarize".into(),
            name: "Summarize".into(),
            origin: crate::services::speech::PendingOrigin::ShiftClick,
        };
        let model = build_model(pill_targets(&[], &[], ClaudeTarget::Webview), Some(&pending), false, &[], 1, (0, 0), "");
        assert_eq!(model.selected_label, "Summarize");
        assert!(!model.targets.iter().any(|(id, _)| id == &model.selected_id));
    }

    #[test]
    fn build_model_copies_the_stop_chat_destination() {
        let model = build_model(vec![], None, false, &[], 0, (0, 0), "Claude Desktop");
        assert_eq!(model.stop_chat_destination, "Claude Desktop");
    }

    #[test]
    fn stop_chat_destination_follows_the_stop_chat_and_claude_targets() {
        assert_eq!(stop_chat_destination(StopChatTarget::Chat, ClaudeTarget::Desktop), "");
        assert_eq!(stop_chat_destination(StopChatTarget::Chat, ClaudeTarget::Webview), "");
        assert_eq!(stop_chat_destination(StopChatTarget::Claude, ClaudeTarget::Desktop), "Claude Desktop");
        assert_eq!(stop_chat_destination(StopChatTarget::Claude, ClaudeTarget::Webview), "Claude");
    }

    #[test]
    fn no_pending_selects_plain_dictation() {
        let model = build_model(pill_targets(&[], &[], ClaudeTarget::Webview), None, false, &[text()], 7, (3, 4), "");
        assert_eq!((model.selected_id.as_str(), model.selected_label.as_str()), ("__dictation__", "Dictation"));
        assert_eq!((model.context_version, model.x, model.y), (7, 3, 4));
        assert_eq!(model.items.len(), 1);
    }

    #[test]
    fn the_model_is_locked_only_after_the_take() {
        let mut speech = SpeechService::new();
        let model = |speech: &SpeechService| {
            build_model(vec![], speech.shown_target(), speech.target_locked(), &[], 0, (0, 0), "")
        };
        assert!(!model(&speech).target_locked);
        speech.take_pending_prompt();
        assert!(model(&speech).target_locked);
    }

    #[test]
    fn the_taken_skill_stays_on_the_locked_pill() {
        let mut speech = SpeechService::new();
        speech.select_target("assistant", "Assistant").unwrap();
        speech.take_pending_prompt();
        let model = build_model(vec![], speech.shown_target(), speech.target_locked(), &[], 0, (0, 0), "");
        assert_eq!((model.selected_id.as_str(), model.selected_label.as_str()), ("assistant", "Assistant"));
        assert!(model.target_locked);
        speech.set_pending_prompt("fix".into(), "Fix".into(), crate::services::speech::PendingOrigin::ShiftClick);
        assert_eq!(speech.shown_target().map(|p| p.id.as_str()), Some("assistant"));
    }

    #[test]
    fn disabled_widget_has_no_transport_in_any_session() {
        assert_eq!(choose_transport(false, SessionKind::X11OrMacos), None);
        #[cfg(target_os = "linux")]
        {
            assert_eq!(choose_transport(false, SessionKind::GnomeWayland), None);
            assert_eq!(choose_transport(false, SessionKind::OtherWayland), None);
        }
    }

    #[test]
    fn x11_or_macos_uses_the_window() {
        assert_eq!(
            choose_transport(true, SessionKind::X11OrMacos),
            Some(Transport::Window)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn other_wayland_has_no_transport() {
        assert_eq!(choose_transport(true, SessionKind::OtherWayland), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_wayland_uses_the_shell() {
        assert_eq!(
            choose_transport(true, SessionKind::GnomeWayland),
            Some(Transport::Shell)
        );
    }

    #[test]
    fn level_maps_rms_to_zero_through_one() {
        let at_dbfs = |db: f64| FULL_SCALE_RMS * 10f64.powf(db / 20.0);
        assert_eq!(level_from_rms(0.0), 0.0);
        assert_eq!(level_from_rms(-1.0), 0.0);
        assert!(level_from_rms(at_dbfs(-50.0)).abs() < 1e-9);
        assert_eq!(level_from_rms(50.0), 0.0);
        assert!((level_from_rms(at_dbfs(-30.0)) - 2.0 / 3.0).abs() < 1e-9);
        assert!((level_from_rms(1036.0) - 0.667).abs() < 1e-3);
        assert_eq!(level_from_rms(at_dbfs(-20.0) + 1e-6), 1.0);
        assert_eq!(level_from_rms(3277.0), 1.0);
        assert_eq!(level_from_rms(32768.0), 1.0);
    }

    #[test]
    fn smoothed_level_rises_fast() {
        assert!(smooth_level(0.0, 1.0) >= 0.6);
        assert!(smooth_level(0.25, 0.75) >= 0.55);
    }

    #[test]
    fn smoothed_level_falls_slowly() {
        assert!(smooth_level(1.0, 0.0) > 0.5);
        let after_eleven_ticks = (0..11).fold(1.0, |level, _| smooth_level(level, 0.0));
        assert!(after_eleven_ticks < 0.1);
        assert!(after_eleven_ticks > 0.0);
    }

    #[test]
    fn smoothed_level_holds_a_steady_target() {
        assert_eq!(smooth_level(0.4, 0.4), 0.4);
    }

    #[test]
    fn point_inside_a_work_area_is_accepted() {
        let areas = [(0, 0, 1920, 1040), (1920, 0, 2560, 1400)];
        assert!(is_inside_any((100, 100), &areas));
        assert!(is_inside_any((2000, 1000), &areas));
    }

    #[test]
    fn point_outside_every_work_area_is_rejected() {
        let areas = [(0, 0, 1920, 1040), (1920, 0, 2560, 1400)];
        assert!(!is_inside_any((100, 1040), &areas));
        assert!(!is_inside_any((-1, 0), &areas));
        assert!(!is_inside_any((5000, 100), &areas));
    }

    #[test]
    fn logical_size_converts_to_rounded_physical_size() {
        assert_eq!(physical_size((240.0, 80.0), 2.0), (480, 160));
        assert_eq!(physical_size((240.0, 44.0), 1.25), (300, 55));
    }

    #[test]
    fn pill_at_the_bottom_right_corner_is_clamped_inside_at_scale_two() {
        let size = physical_size((240.0, 80.0), 2.0);
        assert_eq!(
            clamp_origin((3800.0, 2100.0), (size.0 as f64, size.1 as f64), (0.0, 0.0, 3840.0, 2160.0)),
            (3360.0, 2000.0)
        );
    }

    #[test]
    fn a_drag_is_stored_only_when_the_widget_does_not_open_at_the_cursor() {
        assert!(!should_store_drag(true));
        assert!(should_store_drag(false));
    }

    #[test]
    fn default_origin_is_bottom_center_above_the_work_area_edge() {
        assert_eq!(default_origin((0, 0, 1920, 1040), (240, 44), 1.0), (840, 972));
        assert_eq!(default_origin((0, 0, 3840, 2080), (480, 88), 2.0), (1680, 1944));
    }

    #[test]
    fn preview_display_size_fits_the_image_into_the_preview_limits() {
        assert_eq!(preview_display_size((1920, 1080), 1.0), (264, 149));
        assert_eq!(preview_display_size((300, 700), 1.0), (69, 160));
        assert_eq!(preview_display_size((1200, 300), 1.0), (264, 66));
        assert_eq!(preview_display_size((600, 600), 1.0), (160, 160));
        assert_eq!(preview_display_size((120, 60), 1.0), (120, 60));
        assert_eq!(preview_display_size((2000, 40), 1.0), (264, 5));
        assert_eq!(preview_display_size((4000, 5), 1.0), (264, 1));
        assert_eq!(preview_display_size((200, 100), 2.0), (100, 50));
    }

    #[test]
    fn the_box_is_at_least_32_points_per_side() {
        assert_eq!(preview_box((264, 149)), (264, 149));
        assert_eq!(preview_box((264, 5)), (264, 32));
        assert_eq!(preview_box((264, 1)), (264, 32));
        assert_eq!(preview_box((120, 60)), (120, 60));
    }

    #[test]
    fn display_sizes_are_absent_for_text_and_for_an_unreadable_image() {
        let image = |number, data: &str| ContextItem::Image { number, data: data.into(), media_type: "image/png".into() };
        let items = [numbered_text(1), image(1, "readable"), image(2, "broken")];
        let sizes = preview_sizes(&items, 1.0, |data| (data == "readable").then_some((200, 100)));
        assert_eq!(sizes, [None, Some((200, 100)), None]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_shell_items_carry_the_display_sizes() {
        let context_items = [numbered_text(1), numbered_image(1), numbered_image(2)];
        let mut items = pill_items(&context_items);
        items[1].preview_width = Some(264);
        items[1].preview_height = Some(149);
        assert_eq!(
            shell_items(items, &context_items, vec!["/tmp/a.png".into(), "/tmp/b.png".into()]),
            [
                (0, "text".into(), "#1".into(), "t".into(), String::new(), 0, 0),
                (1, "image".into(), "#1".into(), String::new(), "/tmp/a.png".into(), 264, 149),
                (2, "image".into(), "#2".into(), String::new(), "/tmp/b.png".into(), 0, 0),
            ]
        );
    }

    #[test]
    fn the_window_model_serializes_the_keys_the_webview_reads() {
        let model = PillModel {
            targets: vec![("__dictation__".into(), "Dictation".into())],
            selected_id: "__dictation__".into(),
            selected_label: "Dictation".into(),
            target_locked: false,
            items: vec![
                PillItem {
                    index: 0,
                    kind: "text".into(),
                    label: "#1".into(),
                    preview: "hello".into(),
                    thumbnail: None,
                    preview_width: None,
                    preview_height: None,
                },
                PillItem {
                    index: 1,
                    kind: "image".into(),
                    label: "#1".into(),
                    preview: String::new(),
                    thumbnail: Some("data:image/png;base64,AAAA".into()),
                    preview_width: Some(264),
                    preview_height: Some(149),
                },
                PillItem {
                    index: 2,
                    kind: "image".into(),
                    label: "#2".into(),
                    preview: String::new(),
                    thumbnail: None,
                    preview_width: None,
                    preview_height: None,
                },
            ],
            item_rows: 2,
            context_version: 7,
            stop_chat_destination: "Claude".into(),
            x: 3,
            y: 4,
        };
        assert_eq!(
            serde_json::to_value(&model).unwrap(),
            serde_json::json!({
                "targets": [{ "id": "__dictation__", "label": "Dictation" }],
                "selectedId": "__dictation__",
                "selectedLabel": "Dictation",
                "targetLocked": false,
                "items": [
                    {
                        "index": 0,
                        "kind": "text",
                        "label": "#1",
                        "preview": "hello"
                    },
                    {
                        "index": 1,
                        "kind": "image",
                        "label": "#1",
                        "preview": "",
                        "thumbnail": "data:image/png;base64,AAAA",
                        "previewWidth": 264,
                        "previewHeight": 149
                    },
                    {
                        "index": 2,
                        "kind": "image",
                        "label": "#2",
                        "preview": ""
                    }
                ],
                "itemRows": 2,
                "contextVersion": 7,
                "stopChatDestination": "Claude",
            })
        );
    }
}
