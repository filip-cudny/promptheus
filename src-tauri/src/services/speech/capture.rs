use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use base64::Engine;
use tauri::{AppHandle, Manager};

use super::screenshot::{self, keep_capture};
use super::selection;
use super::widget::{RecordingWidget, Transport, WINDOW_LABEL};
use crate::commands::menu::CONTEXT_MENU_TITLE;
use crate::services::dialog::CONVERSATION_DIALOG_LABEL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOwner {
    ContextMenu,
    Chat,
}

impl WindowOwner {
    pub fn label(self) -> &'static str {
        match self {
            WindowOwner::ContextMenu => "context-menu",
            WindowOwner::Chat => CONVERSATION_DIALOG_LABEL,
        }
    }

    pub fn from_label(label: &str) -> Option<WindowOwner> {
        [WindowOwner::ContextMenu, WindowOwner::Chat]
            .into_iter()
            .find(|owner| owner.label() == label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Pill { transport: Transport, generation: u64 },
    Window(WindowOwner),
}

impl Owner {
    pub fn label(&self) -> &'static str {
        match self {
            Owner::Pill { .. } => WINDOW_LABEL,
            Owner::Window(owner) => owner.label(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Owner::Pill { .. } => "pill",
            Owner::Window(WindowOwner::ContextMenu) => "context menu",
            Owner::Window(WindowOwner::Chat) => "chat",
        }
    }
}

struct Record {
    run: u64,
    label: &'static str,
    title: String,
}

static CURRENT: Mutex<Option<Record>> = Mutex::new(None);

fn lock_current() -> std::sync::MutexGuard<'static, Option<Record>> {
    CURRENT.lock().unwrap_or_else(PoisonError::into_inner)
}

fn start(label: &'static str, title: &str) -> u64 {
    let mut current = lock_current();
    let run = selection::begin();
    *current = Some(Record { run, label, title: title.to_string() });
    run
}

fn owned_title<'a>(record: Option<&'a Record>, label: &str, is_current: impl Fn(u64) -> bool) -> Option<&'a str> {
    record
        .filter(|record| record.label == label && is_current(record.run))
        .map(|record| record.title.as_str())
}

pub fn cancel_owned_by(label: &str) {
    let title = {
        let current = lock_current();
        let title = owned_title(current.as_ref(), label, selection::is_current).map(str::to_string);
        if title.is_some() {
            selection::cancel();
        }
        title
    };
    let Some(title) = title else {
        return;
    };
    log::debug!("capture cancelled: owner window {label}");
    cancel_in_shell(title);
}

#[cfg(target_os = "linux")]
fn cancel_in_shell(title: String) {
    if !crate::services::gnome_shell::is_gnome_wayland() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let reply = async { crate::services::gnome_shell::proxy().await?.cancel_capture(&title).await }.await;
        if let Err(e) = reply {
            log::warn!("CancelCapture failed: {e}");
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn cancel_in_shell(_title: String) {}

pub fn still_valid(owner: &Owner, run_current: bool, shown_generation: Option<u64>, target_locked: bool) -> bool {
    match owner {
        Owner::Pill { generation, .. } => run_current && keep_capture(*generation, shown_generation, target_locked),
        Owner::Window(_) => run_current,
    }
}

pub async fn still_valid_now(app: &AppHandle, owner: &Owner, run: u64) -> bool {
    let run_current = selection::is_current(run);
    let (shown_generation, target_locked) = match owner {
        Owner::Pill { .. } if run_current => (
            app.state::<RecordingWidget>()
                .shown_generation()
                .await
                .map(|(_, generation)| generation),
            super::session::target_locked(app).await,
        ),
        _ => (None, false),
    };
    still_valid(owner, run_current, shown_generation, target_locked)
}

fn owner_title(app: &AppHandle, owner: &Owner) -> Option<String> {
    match owner {
        Owner::Pill { .. } => Some(String::new()),
        Owner::Window(WindowOwner::ContextMenu) => Some(CONTEXT_MENU_TITLE.to_string()),
        Owner::Window(WindowOwner::Chat) => {
            let title = app
                .get_window(CONVERSATION_DIALOG_LABEL)
                .and_then(|window| window.title().ok());
            if title.is_none() {
                log::debug!("capture skipped: chat window missing");
            }
            title
        }
    }
}

#[cfg(target_os = "linux")]
fn session_supported(owner: &Owner) -> bool {
    match owner {
        Owner::Pill { transport, .. } => matches!(transport, Transport::Shell),
        Owner::Window(_) => crate::services::gnome_shell::is_gnome_wayland(),
    }
}

#[cfg(target_os = "macos")]
fn session_supported(owner: &Owner) -> bool {
    match owner {
        Owner::Pill { transport, .. } => matches!(transport, Transport::Window),
        Owner::Window(_) => true,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn session_supported(_owner: &Owner) -> bool {
    false
}

#[cfg(target_os = "linux")]
async fn capture_to_file(_app: &AppHandle, _owner: &Owner, title: &str, path: &std::path::Path) -> bool {
    let Some(path) = path.to_str() else {
        log::warn!("screenshot path is not valid UTF-8");
        return false;
    };
    screenshot::capture_gnome(path, title).await
}

#[cfg(target_os = "macos")]
async fn capture_to_file(app: &AppHandle, owner: &Owner, _title: &str, path: &std::path::Path) -> bool {
    screenshot::capture_macos(app, owner, path).await
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn capture_to_file(_app: &AppHandle, _owner: &Owner, _title: &str, _path: &std::path::Path) -> bool {
    false
}

#[cfg(target_os = "linux")]
async fn capture_text(_app: &AppHandle, _owner: &Owner, title: &str, _run: u64) -> Option<String> {
    selection::capture_gnome(title).await
}

#[cfg(target_os = "macos")]
async fn capture_text(app: &AppHandle, owner: &Owner, _title: &str, run: u64) -> Option<String> {
    selection::capture_macos(app, owner, run).await
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn capture_text(_app: &AppHandle, _owner: &Owner, _title: &str, _run: u64) -> Option<String> {
    None
}

pub async fn screenshot(app: &AppHandle, owner: &Owner) -> crate::Result<Option<Vec<u8>>> {
    if !session_supported(owner) {
        log::debug!("screenshot unsupported on this session, owner={}", owner.kind());
        return Ok(None);
    }
    let Some(title) = owner_title(app, owner) else {
        return Ok(None);
    };
    let run = start(owner.label(), &title);
    let started = Instant::now();

    let dir = app.path().app_cache_dir()?.join(screenshot::DIR_NAME);
    let path = dir.join(format!("{}.png", uuid::Uuid::new_v4()));
    tauri::async_runtime::spawn_blocking(move || std::fs::create_dir_all(dir)).await??;
    log::debug!("screenshot file: {}", path.display());

    let captured = capture_to_file(app, owner, &title, &path).await;
    let bytes = tauri::async_runtime::spawn_blocking(move || screenshot::read_and_delete(&path, captured)).await?;
    if !still_valid_now(app, owner, run).await {
        log::debug!("screenshot dropped: owner changed, owner={}", owner.kind());
        return Ok(None);
    }
    log::debug!(
        "screenshot finished: owner={}, captured={}, {} ms",
        owner.kind(),
        bytes.is_some(),
        started.elapsed().as_millis()
    );
    Ok(bytes)
}

pub async fn selection(app: &AppHandle, owner: &Owner) -> Option<String> {
    if !session_supported(owner) {
        log::debug!("select text unsupported on this session, owner={}", owner.kind());
        return None;
    }
    let title = owner_title(app, owner)?;
    let run = start(owner.label(), &title);
    let started = Instant::now();

    let text = capture_text(app, owner, &title, run).await;
    if !still_valid_now(app, owner, run).await {
        log::debug!("selection dropped: owner changed, owner={}", owner.kind());
        return None;
    }
    let text = text.filter(|text| !text.is_empty())?;
    log::debug!(
        "selection captured: owner={}, {} chars, {} ms",
        owner.kind(),
        text.chars().count(),
        started.elapsed().as_millis()
    );
    Some(text)
}

pub fn png_base64(bytes: Vec<u8>) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pill(generation: u64) -> Owner {
        #[cfg(target_os = "linux")]
        let transport = Transport::Shell;
        #[cfg(not(target_os = "linux"))]
        let transport = Transport::Window;
        Owner::Pill { transport, generation }
    }

    fn lock_runs() -> std::sync::MutexGuard<'static, ()> {
        selection::RUN_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn owned_title_now(label: &str) -> Option<String> {
        owned_title(lock_current().as_ref(), label, selection::is_current).map(str::to_string)
    }

    #[test]
    fn a_shown_unlocked_pill_of_the_same_generation_is_valid() {
        assert!(still_valid(&pill(3), true, Some(3), false));
    }

    #[test]
    fn a_hidden_pill_is_not_valid() {
        assert!(!still_valid(&pill(3), true, None, false));
    }

    #[test]
    fn a_pill_of_another_generation_is_not_valid() {
        assert!(!still_valid(&pill(3), true, Some(4), false));
    }

    #[test]
    fn a_locked_pill_is_not_valid() {
        assert!(!still_valid(&pill(3), true, Some(3), true));
    }

    #[test]
    fn a_pill_whose_run_ended_is_not_valid() {
        assert!(!still_valid(&pill(3), false, Some(3), false));
    }

    #[test]
    fn a_window_owner_is_valid_while_its_run_is_current_whatever_the_pill_state() {
        for owner in [WindowOwner::ContextMenu, WindowOwner::Chat] {
            let owner = Owner::Window(owner);
            assert!(still_valid(&owner, true, None, false));
            assert!(still_valid(&owner, true, Some(9), true));
            assert!(!still_valid(&owner, false, None, false));
            assert!(!still_valid(&owner, false, Some(9), false));
        }
    }

    #[test]
    fn window_owners_are_found_by_their_label() {
        assert_eq!(WindowOwner::from_label("context-menu"), Some(WindowOwner::ContextMenu));
        assert_eq!(WindowOwner::from_label(CONVERSATION_DIALOG_LABEL), Some(WindowOwner::Chat));
        assert_eq!(WindowOwner::from_label(WINDOW_LABEL), None);
        assert_eq!(WindowOwner::from_label("settings"), None);
    }

    #[test]
    fn a_new_capture_ends_the_running_one() {
        let _runs = lock_runs();
        let menu = start(WindowOwner::ContextMenu.label(), "menu title");
        assert!(selection::is_current(menu));
        let pill = start(WINDOW_LABEL, "");
        assert!(!selection::is_current(menu));
        assert!(selection::is_current(pill));
    }

    #[test]
    fn cancelling_for_a_label_cancels_nothing_while_another_owner_holds_the_run() {
        let _runs = lock_runs();
        let pill = start(WINDOW_LABEL, "");
        assert_eq!(owned_title_now("context-menu"), None);
        cancel_owned_by("context-menu");
        assert!(selection::is_current(pill));
    }

    #[test]
    fn cancelling_for_the_owner_label_ends_its_run_and_names_its_title() {
        let _runs = lock_runs();
        let menu = start(WindowOwner::ContextMenu.label(), "menu title");
        assert_eq!(owned_title_now("context-menu").as_deref(), Some("menu title"));
        cancel_owned_by("context-menu");
        assert!(!selection::is_current(menu));
        assert_eq!(owned_title_now("context-menu"), None);
    }
}
