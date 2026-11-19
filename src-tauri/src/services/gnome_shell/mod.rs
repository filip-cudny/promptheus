use std::sync::OnceLock;

use zbus::Connection;

#[zbus::proxy(
    interface = "com.promptheus.Shell",
    default_service = "org.gnome.Shell",
    default_path = "/com/promptheus/Shell"
)]
pub trait Shell {
    fn set_shortcuts(
        &self,
        shortcuts: std::collections::HashMap<String, Vec<String>>,
    ) -> zbus::Result<Vec<String>>;

    fn get_pointer(&self) -> zbus::Result<(i32, i32, i32, i32, i32, i32)>;

    fn get_focused_wm_class(&self) -> zbus::Result<String>;

    fn activate_window(&self, title: &str) -> zbus::Result<bool>;

    fn paste_into_app(&self, wm_class: &str, uris: &[&str]) -> zbus::Result<String>;

    fn restore_app_window(&self, wm_class: &str) -> zbus::Result<String>;

    fn watch_app_titles(&self, wm_class: &str, duration_ms: u32) -> zbus::Result<()>;

    fn capture_area(&self, path: &str, owner: &str) -> zbus::Result<String>;

    fn capture_selection(&self, owner: &str) -> zbus::Result<(String, String)>;

    fn cancel_capture(&self, owner: &str) -> zbus::Result<bool>;

    fn show_toast(
        &self,
        level: &str,
        title: &str,
        message: &str,
        monochromatic: bool,
        action: &str,
    ) -> zbus::Result<()>;

    fn show_recording_widget(
        &self,
        x: i32,
        y: i32,
        has_position: bool,
        monochromatic: bool,
    ) -> zbus::Result<(i32, i32, i32, i32, i32, i32)>;

    fn update_recording_widget(
        &self,
        state: &str,
        level: f64,
        elapsed_ms: u32,
    ) -> zbus::Result<()>;

    fn update_recording_widget_retry(
        &self,
        attempt: u32,
        max_attempts: u32,
        delay_ms: u32,
    ) -> zbus::Result<()>;

    #[allow(clippy::too_many_arguments)]
    fn update_recording_widget_model(
        &self,
        targets: Vec<(String, String)>,
        selected_id: &str,
        selected_label: &str,
        target_locked: bool,
        items: Vec<(u32, String, String, String, String)>,
        item_rows: u32,
        context_version: u64,
        x: i32,
        y: i32,
        stop_chat_destination: &str,
    ) -> zbus::Result<()>;

    fn hide_recording_widget(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn recording_widget_action(&self, action: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn recording_widget_target_selected(&self, id: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn recording_widget_item_removed(&self, index: u32, context_version: u64) -> zbus::Result<()>;

    #[zbus(signal)]
    fn recording_widget_moved(
        &self,
        x: i32,
        y: i32,
        work_x: i32,
        work_y: i32,
        work_width: i32,
        work_height: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ToastActivated")]
    fn toast_activated(&self, action: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn shortcut_activated(&self, action: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn ready(&self) -> zbus::Result<()>;
}

static CONNECTION: OnceLock<Connection> = OnceLock::new();

fn is_gnome_wayland_session(session_type: Option<&str>, current_desktop: Option<&str>) -> bool {
    session_type == Some("wayland")
        && current_desktop.is_some_and(|d| d.split(':').any(|part| part == "GNOME"))
}

pub fn is_gnome_wayland() -> bool {
    is_gnome_wayland_session(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
    )
}

async fn connection() -> zbus::Result<Connection> {
    if let Some(connection) = CONNECTION.get() {
        return Ok(connection.clone());
    }
    let connection = Connection::session().await?;
    log::debug!("connected to the session bus");
    Ok(CONNECTION.get_or_init(|| connection).clone())
}

pub async fn proxy() -> zbus::Result<ShellProxy<'static>> {
    ShellProxy::new(&connection().await?).await
}

pub fn blocking_proxy() -> zbus::Result<ShellProxyBlocking<'static>> {
    let connection = zbus::block_on(connection())?;
    ShellProxyBlocking::new(&zbus::blocking::Connection::from(connection))
}

fn to_error(e: impl std::fmt::Display) -> crate::Error {
    crate::Error::Other(e.to_string())
}

/// Moves the window titled `title` to the logical stage position `(x, y)`.
///
/// With `show` set, the `PlaceWindow` call goes on the bus before `show` runs:
/// GTK3 on Wayland creates the surface at `show()`, and the extension waits for
/// the window and keeps it hidden until its first frame, then moves it.
pub async fn place_window(
    title: &str,
    x: i32,
    y: i32,
    activate: bool,
    show: Option<&tauri::WebviewWindow>,
) -> crate::Result<bool> {
    call_placement("PlaceWindow", title, x, y, activate, show).await
}

/// Keeps the frame of the window titled `title` anchored with its bottom-right
/// corner at the logical stage position `(right, bottom)`.
///
/// The extension re-applies the anchor on every size change of the window
/// until it is unmanaged, so callers only resize afterwards. `show` behaves as
/// in [`place_window`].
pub async fn place_window_anchored(
    title: &str,
    right: i32,
    bottom: i32,
    activate: bool,
    show: Option<&tauri::WebviewWindow>,
) -> crate::Result<bool> {
    call_placement("PlaceWindowAnchored", title, right, bottom, activate, show).await
}

/// Moves the frame of the window titled `title` to its transient parent's
/// frame origin plus `(dx, dy)` logical stage pixels.
///
/// Returns `false` when the window has no transient parent or was not found.
/// `show` behaves as in [`place_window`].
pub async fn place_window_on_parent(
    title: &str,
    dx: i32,
    dy: i32,
    activate: bool,
    show: Option<&tauri::WebviewWindow>,
) -> crate::Result<bool> {
    call_placement("PlaceWindowOnParent", title, dx, dy, activate, show).await
}

pub async fn set_transient_parent(
    child: &tauri::WebviewWindow,
    parent: &tauri::Window,
) -> crate::Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let child_clone = child.clone();
    let parent_clone = parent.clone();
    child.run_on_main_thread(move || {
        let result = (|| -> crate::Result<()> {
            let child_gtk = child_clone.gtk_window()?;
            let parent_gtk = parent_clone.gtk_window()?;
            gtk::prelude::GtkWindowExt::set_transient_for(&child_gtk, Some(&parent_gtk));
            Ok(())
        })();
        let _ = tx.send(result);
    })?;
    rx.await.map_err(to_error)??;
    log::debug!(
        "set_transient_parent: child={} parent={}",
        child.label(),
        parent.label(),
    );
    Ok(())
}

async fn call_placement(
    method: &'static str,
    title: &str,
    a: i32,
    b: i32,
    activate: bool,
    show: Option<&tauri::WebviewWindow>,
) -> crate::Result<bool> {
    let proxy = proxy().await.map_err(to_error)?;
    let body = (title, a, b, activate);
    let t = std::time::Instant::now();
    let placed = match show {
        Some(win) => call_before_show(&proxy, method, &body, win).await?,
        None => proxy.inner().call(method, &body).await.map_err(to_error)?,
    };
    log::debug!(
        "{method}({title:?}, ({a}, {b}), activate={activate}, show={}) -> {placed} in {:?}",
        show.is_some(),
        t.elapsed(),
    );
    if !placed {
        log::warn!("extension found no window titled {title:?} for {method}");
    }
    Ok(placed)
}

async fn call_before_show(
    proxy: &ShellProxy<'static>,
    method: &'static str,
    body: &(&str, i32, i32, bool),
    win: &tauri::WebviewWindow,
) -> crate::Result<bool> {
    use futures::StreamExt;

    let connection = proxy.inner().connection().clone();
    let mut replies = zbus::MessageStream::from(&connection);
    let call = zbus::Message::method_call("/com/promptheus/Shell", method)
        .and_then(|b| b.destination("org.gnome.Shell"))
        .and_then(|b| b.interface("com.promptheus.Shell"))
        .and_then(|b| b.build(body))
        .map_err(to_error)?;
    let serial = call.primary_header().serial_num();
    connection.send(&call).await.map_err(to_error)?;
    win.show()?;
    let reply = loop {
        let message = replies
            .next()
            .await
            .ok_or_else(|| to_error("session bus connection closed"))?
            .map_err(to_error)?;
        if message.header().reply_serial() == Some(serial) {
            break message;
        }
    };
    if reply.message_type() == zbus::message::Type::Error {
        return Err(to_error(format!(
            "{method} failed: {:?}",
            reply.header().error_name()
        )));
    }
    reply.body().deserialize::<bool>().map_err(to_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_with_gnome_is_gnome_wayland() {
        assert!(is_gnome_wayland_session(Some("wayland"), Some("ubuntu:GNOME")));
    }

    #[test]
    fn x11_with_gnome_is_not_gnome_wayland() {
        assert!(!is_gnome_wayland_session(Some("x11"), Some("GNOME")));
    }

    #[test]
    fn wayland_with_kde_is_not_gnome_wayland() {
        assert!(!is_gnome_wayland_session(Some("wayland"), Some("KDE")));
    }

    #[test]
    fn missing_variables_are_not_gnome_wayland() {
        assert!(!is_gnome_wayland_session(None, None));
        assert!(!is_gnome_wayland_session(Some("wayland"), None));
        assert!(!is_gnome_wayland_session(None, Some("GNOME")));
    }
}
