#[cfg(target_os = "linux")]
use std::sync::Mutex;

#[cfg(target_os = "linux")]
type WindowRect = (i32, i32, i32, i32);

// One rectangle for the whole process: it assumes a single window (the context menu)
// uses it. It stops being correct once a second window calls `install` or `set`;
// upgrade path: key the rectangle by window label.
#[cfg(target_os = "linux")]
static STORED: Mutex<Option<WindowRect>> = Mutex::new(None);

#[cfg(target_os = "linux")]
fn window_rect((x, y, width, height): (f64, f64, f64, f64)) -> WindowRect {
    let left = x.floor() as i32;
    let top = y.floor() as i32;
    let right = (x + width).ceil() as i32;
    let bottom = (y + height).ceil() as i32;
    (left, top, right - left, bottom - top)
}

#[cfg(target_os = "linux")]
fn stored() -> Option<WindowRect> {
    *STORED.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(target_os = "linux")]
fn apply_stored(gtk_window: &gtk::ApplicationWindow) {
    use gtk::prelude::WidgetExt;

    let Some((x, y, width, height)) = stored() else {
        return;
    };
    let region = gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(
        x, y, width, height,
    ));
    gtk_window.input_shape_combine_region(Some(&region));
}

#[cfg(target_os = "linux")]
pub fn install(window: &tauri::Window) {
    if !super::gnome_shell::is_gnome_wayland() {
        return;
    }
    let host = window.clone();
    if let Err(e) = window.run_on_main_thread(move || install_on_main_thread(&host)) {
        log::warn!(
            target: "app_lib::services::menu_input_region",
            "run_on_main_thread failed for {}: {e}",
            window.label(),
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub fn install(_window: &tauri::Window) {}

#[cfg(target_os = "linux")]
fn install_on_main_thread(window: &tauri::Window) {
    use gtk::prelude::WidgetExt;

    match window.gtk_window() {
        Ok(gtk_window) => {
            gtk_window.connect_configure_event(|gtk_window, _| {
                apply_stored(gtk_window);
                false
            });
        }
        Err(e) => log::warn!(
            target: "app_lib::services::menu_input_region",
            "no gtk window for {}: {e}",
            window.label(),
        ),
    }
}

#[cfg(target_os = "linux")]
pub fn set(window: &tauri::Window, rect: (f64, f64, f64, f64)) {
    let (x, y, width, height) = window_rect(rect);
    log::debug!(
        target: "app_lib::services::menu_input_region",
        "set x={x} y={y} width={width} height={height}",
    );
    *STORED.lock().unwrap_or_else(|e| e.into_inner()) = Some((x, y, width, height));
    let target = window.clone();
    if let Err(e) = window.run_on_main_thread(move || match target.gtk_window() {
        Ok(gtk_window) => apply_stored(&gtk_window),
        Err(e) => log::warn!(
            target: "app_lib::services::menu_input_region",
            "no gtk window for {}: {e}",
            target.label(),
        ),
    }) {
        log::warn!(
            target: "app_lib::services::menu_input_region",
            "run_on_main_thread failed for {}: {e}",
            window.label(),
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set(_window: &tauri::Window, _rect: (f64, f64, f64, f64)) {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::window_rect;

    #[test]
    fn rounds_outward() {
        assert_eq!(window_rect((0.0, 193.6, 320.0, 456.2)), (0, 193, 320, 457));
    }

    #[test]
    fn whole_numbers_stay() {
        assert_eq!(window_rect((0.0, 0.0, 320.0, 650.0)), (0, 0, 320, 650));
    }
}
