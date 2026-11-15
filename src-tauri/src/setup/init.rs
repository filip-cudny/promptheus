use tauri::Manager;

use crate::commands::settings_dialog::open_settings_window;
use crate::services::{self, config::ConfigService};
use crate::setup;

pub fn run(app: &mut tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    services::capability_audit::assert_no_ai_webview_ipc_grant();

    #[cfg(target_os = "linux")]
    services::webkit_pressure::install();

    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        setup::menu::install(app)?;
    }

    let config_dir = app.path().app_config_dir()?;
    services::config::load_env(&config_dir);
    let config_service = ConfigService::load(&config_dir)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    log::info!("config loaded from {}", config_dir.display());
    services::secrets::copy_renamed_values(&config_dir, config_service.settings());

    if config_service.settings().show_tray_icon {
        setup::tray::install(app)?;
        log::info!("system tray initialized");
    }

    services::dock_icon::apply(app.handle(), config_service.settings().appearance.logo);

    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::MacosLauncher;
        app.handle().plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))?;
        services::autostart::reconcile(
            app.handle(),
            config_service.settings(),
            !cfg!(debug_assertions),
        );
    }

    setup::windows::create(app)?;
    setup::shortcuts::register(app, config_service.settings())?;
    let in_fallback = config_service.fallback().is_some();
    setup::state::manage(app, &config_dir, config_service)?;
    if in_fallback {
        let handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = open_settings_window(handle, None).await {
                log::warn!("failed to open the settings window for the settings.json error: {e}");
            }
        });
    }
    setup::background::spawn_heartbeat(app.handle().clone());
    setup::background::spawn_ai_webview_unload(app.handle().clone());
    setup::background::spawn_audio_clip_sweeper(app.handle().clone());
    setup::background::spawn_settings_sync(app.handle().clone());
    setup::background::spawn_missing_env_check(app.handle().clone());
    setup::background::spawn_claude_desktop_cache_clear(app.handle().clone());

    Ok(())
}
