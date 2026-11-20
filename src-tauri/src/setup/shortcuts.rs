#[cfg(desktop)]
use std::collections::HashMap;
#[cfg(desktop)]
use std::sync::RwLock;

#[cfg(desktop)]
use tauri::Manager;

#[cfg(desktop)]
use crate::models::settings::Settings;
#[cfg(desktop)]
use crate::services::hotkeys::{execute_hotkey_action, reload_shortcuts, ShortcutActionMap};

#[cfg(desktop)]
pub fn register(
    app: &tauri::App,
    settings: &Settings,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::ShortcutState;

    #[cfg(target_os = "linux")]
    if crate::services::gnome_shell::is_gnome_wayland() {
        log::info!("GNOME Wayland session: shortcuts are grabbed by the shell extension");
        tauri::async_runtime::spawn(run_shell_shortcuts(
            app.handle().clone(),
            settings.clone(),
        ));
        return Ok(());
    }

    app.manage(ShortcutActionMap(RwLock::new(HashMap::new())));

    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let shortcut_str = shortcut.into_string();
                    let action_map = app.state::<ShortcutActionMap>();
                    let map = action_map.0.read().unwrap();
                    if let Some(action) = map.get(&shortcut_str) {
                        let action = action.clone();
                        drop(map);
                        log::info!(
                            target: "app_lib::hotkey_handler",
                            "hotkey action: {} -> {}",
                            shortcut_str, action,
                        );
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            execute_hotkey_action(&app, &action).await;
                        });
                    } else {
                        log::warn!(
                            target: "app_lib::hotkey_handler",
                            "hotkey pressed but no action found: {}",
                            shortcut_str,
                        );
                    }
                }
            })
            .build(),
    )?;

    reload_shortcuts(app.handle(), settings);

    Ok(())
}

#[cfg(target_os = "linux")]
async fn run_shell_shortcuts(app: tauri::AppHandle, initial_settings: Settings) {
    use std::sync::Arc;

    use futures::StreamExt;
    use tokio::sync::Mutex;

    use crate::services::config::ConfigService;
    use crate::services::hotkeys::sync_shell_shortcuts;

    let proxy = match crate::services::gnome_shell::proxy().await {
        Ok(proxy) => proxy,
        Err(e) => {
            log::warn!("failed to connect to the shell extension, shortcuts are disabled: {e}");
            return;
        }
    };
    let (mut ready, mut activated) = match (
        proxy.receive_ready().await,
        proxy.receive_shortcut_activated().await,
    ) {
        (Ok(ready), Ok(activated)) => (ready, activated),
        (Err(e), _) | (_, Err(e)) => {
            log::warn!("failed to subscribe to shell extension signals, shortcuts are disabled: {e}");
            return;
        }
    };

    let current_settings = || async {
        match app.try_state::<Arc<Mutex<ConfigService>>>() {
            Some(config) => config.lock().await.settings().clone(),
            None => initial_settings.clone(),
        }
    };

    sync_shell_shortcuts(&current_settings().await).await;

    let mut widget_actions = subscribe_or_warn(proxy.receive_recording_widget_action().await);
    let mut widget_moves = subscribe_or_warn(proxy.receive_recording_widget_moved().await);
    let mut widget_targets = subscribe_or_warn(proxy.receive_recording_widget_target_selected().await);
    let mut widget_removals = subscribe_or_warn(proxy.receive_recording_widget_item_removed().await);
    let mut toast_activations = subscribe_or_warn(proxy.receive_toast_activated().await);

    loop {
        tokio::select! {
            Some(signal) = next_signal(&mut widget_actions) => {
                let action = match signal.args() {
                    Ok(args) => args.action.to_string(),
                    Err(e) => {
                        log::warn!("invalid RecordingWidgetAction signal: {e}");
                        continue;
                    }
                };
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    run_widget_action(app, &action).await;
                });
            }
            Some(signal) = next_signal(&mut widget_targets) => {
                match signal.args() {
                    Ok(args) => {
                        let id = args.id.to_string();
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = crate::services::speech::session::select_recording_widget_target(app, id).await {
                                log::warn!("recording widget target selection failed: {e}");
                            }
                        });
                    }
                    Err(e) => log::warn!("invalid RecordingWidgetTargetSelected signal: {e}"),
                }
            }
            Some(signal) = next_signal(&mut widget_removals) => {
                match signal.args() {
                    Ok(args) => {
                        let (index, version) = (args.index, args.context_version);
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = crate::services::speech::session::remove_recording_widget_item(app, index, version).await {
                                log::warn!("recording widget item removal failed: {e}");
                            }
                        });
                    }
                    Err(e) => log::warn!("invalid RecordingWidgetItemRemoved signal: {e}"),
                }
            }
            Some(signal) = next_signal(&mut toast_activations) => {
                match signal.args() {
                    Ok(args) if args.action == "open_settings" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = crate::commands::settings_dialog::open_settings_window(app, None).await {
                                log::warn!("opening Settings from a toast failed: {e}");
                            }
                        });
                    }
                    Ok(args) => log::warn!("unknown toast action: {}", args.action),
                    Err(e) => log::warn!("invalid ToastActivated signal: {e}"),
                }
            }
            Some(signal) = next_signal(&mut widget_moves) => {
                match signal.args() {
                    Ok(args) => {
                        let placement = crate::services::speech::widget::Placement {
                            origin: (args.x as f64, args.y as f64),
                            work_area: (
                                args.work_x as f64,
                                args.work_y as f64,
                                args.work_width as f64,
                                args.work_height as f64,
                            ),
                        };
                        app.state::<crate::services::speech::widget::RecordingWidget>()
                            .shell_moved(&app, placement, args.scale);
                    }
                    Err(e) => log::warn!("invalid RecordingWidgetMoved signal: {e}"),
                }
            }
            Some(_) = ready.next() => {
                log::info!("shell extension ready, registering shortcuts");
                sync_shell_shortcuts(&current_settings().await).await;
            }
            Some(signal) = activated.next() => {
                let action = match signal.args() {
                    Ok(args) => args.action.to_string(),
                    Err(e) => {
                        log::warn!("invalid ShortcutActivated signal: {e}");
                        continue;
                    }
                };
                log::info!(
                    target: "app_lib::hotkey_handler",
                    "hotkey action: {}",
                    action,
                );
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    execute_hotkey_action(&app, &action).await;
                });
            }
            else => break,
        }
    }
    log::warn!("shell extension signal streams ended, shortcuts are disabled");
}

#[cfg(target_os = "linux")]
fn subscribe_or_warn<S>(stream: zbus::Result<S>) -> Option<S> {
    stream
        .map_err(|e| log::warn!("failed to subscribe to recording widget signals: {e}"))
        .ok()
}

#[cfg(target_os = "linux")]
async fn next_signal<S: futures::Stream + Unpin>(stream: &mut Option<S>) -> Option<S::Item> {
    use futures::StreamExt;

    match stream {
        Some(stream) => stream.next().await,
        None => std::future::pending().await,
    }
}

#[cfg(target_os = "linux")]
async fn run_widget_action(app: tauri::AppHandle, action: &str) {
    use crate::services::speech::session as speech;

    log::debug!("recording widget action: {action}");
    let result = match action {
        "pause" => speech::pause_speech_recording(app).await,
        "resume" => speech::resume_speech_recording(app).await,
        "stop" => speech::toggle_speech_recording(app, None).await,
        "stop_chat" => speech::stop_chat_speech_recording(app).await,
        "cancel" => speech::cancel_speech_recording(app).await,
        "paste" => speech::paste_into_recording_widget(app).await,
        "screenshot" => speech::capture_screenshot_into_recording_widget(app).await,
        "select_text" => speech::capture_selection_into_recording_widget(app).await,
        other => {
            log::warn!("unknown recording widget action: {other}");
            return;
        }
    };
    if let Err(e) = result {
        log::warn!("recording widget action {action} failed: {e}");
    }
}

#[cfg(not(desktop))]
pub fn register(
    _app: &tauri::App,
    _settings: &crate::models::settings::Settings,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
