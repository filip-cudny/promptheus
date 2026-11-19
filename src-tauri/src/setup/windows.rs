use tauri::webview::WebviewWindowBuilder;

fn overlay<'a>(
    app: &'a tauri::App,
    label: &str,
    title: &str,
    width: f64,
    height: f64,
) -> WebviewWindowBuilder<'a, tauri::Wry, tauri::App> {
    WebviewWindowBuilder::new(
        app,
        label,
        tauri::WebviewUrl::App(format!("{label}.html").into()),
    )
    .title(title)
    .inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
}

pub fn create(app: &tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context_menu_window = overlay(
        app,
        "context-menu",
        crate::commands::menu::CONTEXT_MENU_TITLE,
        320.0,
        400.0,
    )
    .build()?;

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = crate::services::macos_panel::make_nonactivating_panel(&context_menu_window)
        {
            log::warn!("failed to convert context-menu to non-activating panel: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = context_menu_window;

    overlay(
        app,
        "notification",
        crate::services::notification_window::NOTIFICATION_TITLE,
        380.0,
        100.0,
    )
    .shadow(false)
    .focusable(false)
    .build()?;

    let recording_widget = overlay(app, "recording-widget", "", 264.0, 72.0)
        .shadow(false)
        .focusable(false)
        .accept_first_mouse(true)
        .build()?;

    #[cfg(target_os = "macos")]
    {
        if let Err(e) =
            crate::services::macos_panel::make_nonactivating_widget_panel(&recording_widget)
        {
            log::warn!("failed to convert recording-widget to non-activating panel: {e}");
        }
        if let Err(e) = crate::services::macos_panel::install_pointer_tracking(
            &recording_widget,
            app.handle().clone(),
        ) {
            log::warn!("failed to install recording-widget pointer tracking: {e}");
        }
    }

    let handle = app.handle().clone();
    recording_widget.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(position) = event {
            crate::services::speech::widget::window_moved(&handle, *position);
        }
    });

    overlay(
        app,
        "image-preview",
        crate::commands::image_preview::IMAGE_PREVIEW_TITLE,
        800.0,
        800.0,
    )
    .shadow(false)
    .build()?;

    overlay(
        app,
        "provider-menu",
        crate::commands::provider_menu::PROVIDER_MENU_TITLE,
        180.0,
        120.0,
    )
    .build()?;

    overlay(
        app,
        "palette-backdrop",
        crate::services::ai_webview::PALETTE_BACKDROP_TITLE,
        800.0,
        600.0,
    )
    .focusable(false)
    .shadow(false)
    .build()?;

    overlay(
        app,
        "palette",
        crate::services::ai_webview::PALETTE_TITLE,
        520.0,
        400.0,
    )
    .shadow(false)
    .build()?;

    #[cfg(target_os = "linux")]
    configure_overlay_windows_linux(app);

    Ok(())
}

#[cfg(target_os = "linux")]
fn set_utility_hints(app: &tauri::App, label: &str) -> Option<gtk::ApplicationWindow> {
    use gtk::gdk::WindowTypeHint;
    use gtk::prelude::GtkWindowExt;
    use tauri::Manager;

    let gtk_win = app.get_webview_window(label)?.gtk_window().ok()?;
    gtk_win.set_type_hint(WindowTypeHint::Utility);
    gtk_win.set_skip_pager_hint(true);
    gtk_win.set_skip_taskbar_hint(true);
    Some(gtk_win)
}

#[cfg(target_os = "linux")]
fn configure_overlay_windows_linux(app: &tauri::App) {
    use gtk::prelude::GtkWindowExt;

    if let Some(backdrop) = set_utility_hints(app, "palette-backdrop") {
        backdrop.set_accept_focus(false);
    }
    set_utility_hints(app, "palette");
    set_utility_hints(app, "context-menu");
}
