pub mod ai;
pub mod ai_webview;
pub mod change_log;
pub mod clipboard;
pub mod context;
pub mod context_editor;
pub mod dock;
pub mod env_vars;
pub mod execution_control;
pub mod execution_generation;
pub mod execution_stream;
pub mod history;
pub mod history_dialog;
pub mod image_preview;
pub mod mcp;
pub mod menu;
pub mod notification;
pub mod prompts;
pub mod provider_menu;
pub mod conversation_dialog;
pub mod settings;
pub mod settings_agent;
pub mod settings_dialog;
pub mod settings_sync;
pub mod setup;
pub mod skills;
pub mod speech;
pub mod text_preview;
pub mod tokenizer;
pub mod ui_state;

#[macro_export]
macro_rules! handlers {
    () => {
        tauri::generate_handler![
            // === ai ===
            $crate::commands::ai::get_model_capabilities,
            // === ai_webview ===
            $crate::commands::ai_webview::get_webview_providers,
            $crate::commands::ai_webview::reload_active_in_host,
            $crate::commands::ai_webview::open_palette,
            $crate::commands::ai_webview::close_palette,
            $crate::commands::ai_webview::get_tabs,
            $crate::commands::ai_webview::activate_tab,
            $crate::commands::ai_webview::close_tab,
            $crate::commands::ai_webview::new_tab,
            $crate::commands::ai_webview::activate_adjacent_tab,
            $crate::commands::ai_webview::activate_tab_at,
            $crate::commands::ai_webview::close_active_tab,
            $crate::commands::ai_webview::new_tab_like_active,
            $crate::commands::ai_webview::report_native_tabs,
            // === change_log ===
            $crate::commands::change_log::list_change_log,
            $crate::commands::change_log::undo_change,
            // === clipboard ===
            $crate::commands::clipboard::get_clipboard_text,
            $crate::commands::clipboard::get_clipboard_image,
            $crate::commands::clipboard::copy_chat_answer_if_unchanged,
            // === context ===
            $crate::commands::context::get_context_items,
            $crate::commands::context::get_context_text,
            $crate::commands::context::get_labelled_context,
            $crate::commands::context::has_context,
            $crate::commands::context::append_context,
            $crate::commands::context::clear_context,
            $crate::commands::context::remove_context_item,
            $crate::commands::context::append_context_image,
            $crate::commands::context::set_context_from_clipboard,
            $crate::commands::context::append_context_from_clipboard,
            // === context_editor ===
            $crate::commands::context_editor::open_context_editor,
            // === conversation_dialog ===
            $crate::commands::conversation_dialog::open_conversation_dialog,
            $crate::commands::conversation_dialog::focus_or_open_chat,
            $crate::commands::conversation_dialog::get_dialog_init_params,
            $crate::commands::conversation_dialog::get_chat_corners,
            // === dock ===
            $crate::commands::dock::hide_dialog_window,
            // === env_vars ===
            $crate::commands::env_vars::get_env_vars,
            $crate::commands::env_vars::apply_env_var_operation,
            // === execution_stream ===
            $crate::commands::execution_stream::execute_skill,
            $crate::commands::execution_stream::execute_conversation_from_tree,
            $crate::commands::execution_stream::release_conversation_context,
            $crate::commands::execution_stream::seed_conversation_context,
            $crate::commands::execution_stream::resolve_skill_input,
            // === execution_control ===
            $crate::commands::execution_control::reconnect_to_execution,
            $crate::commands::execution_control::cancel_skill_execution,
            $crate::commands::execution_control::cancel_live_execution,
            // === execution_generation ===
            $crate::commands::execution_generation::generate_conversation_title,
            // === history ===
            $crate::commands::history::get_history,
            $crate::commands::history::get_conversations,
            $crate::commands::history::get_history_entry,
            $crate::commands::history::add_conversation_entry,
            $crate::commands::history::update_conversation_entry,
            $crate::commands::history::get_last_interaction,
            $crate::commands::history::delete_history_entry,
            $crate::commands::history::get_history_storage_stats,
            $crate::commands::history::compact_history_database,
            $crate::commands::history::copy_last_interaction,
            $crate::commands::history::get_recent_quick_actions,
            $crate::commands::history::copy_history_entry_part,
            $crate::commands::history::copy_history_entry_content,
            $crate::commands::history::update_history_entry_title,
            $crate::commands::history::search_history,
            $crate::commands::history::list_history_skills,
            // === history_dialog ===
            $crate::commands::history_dialog::open_history_dialog,
            // === image_preview ===
            $crate::commands::image_preview::open_image_preview,
            $crate::commands::image_preview::get_pending_image,
            $crate::commands::image_preview::get_image_preview_work_area,
            // === mcp ===
            $crate::commands::mcp::list_mcp_tools,
            $crate::commands::mcp::list_mcp_servers,
            // === menu ===
            $crate::commands::menu::get_context_menu_items,
            $crate::commands::menu::execute_menu_item,
            $crate::commands::menu::show_context_menu_panel,
            $crate::commands::menu::place_context_menu,
            $crate::commands::menu::clamp_menu_origin,
            $crate::commands::menu::hide_context_menu_panel,
            $crate::commands::menu::focus_context_menu,
            // === notification ===
            $crate::commands::notification::update_notification_window,
            $crate::commands::notification::drain_pending_notifications,
            // === prompts ===
            $crate::commands::prompts::get_prompt,
            $crate::commands::prompts::save_prompt,
            $crate::commands::prompts::get_environment_placeholders,
            // === provider_menu ===
            $crate::commands::provider_menu::show_provider_menu,
            $crate::commands::provider_menu::hide_provider_menu,
            $crate::commands::provider_menu::size_provider_menu,
            $crate::commands::provider_menu::provider_menu_select,
            // === settings_agent ===
            $crate::commands::settings_agent::settings_agent_send,
            $crate::commands::settings_agent::settings_agent_cancel,
            $crate::commands::settings_agent::settings_agent_reset,
            $crate::commands::settings_agent::settings_agent_apply,
            $crate::commands::settings_agent::settings_agent_reject,
            $crate::commands::settings_agent::settings_agent_refresh,
            $crate::commands::settings_agent::settings_agent_test_row,
            $crate::commands::settings_agent::settings_agent_panel,
            // === settings ===
            $crate::commands::settings::get_settings,
            $crate::commands::settings::get_settings_catalog,
            $crate::commands::settings::get_settings_fallback,
            $crate::commands::settings::retry_settings_load,
            $crate::commands::settings::get_missing_env_refs,
            $crate::commands::settings::update_setting,
            $crate::commands::settings::add_shortcut,
            $crate::commands::settings::remove_shortcut,
            $crate::commands::settings::update_surface_model,
            $crate::commands::settings::update_surface_parameter,
            $crate::commands::settings::update_surface_enabled_tools,
            $crate::commands::settings::set_surface_tool,
            $crate::commands::settings::update_speech_to_text_config,
            $crate::commands::settings::add_model,
            $crate::commands::settings::update_model,
            $crate::commands::settings::delete_model,
            $crate::commands::settings::update_notifications,
            $crate::commands::settings::update_history_retention,
            // === settings_sync ===
            $crate::commands::settings_sync::get_settings_sync_status,
            $crate::commands::settings_sync::sync_settings_now,
            $crate::commands::settings_sync::sync_prepare,
            $crate::commands::settings_sync::sync_connect,
            $crate::commands::settings_sync::sync_clone_dir,
            // === setup ===
            $crate::commands::setup::get_setup_status,
            $crate::commands::setup::finish_setup,
            $crate::commands::setup::test_model,
            $crate::commands::setup::assign_setup_model,
            // === settings_dialog ===
            $crate::commands::settings_dialog::open_settings_window,
            $crate::commands::settings_dialog::save_secret,
            $crate::commands::settings_dialog::get_secret_value,
            // === skills ===
            $crate::commands::skills::list_skills,
            $crate::commands::skills::list_skills_full,
            $crate::commands::skills::get_skill,
            $crate::commands::skills::create_skill,
            $crate::commands::skills::update_skill,
            $crate::commands::skills::delete_skill,
            $crate::commands::skills::duplicate_skill,
            $crate::commands::skills::reorder_skills,
            $crate::commands::skills::validate_skill_slug,
            $crate::commands::skills::import_skill_file,
            $crate::commands::skills::export_skill,
            $crate::commands::skills::preview_skill_message,
            // === speech ===
            $crate::commands::speech::toggle_speech_recording,
            $crate::commands::speech::get_dictation_busy,
            $crate::commands::speech::start_composer_dictation,
            $crate::commands::speech::finish_composer_dictation,
            $crate::commands::speech::cancel_composer_dictation,
            $crate::commands::speech::retry_composer_dictation,
            $crate::commands::speech::dismiss_composer_dictation,
            $crate::commands::speech::pause_speech_recording,
            $crate::commands::speech::resume_speech_recording,
            $crate::commands::speech::cancel_speech_recording,
            $crate::commands::speech::select_recording_widget_target,
            $crate::commands::speech::set_recording_widget_dropdown,
            $crate::commands::speech::set_recording_widget_preview,
            $crate::commands::speech::set_recording_widget_tooltip,
            $crate::commands::speech::remove_recording_widget_item,
            $crate::commands::speech::paste_into_recording_widget,
            $crate::commands::speech::capture_screenshot_into_recording_widget,
            $crate::commands::speech::capture_selection_into_recording_widget,
            $crate::commands::speech::cancel_recording_widget_selection,
            $crate::commands::speech::capture_available,
            $crate::commands::speech::capture_screenshot_for_window,
            $crate::commands::speech::capture_selection_for_window,
            $crate::commands::speech::cancel_window_capture,
            $crate::commands::speech::stop_chat_speech_recording,
            $crate::commands::speech::get_recording_state,
            $crate::commands::speech::retry_transcription,
            $crate::commands::speech::get_audio_clip_info,
            $crate::commands::speech::export_audio_clip,
            $crate::commands::speech::discard_audio_clip,
            $crate::commands::speech::get_stt_keyterms,
            $crate::commands::speech::save_stt_keyterms,
            // === text_preview ===
            $crate::commands::text_preview::open_text_preview,
            $crate::commands::text_preview::get_pending_text,
            $crate::commands::text_preview::save_text_preview_geometry,
            // === tokenizer ===
            $crate::commands::tokenizer::count_tokens,
            $crate::commands::tokenizer::count_conversation_tokens,
            // === ui_state ===
            $crate::commands::ui_state::get_ui_state,
            $crate::commands::ui_state::set_ui_state,
        ]
    };
}
