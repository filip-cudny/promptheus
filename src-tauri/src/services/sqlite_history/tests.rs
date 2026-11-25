use super::*;
use crate::models::history::{InvocationSource, OutputRoute};
use crate::services::database::Database;

impl SqliteHistoryService {
    fn get_last_item_by_type(&self, entry_type: HistoryEntryType) -> Option<HistoryEntry> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM conversations WHERE entry_type = ?1
             ORDER BY COALESCE(updated_at, created_at) DESC, rowid DESC LIMIT 1"
        );
        self.db.conn().query_row(&sql, [entry_type_str(&entry_type)], row_to_entry).ok()
    }

    fn clear(&self) {
        let _ = self.db.conn().execute("DELETE FROM conversations", []);
    }

    fn entry_count(&self) -> usize {
        self.db
            .conn()
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize
    }
}

fn make_db() -> Database {
    Database::open_in_memory().unwrap()
}

fn make_svc() -> SqliteHistoryService {
    SqliteHistoryService::new(make_db(), 0)
}

fn backdate(svc: &SqliteHistoryService, entry_id: &str, days: u32) {
    svc.conn()
        .execute(
            "UPDATE conversations
             SET created_at = datetime('now', 'localtime', ?2), updated_at = NULL
             WHERE id = ?1",
            rusqlite::params![entry_id, format!("-{} days", days)],
        )
        .unwrap();
}

fn add_plain(svc: &SqliteHistoryService, text: &str) -> String {
    svc.add_entry(
        text.into(),
        HistoryEntryType::Text,
        None,
        None,
        true,
        None,
        false,
        None,
        false,
        None,
    )
    .unwrap()
}

fn make_nodes(user_text: &str, assistant_text: &str) -> Vec<SerializedConversationNode> {
    vec![
        SerializedConversationNode {
            node_id: "u1".into(),
            parent_id: None,
            role: "user".into(),
            content: user_text.into(),
            timestamp: "2026-01-01".into(),
            children: vec!["a1".into()],
            updates: vec![],
            prompt_tokens: None,
            completion_tokens: None,
            thinking: None,
            thinking_duration: None,
            query_duration: None,
            error: None,
            cancelled: false,
            tool_calls: vec![],
            text_attachments: vec![],
            sources: vec![],
            applied_skills: vec![],
        },
        SerializedConversationNode {
            node_id: "a1".into(),
            parent_id: Some("u1".into()),
            role: "assistant".into(),
            content: assistant_text.into(),
            timestamp: "2026-01-01".into(),
            children: vec![],
            updates: vec![],
            prompt_tokens: None,
            completion_tokens: None,
            thinking: None,
            thinking_duration: None,
            query_duration: None,
            error: None,
            cancelled: false,
            tool_calls: vec![],
            text_attachments: vec![],
            sources: vec![],
            applied_skills: vec![],
        },
    ]
}

#[test]
fn add_and_get_history() {
    let svc = make_svc();
    svc.add_entry(
        "hello".into(),
        HistoryEntryType::Text,
        Some("world".into()),
        None,
        true,
        None,
        false,
        None,
        false,
        None,
    );
    let history = svc.get_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].input_preview, "hello");
}

#[test]
fn add_conversation_and_restore() {
    let svc = make_svc();
    let nodes = make_nodes("hi", "hello");
    let id = svc.add_conversation_entry(
        "context".into(),
        None,
        None,
        true,
        None,
        nodes,
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        false,
        None,
        vec![],
        None,
        None,
        InvocationSource::ContextMenu,
        OutputRoute::Clipboard,
    );

    let entry = svc.get_entry_by_id(&id).unwrap();
    assert!(entry.is_multi_turn);
    let conv = entry.conversation_data.unwrap();
    assert_eq!(conv.nodes.len(), 2);
    assert_eq!(conv.root_node_id, Some("u1".into()));
    assert_eq!(conv.current_path, vec!["u1", "a1"]);
    assert_eq!(conv.context_text, "context");
}

#[test]
fn resolve_full_content_returns_text_beyond_the_capped_preview() {
    let svc = make_svc();
    let long_answer = "z".repeat(5000);
    let long_prompt = "y".repeat(5000);
    let id = svc.add_conversation_entry(
        "context".into(),
        None,
        None,
        true,
        None,
        make_nodes(&long_prompt, &long_answer),
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        true,
        None,
        vec![],
        None,
        None,
        InvocationSource::ContextMenu,
        OutputRoute::Clipboard,
    );

    let entry = svc.get_last_quick_action(HistoryEntryType::Text).unwrap();
    assert_eq!(entry.input_preview.chars().count(), 200);
    assert_eq!(entry.output_preview.as_ref().unwrap().chars().count(), 200);

    assert_eq!(
        svc.resolve_full_content(&id, HistoryContentPart::Output),
        Some(long_answer),
    );
    assert_eq!(
        svc.resolve_full_content(&id, HistoryContentPart::Input),
        Some(long_prompt),
    );
}

#[test]
fn resolve_full_content_falls_back_to_columns_for_simple_entries() {
    let svc = make_svc();
    let id = svc
        .add_entry(
            "spoken input".into(),
            HistoryEntryType::Speech,
            Some("full transcript".into()),
            None,
            true,
            None,
            false,
            None,
            true,
            None,
        )
        .unwrap();

    assert_eq!(
        svc.resolve_full_content(&id, HistoryContentPart::Output),
        Some("full transcript".into()),
    );
    assert_eq!(
        svc.resolve_full_content(&id, HistoryContentPart::Input),
        Some("spoken input".into()),
    );
    assert_eq!(svc.resolve_full_content("missing", HistoryContentPart::Output), None);
}

#[test]
fn update_conversation_entry() {
    let svc = make_svc();
    let nodes = make_nodes("hi", "hello");
    let id = svc.add_conversation_entry(
        "ctx".into(),
        None,
        None,
        true,
        None,
        nodes,
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        false,
        None,
        vec![],
        None,
        None,
        InvocationSource::ContextMenu,
        OutputRoute::Clipboard,
    );

    let new_nodes = make_nodes("hi updated", "hello updated");
    svc.update_conversation_entry(
        &id,
        "new ctx".into(),
        new_nodes,
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        vec![],
        None,
        None,
    )
    .unwrap();

    let entry = svc.get_entry_by_id(&id).unwrap();
    assert!(entry.updated_at.is_some());
    let conv = entry.conversation_data.unwrap();
    assert_eq!(conv.context_text, "new ctx");
    assert_eq!(conv.nodes[0].content, "hi updated");
}

#[test]
fn update_nonexistent_entry_fails() {
    let svc = make_svc();
    let result = svc.update_conversation_entry(
        "nonexistent",
        "ctx".into(),
        vec![],
        None,
        vec![],
        vec![],
        None,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn retention_prunes_only_entries_past_cutoff() {
    let svc = SqliteHistoryService::new(make_db(), 30);
    let stale = add_plain(&svc, "stale");
    let borderline = add_plain(&svc, "borderline");
    add_plain(&svc, "fresh");
    backdate(&svc, &stale, 60);
    backdate(&svc, &borderline, 29);

    assert_eq!(svc.enforce_retention(), 1);
    assert_eq!(svc.entry_count(), 2);
    assert!(svc.get_entry_by_id(&stale).is_none());
    assert!(svc.get_entry_by_id(&borderline).is_some());
}

#[test]
fn retention_disabled_keeps_everything() {
    let svc = SqliteHistoryService::new(make_db(), 0);
    let ancient = add_plain(&svc, "ancient");
    backdate(&svc, &ancient, 3000);

    assert_eq!(svc.enforce_retention(), 0);
    assert_eq!(svc.entry_count(), 1);
}

#[test]
fn retention_respects_updated_at_over_created_at() {
    let svc = SqliteHistoryService::new(make_db(), 30);
    let touched = add_plain(&svc, "touched");
    svc.conn()
        .execute(
            "UPDATE conversations
             SET created_at = datetime('now', 'localtime', '-90 days'),
                 updated_at = datetime('now', 'localtime', '-1 days')
             WHERE id = ?1",
            rusqlite::params![touched],
        )
        .unwrap();

    assert_eq!(svc.enforce_retention(), 0);
    assert!(svc.get_entry_by_id(&touched).is_some());
}

#[test]
fn adding_an_entry_does_not_trigger_a_sweep() {
    let mut svc = SqliteHistoryService::new(make_db(), 30);
    let stale = add_plain(&svc, "stale");
    backdate(&svc, &stale, 60);

    add_plain(&svc, "fresh");
    assert_eq!(svc.entry_count(), 2);

    svc.set_retention_days(30);
    assert_eq!(svc.enforce_retention(), 1);
    assert_eq!(svc.entry_count(), 1);
}

#[test]
fn get_entry_by_id_found_and_not_found() {
    let svc = make_svc();
    svc.add_entry(
        "test".into(),
        HistoryEntryType::Text,
        None,
        None,
        true,
        None,
        false,
        None,
        false,
        None,
    );
    let entry = svc.get_history().into_iter().next().unwrap();
    assert!(svc.get_entry_by_id(&entry.id).is_some());
    assert!(svc.get_entry_by_id("nonexistent").is_none());
}

#[test]
fn get_last_item_by_type() {
    let svc = make_svc();
    svc.add_entry("t1".into(), HistoryEntryType::Text, None, None, true, None, false, None, false, None);
    svc.add_entry("s1".into(), HistoryEntryType::Speech, None, None, true, None, false, None, false, None);
    svc.add_entry("t2".into(), HistoryEntryType::Text, None, None, true, None, false, None, false, None);

    let last_text = svc.get_last_item_by_type(HistoryEntryType::Text).unwrap();
    assert_eq!(last_text.input_preview, "t2");

    let last_speech = svc.get_last_item_by_type(HistoryEntryType::Speech).unwrap();
    assert_eq!(last_speech.input_preview, "s1");
}

#[test]
fn update_title() {
    let svc = make_svc();
    svc.add_entry("test".into(), HistoryEntryType::Text, None, None, true, None, false, None, false, None);
    let entry = svc.get_history().into_iter().next().unwrap();

    svc.update_entry_title(&entry.id, "My Title".into()).unwrap();
    let updated = svc.get_entry_by_id(&entry.id).unwrap();
    assert_eq!(updated.title, Some("My Title".into()));
}

#[test]
fn clear_removes_all() {
    let svc = make_svc();
    svc.add_entry("test".into(), HistoryEntryType::Text, None, None, true, None, false, None, false, None);
    assert_eq!(svc.entry_count(), 1);
    svc.clear();
    assert_eq!(svc.entry_count(), 0);
}

#[test]
fn image_round_trip() {
    let svc = make_svc();
    let nodes = make_nodes("hi", "hello");
    let image_data = BASE64.encode(b"fake png data");

    let id = svc.add_conversation_entry(
        "ctx".into(),
        None,
        None,
        true,
        None,
        nodes,
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        false,
        None,
        vec![
            ImagePayload {
                node_id: Some("u1".into()),
                image_index: 0,
                data: image_data.clone(),
                media_type: "image/png".into(),
            },
            ImagePayload {
                node_id: None,
                image_index: 0,
                data: image_data.clone(),
                media_type: "image/jpeg".into(),
            },
        ],
        None,
        None,
        InvocationSource::ContextMenu,
        OutputRoute::Clipboard,
    );

    let entry = svc.get_entry_by_id(&id).unwrap();
    let conv = entry.conversation_data.unwrap();

    let node_imgs = conv.node_images.get("u1").unwrap();
    assert_eq!(node_imgs.len(), 1);
    assert_eq!(node_imgs[0].data, image_data);
    assert_eq!(node_imgs[0].media_type, "image/png");

    assert_eq!(conv.context_images.len(), 1);
    assert_eq!(conv.context_images[0].data, image_data);
    assert_eq!(conv.context_images[0].media_type, "image/jpeg");
}

#[test]
fn context_images_read_back_with_numbers_from_image_index() {
    let svc = make_svc();
    let nodes = make_nodes("hi", "hello");
    let image_data = BASE64.encode(b"data");
    let ctx = |image_index| ImagePayload {
        node_id: None,
        image_index,
        data: image_data.clone(),
        media_type: "image/png".into(),
    };

    let save = |images: Vec<ImagePayload>| {
        svc.add_conversation_entry(
            "ctx".into(),
            None,
            None,
            true,
            None,
            nodes.clone(),
            Some("u1".into()),
            vec!["u1".into(), "a1".into()],
            false,
            None,
            images,
            None,
            None,
            InvocationSource::ContextMenu,
            OutputRoute::Clipboard,
        )
    };

    let id = save(vec![ctx(1), ctx(4)]);
    let conv = svc.get_entry_by_id(&id).unwrap().conversation_data.unwrap();
    let numbers: Vec<_> = conv.context_images.iter().map(|i| i.number).collect();
    assert_eq!(numbers, [Some(2), Some(5)]);

    let id = save(vec![ctx(0)]);
    let conv = svc.get_entry_by_id(&id).unwrap().conversation_data.unwrap();
    assert_eq!(conv.context_images[0].number, Some(1));
}

#[test]
fn image_cascade_delete() {
    let svc = make_svc();
    let nodes = make_nodes("hi", "hello");
    let image_data = BASE64.encode(b"data");

    svc.add_conversation_entry(
        "ctx".into(),
        None,
        None,
        true,
        None,
        nodes,
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        false,
        None,
        vec![ImagePayload {
            node_id: Some("u1".into()),
            image_index: 0,
            data: image_data,
            media_type: "image/png".into(),
        }],
        None,
        None,
        InvocationSource::ContextMenu,
        OutputRoute::Clipboard,
    );

    svc.clear();

    let count: i64 = svc
        .db
        .conn()
        .query_row("SELECT COUNT(*) FROM conversation_images", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn quick_action_query() {
    let svc = make_svc();
    svc.add_entry("normal".into(), HistoryEntryType::Text, None, None, true, None, false, None, false, None);
    svc.add_entry("quick".into(), HistoryEntryType::Text, None, None, true, None, false, None, true, None);

    let last_quick = svc.get_last_quick_action(HistoryEntryType::Text).unwrap();
    assert_eq!(last_quick.input_preview, "quick");
}

fn add_quick(svc: &SqliteHistoryService, text: &str, entry_type: HistoryEntryType) -> String {
    svc.add_entry(text.into(), entry_type, None, None, true, None, false, None, true, None)
        .unwrap()
}

fn set_timestamps(svc: &SqliteHistoryService, id: &str, created_at: &str, updated_at: Option<&str>) {
    svc.conn()
        .execute(
            "UPDATE conversations SET created_at = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, created_at, updated_at],
        )
        .unwrap();
}

#[test]
fn get_recent_quick_actions_returns_only_quick_rows_of_both_types() {
    let svc = make_svc();
    add_plain(&svc, "plain text");
    svc.add_entry("plain speech".into(), HistoryEntryType::Speech, None, None, true, None, false, None, false, None);
    add_quick(&svc, "quick text", HistoryEntryType::Text);
    add_quick(&svc, "quick speech", HistoryEntryType::Speech);

    let inputs: Vec<String> = svc
        .get_recent_quick_actions(10)
        .into_iter()
        .map(|e| e.input_preview)
        .collect();

    assert_eq!(inputs, vec!["quick speech", "quick text"]);
}

#[test]
fn get_recent_quick_actions_orders_by_updated_at_then_created_at() {
    let svc = make_svc();
    let oldest = add_quick(&svc, "oldest", HistoryEntryType::Text);
    let middle = add_quick(&svc, "middle", HistoryEntryType::Speech);
    let newest = add_quick(&svc, "newest", HistoryEntryType::Text);
    set_timestamps(&svc, &oldest, "2026-01-01 10:00:00", Some("2026-01-03 10:00:00"));
    set_timestamps(&svc, &middle, "2026-01-02 10:00:00", None);
    set_timestamps(&svc, &newest, "2026-01-01 12:00:00", None);

    let ids: Vec<String> = svc.get_recent_quick_actions(10).into_iter().map(|e| e.id).collect();

    assert_eq!(ids, vec![oldest, middle, newest]);
}

#[test]
fn get_recent_quick_actions_breaks_equal_timestamps_by_later_insert() {
    let svc = make_svc();
    let first = add_quick(&svc, "first", HistoryEntryType::Text);
    let second = add_quick(&svc, "second", HistoryEntryType::Speech);
    set_timestamps(&svc, &first, "2026-01-01 10:00:00", None);
    set_timestamps(&svc, &second, "2026-01-01 10:00:00", None);

    let ids: Vec<String> = svc.get_recent_quick_actions(10).into_iter().map(|e| e.id).collect();

    assert_eq!(ids, vec![second, first]);
}

#[test]
fn get_recent_quick_actions_returns_at_most_limit_rows() {
    let svc = make_svc();
    for i in 0..5 {
        add_quick(&svc, &format!("entry {i}"), HistoryEntryType::Text);
    }

    assert_eq!(svc.get_recent_quick_actions(3).len(), 3);
    assert_eq!(svc.get_recent_quick_actions(0).len(), 0);
}

fn add_failed_speech(svc: &SqliteHistoryService) -> String {
    svc.add_entry(
        "Voice note · 0:07 — transcription failed".into(),
        HistoryEntryType::Speech,
        None,
        None,
        false,
        Some("network".into()),
        false,
        None,
        true,
        None,
    )
    .unwrap()
}

#[test]
fn resolve_copyable_part_returns_full_output_beyond_the_capped_preview() {
    let svc = make_svc();
    let long_answer = "z".repeat(5000);
    let id = svc.add_conversation_entry(
        "context".into(),
        None,
        None,
        true,
        None,
        make_nodes("prompt", &long_answer),
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        true,
        None,
        vec![],
        None,
        None,
        InvocationSource::ContextMenu,
        OutputRoute::Clipboard,
    );

    assert_eq!(
        svc.resolve_copyable_part(&id, HistoryContentPart::Output),
        Some(long_answer),
    );
}

#[test]
fn resolve_copyable_part_has_no_output_for_a_failed_row_but_keeps_its_input() {
    let svc = make_svc();
    let id = add_failed_speech(&svc);

    assert_eq!(svc.resolve_copyable_part(&id, HistoryContentPart::Output), None);
    assert_eq!(
        svc.resolve_copyable_part(&id, HistoryContentPart::Input),
        Some("Voice note · 0:07 — transcription failed".to_string()),
    );
}

#[test]
fn vacuum_reclaims_free_pages_after_bulk_delete() {
    let svc = SqliteHistoryService::new(make_db(), 0);
    for i in 0..200 {
        add_plain(&svc, &format!("{}-{}", i, "x".repeat(4096)));
    }
    let filled = svc.storage_stats();
    svc.clear();

    let before = svc.storage_stats();
    assert!(before.reclaimable_bytes > 0);
    assert_eq!(svc.entry_count(), 0);

    let after = svc.vacuum().unwrap();
    assert_eq!(after.reclaimable_bytes, 0);
    assert!(after.database_bytes < filled.database_bytes);
}

#[test]
fn compact_after_bulk_delete_skips_small_databases() {
    let svc = SqliteHistoryService::new(make_db(), 0);
    let stale = add_plain(&svc, "stale");
    backdate(&svc, &stale, 60);
    svc.clear();

    assert!(!svc.compact_after_bulk_delete());
}

fn add_with_source(
    svc: &SqliteHistoryService,
    invocation_source: InvocationSource,
    output_route: OutputRoute,
) -> String {
    svc.add_conversation_entry(
        String::new(),
        None,
        None,
        true,
        None,
        make_nodes("hi", "hello"),
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        false,
        None,
        vec![],
        None,
        None,
        invocation_source,
        output_route,
    )
}

fn read_source_columns(svc: &SqliteHistoryService, id: &str) -> (Option<String>, Option<String>) {
    svc.conn()
        .query_row(
            "SELECT invocation_source, output_route FROM conversations WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn add_entry_stores_the_given_output_route_and_null_without_one() {
    let svc = make_svc();
    let add = |route| {
        svc.add_entry("s".into(), HistoryEntryType::Speech, None, None, true, None, false, None, true, route)
            .unwrap()
    };
    let routed = add(Some("claude_desktop"));
    let unrouted = add(None);

    assert_eq!(read_source_columns(&svc, &routed), (None, Some("claude_desktop".into())));
    assert_eq!(read_source_columns(&svc, &unrouted), (None, None));
}

#[test]
fn add_conversation_entry_stores_alternative_execute_and_clipboard() {
    let svc = make_svc();
    let id = add_with_source(&svc, InvocationSource::AlternativeExecute, OutputRoute::Clipboard);
    assert_eq!(
        read_source_columns(&svc, &id),
        (Some("alternative_execute".into()), Some("clipboard".into()))
    );
}

#[test]
fn add_conversation_entry_stores_chat_and_chat() {
    let svc = make_svc();
    let id = add_with_source(&svc, InvocationSource::Chat, OutputRoute::Chat);
    assert_eq!(
        read_source_columns(&svc, &id),
        (Some("chat".into()), Some("chat".into()))
    );
}

#[test]
fn update_conversation_entry_keeps_source_columns() {
    let svc = make_svc();
    let id = add_with_source(&svc, InvocationSource::ContextMenu, OutputRoute::Clipboard);
    svc.update_conversation_entry(
        &id,
        "new ctx".into(),
        make_nodes("hi updated", "hello updated"),
        Some("u1".into()),
        vec!["u1".into(), "a1".into()],
        vec![],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        read_source_columns(&svc, &id),
        (Some("context_menu".into()), Some("clipboard".into()))
    );
}

#[test]
fn speech_entry_has_null_source_columns() {
    let svc = make_svc();
    let id = add_plain(&svc, "spoken");
    assert_eq!(read_source_columns(&svc, &id), (None, None));
}
