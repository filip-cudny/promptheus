use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::sync::Mutex;

use crate::models::history::{
    HistoryContentPart, HistoryEntry, HistoryEntryType, ImagePayload, InvocationSource,
    OutputRoute, SerializedConversationNode,
};
use crate::services::clipboard::ClipboardService;
use crate::services::conversation_context::ConversationContextCache;
use crate::services::history_events::emit_history_changed;
use crate::services::history_search::{HistorySearch, SearchQuery, SearchResponse};
use crate::services::skill::{strip_skill_prefix, SkillService};
use crate::services::speech::{AudioClipStore, SpeechService};
use crate::services::sqlite_history::{HistoryStorageStats, SqliteHistoryService};

#[derive(Debug, Clone, Serialize)]
pub struct SkillCount {
    pub skill_id: String,
    pub skill_name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastInteractionData {
    pub last_text: Option<HistoryEntry>,
    pub last_speech: Option<HistoryEntry>,
}

const PREVIEW_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct RecentQuickAction {
    pub id: String,
    pub entry_type: HistoryEntryType,
    pub skill_id: Option<String>,
    pub skill_name: Option<String>,
    pub timestamp: String,
    pub input_preview: String,
    pub output_preview: Option<String>,
    pub status: String,
    pub has_audio: bool,
}

#[tauri::command]
pub async fn get_history(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
) -> crate::Result<Vec<HistoryEntry>> {
    Ok(history.lock().await.get_history())
}

#[tauri::command]
pub async fn get_conversations(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    offset: u32,
    limit: u32,
) -> crate::Result<Vec<HistoryEntry>> {
    Ok(history.lock().await.get_conversations(offset, limit))
}

#[tauri::command]
pub async fn get_history_entry(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    entry_id: String,
) -> crate::Result<Option<HistoryEntry>> {
    Ok(history.lock().await.get_entry_by_id(&entry_id))
}

#[tauri::command]
pub async fn add_conversation_entry(
    app: AppHandle,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    conversation_context: State<'_, Arc<Mutex<ConversationContextCache>>>,
    context_text: String,
    skill_id: Option<String>,
    skill_name: Option<String>,
    success: bool,
    error: Option<String>,
    nodes: Vec<SerializedConversationNode>,
    root_node_id: Option<String>,
    current_path: Vec<String>,
    tab_id: Option<String>,
    images: Vec<ImagePayload>,
    model_id: Option<String>,
    reasoning_effort: Option<String>,
    invocation_source: InvocationSource,
) -> crate::Result<String> {
    let resolved_environment_section = match tab_id.as_deref() {
        Some(id) => conversation_context
            .lock()
            .await
            .get(id)
            .map(|s| s.to_string()),
        None => None,
    };
    let id = history.lock().await.add_conversation_entry(
        context_text,
        skill_id,
        skill_name,
        success,
        error,
        nodes,
        root_node_id,
        current_path,
        false,
        resolved_environment_section,
        images,
        model_id,
        reasoning_effort,
        invocation_source,
        OutputRoute::Chat,
    );
    emit_history_changed(&app, Some(id.clone()), None)?;
    Ok(id)
}

#[tauri::command]
pub async fn update_conversation_entry(
    app: AppHandle,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    entry_id: String,
    context_text: String,
    nodes: Vec<SerializedConversationNode>,
    root_node_id: Option<String>,
    current_path: Vec<String>,
    images: Vec<ImagePayload>,
    model_id: Option<String>,
    reasoning_effort: Option<String>,
) -> crate::Result<()> {
    history.lock().await.update_conversation_entry(
        &entry_id,
        context_text,
        nodes,
        root_node_id,
        current_path,
        images,
        model_id,
        reasoning_effort,
    )?;
    emit_history_changed(&app, Some(entry_id), None)
}

#[tauri::command]
pub async fn get_last_interaction(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
) -> crate::Result<LastInteractionData> {
    let history = history.lock().await;
    Ok(LastInteractionData {
        last_text: history.get_last_quick_action(HistoryEntryType::Text),
        last_speech: history.get_last_quick_action(HistoryEntryType::Speech),
    })
}

#[tauri::command]
pub async fn update_history_entry_title(
    app: AppHandle,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    entry_id: String,
    title: String,
) -> crate::Result<()> {
    history.lock().await.update_entry_title(&entry_id, title)?;
    emit_history_changed(&app, Some(entry_id), None)
}

#[tauri::command]
pub async fn delete_history_entry(
    app: AppHandle,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    entry_id: String,
) -> crate::Result<()> {
    history.lock().await.delete_entry(&entry_id)?;
    emit_history_changed(&app, None, Some(entry_id))
}

#[tauri::command]
pub async fn get_history_storage_stats(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
) -> crate::Result<HistoryStorageStats> {
    Ok(history.lock().await.storage_stats())
}

#[tauri::command]
pub async fn compact_history_database(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
) -> crate::Result<HistoryStorageStats> {
    let history = history.lock().await;
    let stats = tokio::task::block_in_place(|| history.vacuum())?;
    Ok(stats)
}

#[tauri::command]
pub async fn search_history(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    history_search: State<'_, Arc<Mutex<HistorySearch>>>,
    query: SearchQuery,
) -> crate::Result<SearchResponse> {
    let history = history.lock().await;
    let mut search = history_search.lock().await;
    let response = search.run(&history, &query);
    log::debug!(
        target: "app_lib::history_search",
        "search_history: query='{}' type={:?} status={:?} skills={} date_from={:?} total={} returned={}",
        query.query,
        query.type_filter,
        query.status_filter,
        query.skill_ids.len(),
        query.date_from,
        response.total,
        response.results.len(),
    );
    Ok(response)
}

fn collect_skill_counts(entries: &[HistoryEntry]) -> Vec<SkillCount> {
    let mut map: HashMap<String, SkillCount> = HashMap::new();
    for e in entries.iter() {
        let id = e.skill_id.as_deref().filter(|s| !s.is_empty());
        let name = e.skill_name.as_deref().filter(|s| !s.is_empty());
        if let (Some(id), Some(name)) = (id, name) {
            map.entry(id.to_string())
                .and_modify(|c| c.count += 1)
                .or_insert(SkillCount {
                    skill_id: id.to_string(),
                    skill_name: name.to_string(),
                    count: 1,
                });
        }
    }
    let mut list: Vec<SkillCount> = map.into_values().collect();
    list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.skill_name.cmp(&b.skill_name)));
    list
}

#[tauri::command]
pub async fn list_history_skills(
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
) -> crate::Result<Vec<SkillCount>> {
    let entries = history.lock().await.get_history();
    let list = collect_skill_counts(&entries);
    log::debug!(
        target: "app_lib::history_search",
        "list_history_skills: returned {} unique skills",
        list.len(),
    );
    Ok(list)
}

/// Which chip of the context-menu "Last interaction" section is being copied.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastInteractionChip {
    Input,
    Output,
    Transcription,
}

/// Copies the full text behind a "Last interaction" chip. The menu only ever
/// receives previews, so the value is resolved here from the stored
/// conversation tree.
#[tauri::command]
pub async fn copy_last_interaction(
    clipboard: State<'_, ClipboardService>,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    skill_service: State<'_, Arc<Mutex<SkillService>>>,
    chip: LastInteractionChip,
) -> crate::Result<()> {
    let entry_type = match chip {
        LastInteractionChip::Transcription => HistoryEntryType::Speech,
        _ => HistoryEntryType::Text,
    };
    let part = match chip {
        LastInteractionChip::Input => HistoryContentPart::Input,
        _ => HistoryContentPart::Output,
    };

    let resolved = {
        let history = history.lock().await;
        history
            .get_last_quick_action(entry_type)
            .and_then(|entry| history.resolve_full_content(&entry.id, part))
    };

    let Some(content) = resolved else {
        return Ok(());
    };

    let content = match chip {
        LastInteractionChip::Input => {
            strip_skill_prefix(&content, &*skill_service.lock().await).to_string()
        }
        _ => content,
    };

    clipboard.set_text(&content)?;
    Ok(())
}

fn cap_preview(text: &str) -> String {
    if text.chars().count() <= PREVIEW_MAX_CHARS {
        return text.to_string();
    }
    let mut capped: String = text.chars().take(PREVIEW_MAX_CHARS - 1).collect();
    capped.push('…');
    capped
}

fn build_recent_quick_actions(
    entries: Vec<HistoryEntry>,
    retrying: &HashSet<String>,
    with_audio: &HashSet<String>,
    skill_service: &SkillService,
) -> Vec<RecentQuickAction> {
    entries
        .into_iter()
        .map(|entry| {
            let status = match (entry.success, retrying.contains(&entry.id)) {
                (true, _) => "ok",
                (false, true) => "retrying",
                (false, false) => "failed",
            };
            RecentQuickAction {
                input_preview: cap_preview(strip_skill_prefix(&entry.input_preview, skill_service)),
                output_preview: entry.output_preview.as_deref().map(cap_preview),
                timestamp: entry
                    .updated_at
                    .or(entry.created_at)
                    .unwrap_or(entry.timestamp),
                status: status.to_string(),
                has_audio: with_audio.contains(&entry.id),
                id: entry.id,
                entry_type: entry.entry_type,
                skill_id: entry.skill_id,
                skill_name: entry.skill_name,
            }
        })
        .collect()
}

/// Newest quick-action entries of both types, with previews only. `has_audio`
/// is set for failed speech rows whose recording is still on disk.
#[tauri::command]
pub async fn get_recent_quick_actions(
    speech: State<'_, Arc<Mutex<SpeechService>>>,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    skill_service: State<'_, Arc<Mutex<SkillService>>>,
    clips: State<'_, Arc<AudioClipStore>>,
    limit: u32,
) -> crate::Result<Vec<RecentQuickAction>> {
    let retrying = speech.lock().await.retrying_entries();

    let (entries, with_audio) = {
        let history = history.lock().await;
        let entries = history.get_recent_quick_actions(limit);
        let with_audio: HashSet<String> = entries
            .iter()
            .filter(|e| !e.success && e.entry_type == HistoryEntryType::Speech)
            .filter(|e| {
                clips
                    .find_by_entry(history.conn(), &e.id)
                    .is_some_and(|clip| clip.path.exists())
            })
            .map(|e| e.id.clone())
            .collect();
        (entries, with_audio)
    };

    let rows =
        build_recent_quick_actions(entries, &retrying, &with_audio, &*skill_service.lock().await);
    log::debug!("get_recent_quick_actions: returned {} rows", rows.len());
    Ok(rows)
}

fn part_clipboard_text(
    resolved: String,
    part: HistoryContentPart,
    skill_service: &SkillService,
) -> String {
    match part {
        HistoryContentPart::Input => strip_skill_prefix(&resolved, skill_service).to_string(),
        HistoryContentPart::Output => resolved,
    }
}

/// Copies the full text of one part of one entry. Returns `false` when there is
/// nothing to copy (unknown entry, or the Output of a failed row).
#[tauri::command]
pub async fn copy_history_entry_part(
    clipboard: State<'_, ClipboardService>,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    skill_service: State<'_, Arc<Mutex<SkillService>>>,
    entry_id: String,
    part: HistoryContentPart,
) -> crate::Result<bool> {
    let resolved = history.lock().await.resolve_copyable_part(&entry_id, part);

    let Some(resolved) = resolved else {
        return Ok(false);
    };

    let content = part_clipboard_text(resolved, part, &*skill_service.lock().await);
    clipboard.set_text(&content)?;
    Ok(true)
}

/// Copies a history entry's response, falling back to its prompt when the entry
/// has no response. Resolves the full text rather than the capped preview the
/// list view renders.
#[tauri::command]
pub async fn copy_history_entry_content(
    clipboard: State<'_, ClipboardService>,
    history: State<'_, Arc<Mutex<SqliteHistoryService>>>,
    entry_id: String,
) -> crate::Result<()> {
    let history = history.lock().await;
    let content = history
        .resolve_full_content(&entry_id, HistoryContentPart::Output)
        .or_else(|| history.resolve_full_content(&entry_id, HistoryContentPart::Input));

    if let Some(content) = content {
        clipboard.set_text(&content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::history::HistoryEntryType;
    use tempfile::TempDir;

    fn skill_service_with_translate() -> (TempDir, SkillService) {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("skills");
        let skill_dir = skills_dir.join("translate");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: translate\ndescription: desc\n---\n\nbody\n",
        )
        .unwrap();
        let service = SkillService::load(&skills_dir, &[]).unwrap();
        (dir, service)
    }

    fn build_one(entry: HistoryEntry, retrying: &[&str]) -> RecentQuickAction {
        let (_dir, skills) = skill_service_with_translate();
        let retrying = retrying.iter().map(|s| s.to_string()).collect();
        build_recent_quick_actions(vec![entry], &retrying, &HashSet::new(), &skills)
            .remove(0)
    }

    fn make_entry(id: &str, skill_id: Option<&str>, skill_name: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            timestamp: "2026-01-01 00:00:00".into(),
            input_preview: "input".into(),
            entry_type: HistoryEntryType::Text,
            output_preview: None,
            skill_id: skill_id.map(|s| s.to_string()),
            success: true,
            error: None,
            is_multi_turn: false,
            skill_name: skill_name.map(|s| s.to_string()),
            conversation_data: None,
            created_at: Some("2026-01-01 00:00:00".into()),
            updated_at: None,
            quick_action: false,
            title: None,
        }
    }

    #[test]
    fn collect_skill_counts_aggregates_and_sorts_by_count_desc_then_name_asc() {
        let entries = vec![
            make_entry("a", Some("translate"), Some("Translate")),
            make_entry("b", Some("translate"), Some("Translate")),
            make_entry("c", Some("translate"), Some("Translate")),
            make_entry("d", Some("rewrite"), Some("Rewrite")),
            make_entry("e", Some("rewrite"), Some("Rewrite")),
            make_entry("f", Some("summarize"), Some("Summarize")),
            make_entry("g", Some("expand"), Some("Expand")),
            make_entry("h", None, None),
            make_entry("i", Some("partial"), None),
            make_entry("j", None, Some("orphan")),
        ];

        let counts = collect_skill_counts(&entries);
        assert_eq!(counts.len(), 4);
        assert_eq!(counts[0].skill_id, "translate");
        assert_eq!(counts[0].count, 3);
        assert_eq!(counts[1].skill_id, "rewrite");
        assert_eq!(counts[1].count, 2);
        assert_eq!(counts[2].skill_name, "Expand");
        assert_eq!(counts[2].count, 1);
        assert_eq!(counts[3].skill_name, "Summarize");
        assert_eq!(counts[3].count, 1);
    }

    #[test]
    fn collect_skill_counts_empty_input_returns_empty_list() {
        let counts = collect_skill_counts(&[]);
        assert!(counts.is_empty());
    }

    #[test]
    fn collect_skill_counts_skips_entries_with_empty_skill_id_or_name() {
        let entries = vec![
            make_entry("legacy-chat", Some(""), Some("Chat")),
            make_entry("legacy-empty-name", Some("translate"), Some("")),
            make_entry("legacy-both-empty", Some(""), Some("")),
            make_entry("real", Some("translate"), Some("Translate")),
        ];

        let counts = collect_skill_counts(&entries);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].skill_id, "translate");
        assert_eq!(counts[0].skill_name, "Translate");
        assert_eq!(counts[0].count, 1);
    }

    #[test]
    fn recent_row_strips_a_known_skill_prefix_from_the_input() {
        let mut entry = make_entry("a", None, None);
        entry.input_preview = "/translate hello".into();

        assert_eq!(build_one(entry, &[]).input_preview, "hello");
    }

    #[test]
    fn recent_row_caps_previews_at_200_chars_including_multibyte() {
        let mut entry = make_entry("a", None, None);
        entry.input_preview = "ą".repeat(300);
        entry.output_preview = Some("ż€".repeat(150));

        let row = build_one(entry, &[]);

        assert_eq!(row.input_preview.chars().count(), 200);
        assert!(row.input_preview.ends_with('…'));
        let output = row.output_preview.unwrap();
        assert_eq!(output.chars().count(), 200);
        assert!(output.ends_with('…'));
    }

    #[test]
    fn recent_row_keeps_a_preview_of_exactly_200_chars_whole() {
        let mut entry = make_entry("a", None, None);
        entry.input_preview = "ą".repeat(200);

        assert_eq!(build_one(entry, &[]).input_preview, "ą".repeat(200));
    }

    #[test]
    fn recent_row_status_is_failed_retrying_or_ok() {
        let mut failed = make_entry("failed", None, None);
        failed.success = false;
        let mut retrying = make_entry("retrying", None, None);
        retrying.success = false;
        let ok = make_entry("ok", None, None);

        assert_eq!(build_one(failed, &["retrying"]).status, "failed");
        assert_eq!(build_one(retrying, &["retrying"]).status, "retrying");
        assert_eq!(build_one(ok, &["ok"]).status, "ok");
    }

    #[test]
    fn recent_row_timestamp_prefers_updated_at_then_created_at_then_timestamp() {
        let mut entry = make_entry("a", None, None);
        entry.updated_at = Some("2026-02-02 00:00:00".into());
        assert_eq!(build_one(entry.clone(), &[]).timestamp, "2026-02-02 00:00:00");

        entry.updated_at = None;
        assert_eq!(build_one(entry.clone(), &[]).timestamp, "2026-01-01 00:00:00");

        entry.created_at = None;
        entry.timestamp = "2026-03-03 00:00:00".into();
        assert_eq!(build_one(entry, &[]).timestamp, "2026-03-03 00:00:00");
    }

    #[test]
    fn part_clipboard_text_strips_the_prefix_for_input_only() {
        let (_dir, skills) = skill_service_with_translate();

        assert_eq!(
            part_clipboard_text("/translate hello".into(), HistoryContentPart::Input, &skills),
            "hello",
        );
        assert_eq!(
            part_clipboard_text("/translate hello".into(), HistoryContentPart::Output, &skills),
            "/translate hello",
        );
    }
}
