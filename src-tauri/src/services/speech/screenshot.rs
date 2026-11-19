use std::path::Path;

pub const DIR_NAME: &str = "pill-screenshots";

pub fn keep_capture(click_generation: u64, current_generation: Option<u64>, target_locked: bool) -> bool {
    current_generation == Some(click_generation) && !target_locked
}

/// Returns the file content when `read` is set; the file is deleted either way.
pub fn read_and_delete(path: &Path, read: bool) -> Option<Vec<u8>> {
    let bytes = if read {
        std::fs::read(path)
            .map_err(|e| log::warn!("reading the screenshot failed: {e}"))
            .ok()
    } else {
        None
    };
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("deleting the screenshot failed: {e}"),
    }
    bytes
}

#[cfg(target_os = "linux")]
pub async fn capture_gnome(path: &str, owner_title: &str) -> bool {
    let reply = async { crate::services::gnome_shell::proxy().await?.capture_area(path, owner_title).await }.await;
    match reply {
        Ok(outcome) if outcome == "captured" => true,
        Ok(outcome) => {
            log::debug!("screenshot not captured: {outcome}");
            false
        }
        Err(e) => {
            log::warn!("CaptureArea failed: {e}");
            false
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::Path;

    use tauri::{AppHandle, Manager};

    use crate::services::dialog::{focus_host_window, CONVERSATION_DIALOG_LABEL};
    use crate::services::notification::{NotificationLevel, NotificationService};
    use crate::services::speech::capture::{Owner, WindowOwner};
    use crate::services::speech::widget::RecordingWidget;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    async fn warn_permission_missing(app: &AppHandle) {
        unsafe { CGRequestScreenCaptureAccess() };
        log::info!("screenshot skipped: Screen Recording permission missing");
        NotificationService::notify_now(
            app,
            "screenshot_permission",
            NotificationLevel::Warning,
            "Screen Recording permission needed",
            Some("Allow Promptheus in System Settings → Privacy & Security → Screen Recording, then restart Promptheus"),
        )
        .await;
    }

    async fn hide_owner(app: &AppHandle, owner: &Owner) -> bool {
        match owner {
            Owner::Pill { generation, .. } => app.state::<RecordingWidget>().hide_for_capture(app, *generation).await,
            Owner::Window(WindowOwner::ContextMenu) => {
                match crate::commands::menu::hide_context_menu_panel(app.clone(), true).await {
                    Ok(()) => true,
                    Err(e) => {
                        log::warn!("hiding the context menu for a capture failed: {e}");
                        false
                    }
                }
            }
            Owner::Window(WindowOwner::Chat) => {
                let Some(host) = app.get_window(CONVERSATION_DIALOG_LABEL) else {
                    log::warn!("hiding the chat for a capture failed: window not found");
                    return false;
                };
                host.hide()
                    .map_err(|e| log::warn!("hiding the chat for a capture failed: {e}"))
                    .is_ok()
            }
        }
    }

    async fn hide_pill_beside_window_owner(app: &AppHandle, owner: &Owner) -> Option<u64> {
        if !matches!(owner, Owner::Window(_)) {
            return None;
        }
        let widget = app.state::<RecordingWidget>();
        let (_, generation) = widget.shown_generation().await?;
        widget.hide_for_capture(app, generation).await.then_some(generation)
    }

    async fn restore_owner(app: &AppHandle, owner: &Owner) {
        match owner {
            Owner::Pill { generation, .. } => {
                app.state::<RecordingWidget>().restore_after_capture(app, *generation).await;
            }
            Owner::Window(WindowOwner::ContextMenu) => {
                if let Err(e) = crate::commands::menu::show_context_menu_panel(app.clone()).await {
                    log::warn!("showing the context menu after a capture failed: {e}");
                    return;
                }
                if let Err(e) = crate::commands::menu::focus_context_menu(app.clone()).await {
                    log::warn!("focusing the context menu after a capture failed: {e}");
                }
            }
            Owner::Window(WindowOwner::Chat) => {
                let Some(host) = app.get_window(CONVERSATION_DIALOG_LABEL) else {
                    log::warn!("showing the chat after a capture failed: window not found");
                    return;
                };
                if let Err(e) = host.show() {
                    log::warn!("showing the chat after a capture failed: {e}");
                    return;
                }
                if let Err(e) = focus_host_window(app, CONVERSATION_DIALOG_LABEL) {
                    log::warn!("focusing the chat after a capture failed: {e}");
                }
            }
        }
    }

    pub async fn capture(app: &AppHandle, owner: &Owner, path: &Path) -> bool {
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            warn_permission_missing(app).await;
            return false;
        }
        if !hide_owner(app, owner).await {
            return false;
        }
        let hidden_pill = hide_pill_beside_window_owner(app, owner).await;
        let target = path.to_path_buf();
        let status = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new("/usr/sbin/screencapture")
                .args(["-i", "-x"])
                .arg(target)
                .status()
        })
        .await;
        if let Some(generation) = hidden_pill {
            app.state::<RecordingWidget>().restore_after_capture(app, generation).await;
        }
        restore_owner(app, owner).await;
        match status {
            Ok(Ok(status)) => log::debug!("screencapture exited: {status}"),
            Ok(Err(e)) => log::warn!("starting screencapture failed: {e}"),
            Err(e) => log::warn!("screencapture task failed: {e}"),
        }
        let captured = path.exists();
        if !captured {
            log::debug!("screenshot cancelled: no file written");
        }
        captured
    }
}

#[cfg(target_os = "macos")]
pub use macos::capture as capture_macos;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shown_unlocked_pill_of_the_same_generation_keeps_the_capture() {
        assert!(keep_capture(3, Some(3), false));
    }

    #[test]
    fn a_closed_pill_drops_the_capture() {
        assert!(!keep_capture(3, None, false));
    }

    #[test]
    fn another_generation_drops_the_capture() {
        assert!(!keep_capture(3, Some(4), false));
    }

    #[test]
    fn a_locked_target_drops_the_capture() {
        assert!(!keep_capture(3, Some(3), true));
    }
}
