mod codec;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::models::history::{
    ConversationHistoryData, HistoryContentPart, HistoryEntry, HistoryEntryType, ImagePayload,
    InvocationSource, OutputRoute, SerializedConversationNode,
};
use crate::models::message::ImageData;
use crate::services::database::{Database, DB_TIMESTAMP_FORMAT};
use serde::Serialize;
use crate::services::history_search::{HistoryStatusFilter, HistoryTypeFilter};

use codec::{
    build_applied_skill_names, build_input_summary, build_output_summary, last_role_content,
    row_to_entry, TreeJson, ENTRY_COLUMNS,
};
pub(crate) use codec::entry_type_str;

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("Entry not found: {0}")]
    EntryNotFound(String),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
}

const VACUUM_MIN_RECLAIMABLE_BYTES: u64 = 100 * 1024 * 1024;
const VACUUM_MIN_FREELIST_RATIO: f64 = 0.25;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HistoryStorageStats {
    pub database_bytes: u64,
    pub reclaimable_bytes: u64,
}

pub struct SqliteHistoryService {
    db: Database,
    retention_days: u32,
}

impl SqliteHistoryService {
    /// `retention_days` of `0` disables pruning and keeps history forever.
    pub fn new(db: Database, retention_days: u32) -> Self {
        Self { db, retention_days }
    }

    pub fn set_retention_days(&mut self, retention_days: u32) {
        self.retention_days = retention_days;
    }

    pub fn add_entry(
        &self,
        input_content: String,
        entry_type: HistoryEntryType,
        output_content: Option<String>,
        skill_id: Option<String>,
        success: bool,
        error: Option<String>,
        is_multi_turn: bool,
        skill_name: Option<String>,
        quick_action: bool,
        output_route: Option<&str>,
    ) -> Option<String> {
        let id = Self::generate_id();
        let now = Self::now_timestamp();

        let result = self.db.conn().execute(
            "INSERT INTO conversations (id, entry_type, input_content, output_content, skill_id, skill_name, success, error, is_multi_turn, quick_action, created_at, output_route)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                id,
                entry_type_str(&entry_type),
                input_content,
                output_content,
                skill_id,
                skill_name,
                success,
                error,
                is_multi_turn,
                quick_action,
                now,
                output_route,
            ],
        );

        if let Err(e) = result {
            log::error!("failed to add history entry: {}", e);
            return None;
        }

        Some(id)
    }

    pub fn add_conversation_entry(
        &self,
        context_text: String,
        skill_id: Option<String>,
        skill_name: Option<String>,
        success: bool,
        error: Option<String>,
        nodes: Vec<SerializedConversationNode>,
        root_node_id: Option<String>,
        current_path: Vec<String>,
        quick_action: bool,
        resolved_environment_section: Option<String>,
        images: Vec<ImagePayload>,
        model_id: Option<String>,
        reasoning_effort: Option<String>,
        invocation_source: InvocationSource,
        output_route: OutputRoute,
    ) -> String {
        let id = Self::generate_id();
        let now = Self::now_timestamp();

        let input_summary = build_input_summary(&nodes);
        let output_summary = build_output_summary(&nodes);
        let applied_skill_names = build_applied_skill_names(&nodes);

        let tree_json = serde_json::to_string(&TreeJson {
            nodes,
            root_node_id: root_node_id.clone(),
            current_path: current_path.clone(),
            resolved_environment_section,
            model_id,
            reasoning_effort,
        })
        .unwrap_or_default();

        let conn = self.db.conn();
        let tx = conn.unchecked_transaction().unwrap();

        tx.execute(
            "INSERT INTO conversations (id, entry_type, input_content, output_content, skill_id, skill_name, success, error, is_multi_turn, quick_action, context_text, tree_json, created_at, applied_skill_names, invocation_source, output_route)
             VALUES (?1, 'text', ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                id,
                input_summary,
                output_summary,
                skill_id,
                skill_name,
                success,
                error,
                quick_action,
                context_text,
                tree_json,
                now,
                applied_skill_names,
                invocation_source.as_str(),
                output_route.as_str(),
            ],
        )
        .unwrap();

        self.insert_images(&tx, &id, &images);

        tx.commit().unwrap();
        id
    }

    pub fn update_conversation_entry(
        &self,
        entry_id: &str,
        context_text: String,
        nodes: Vec<SerializedConversationNode>,
        root_node_id: Option<String>,
        current_path: Vec<String>,
        images: Vec<ImagePayload>,
        model_id: Option<String>,
        reasoning_effort: Option<String>,
    ) -> Result<(), HistoryError> {
        let now = Self::now_timestamp();

        let input_summary = build_input_summary(&nodes);
        let output_summary = build_output_summary(&nodes);
        let applied_skill_names = build_applied_skill_names(&nodes);

        let tree_json = serde_json::to_string(&TreeJson {
            nodes,
            root_node_id,
            current_path,
            resolved_environment_section: None,
            model_id,
            reasoning_effort,
        })
        .unwrap_or_default();

        let conn = self.db.conn();
        let tx = conn.unchecked_transaction()?;

        let updated = tx.execute(
            "UPDATE conversations SET input_content = ?1, output_content = ?2, context_text = ?3, tree_json = ?4, updated_at = ?5, applied_skill_names = ?6 WHERE id = ?7",
            rusqlite::params![input_summary, output_summary, context_text, tree_json, now, applied_skill_names, entry_id],
        )?;

        if updated == 0 {
            return Err(HistoryError::EntryNotFound(entry_id.to_string()));
        }

        tx.execute(
            "DELETE FROM conversation_images WHERE conversation_id = ?1",
            [entry_id],
        )?;

        self.insert_images(&tx, entry_id, &images);

        tx.commit()?;
        Ok(())
    }

    pub fn get_conversations(&self, offset: u32, limit: u32) -> Vec<HistoryEntry> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM conversations
             WHERE quick_action = 0
             ORDER BY COALESCE(updated_at, created_at) DESC, rowid DESC
             LIMIT ?1 OFFSET ?2"
        );
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map(rusqlite::params![limit, offset], row_to_entry)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    pub fn get_history(&self) -> Vec<HistoryEntry> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM conversations
             ORDER BY COALESCE(updated_at, created_at) DESC, rowid DESC"
        );
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map([], row_to_entry)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    pub fn get_entry_by_id(&self, id: &str) -> Option<HistoryEntry> {
        let conn = self.db.conn();
        let sql = format!(
            "SELECT {ENTRY_COLUMNS}, context_text, tree_json FROM conversations WHERE id = ?1"
        );
        let mut stmt = conn.prepare(&sql).ok()?;

        let entry = stmt
            .query_row([id], |row| {
                let context_text: String = row.get::<_, Option<String>>(13)?.unwrap_or_default();
                let tree_json_str: Option<String> = row.get(14)?;

                let conversation_data = tree_json_str
                    .and_then(|json| serde_json::from_str::<TreeJson>(&json).ok())
                    .map(|tree| ConversationHistoryData {
                        context_text: context_text.clone(),
                        nodes: tree.nodes,
                        root_node_id: tree.root_node_id,
                        current_path: tree.current_path,
                        resolved_environment_section: tree.resolved_environment_section,
                        node_images: HashMap::new(),
                        context_images: vec![],
                        model_id: tree.model_id,
                        reasoning_effort: tree.reasoning_effort,
                    });

                Ok((
                    row_to_entry(row)?,
                    conversation_data,
                ))
            })
            .ok()?;

        let (mut history_entry, conversation_data) = entry;

        if let Some(mut conv_data) = conversation_data {
            let mut img_stmt = conn
                .prepare(
                    "SELECT node_id, data, mime_type, image_index FROM conversation_images
                     WHERE conversation_id = ?1 ORDER BY image_index",
                )
                .ok()?;

            let images: Vec<(Option<String>, Vec<u8>, String, u32)> = img_stmt
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
                .ok()?
                .filter_map(|r| r.ok())
                .collect();

            for (node_id, data, mime_type, image_index) in images {
                let mut image_data = ImageData {
                    data: BASE64.encode(&data),
                    media_type: mime_type,
                    number: None,
                };
                match node_id {
                    Some(nid) => conv_data
                        .node_images
                        .entry(nid)
                        .or_default()
                        .push(image_data),
                    None => {
                        image_data.number = Some(image_index + 1);
                        conv_data.context_images.push(image_data);
                    }
                }
            }

            history_entry.conversation_data = Some(conv_data);
        }

        Some(history_entry)
    }

    pub fn get_last_quick_action(&self, entry_type: HistoryEntryType) -> Option<HistoryEntry> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM conversations WHERE entry_type = ?1 AND quick_action = 1
             ORDER BY COALESCE(updated_at, created_at) DESC, rowid DESC LIMIT 1"
        );
        self.db.conn().query_row(&sql, [entry_type_str(&entry_type)], row_to_entry).ok()
    }

    pub fn get_recent_quick_actions(&self, limit: u32) -> Vec<HistoryEntry> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM conversations WHERE quick_action = 1
             ORDER BY COALESCE(updated_at, created_at) DESC, rowid DESC LIMIT ?1"
        );
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql).unwrap();
        stmt.query_map([limit], row_to_entry)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Like `resolve_full_content`, but a failed row has no copyable Output, so
    /// a failed recording's placeholder never reaches the clipboard.
    pub fn resolve_copyable_part(
        &self,
        entry_id: &str,
        part: HistoryContentPart,
    ) -> Option<String> {
        if part == HistoryContentPart::Output {
            let success: bool = self
                .db
                .conn()
                .query_row(
                    "SELECT success FROM conversations WHERE id = ?1",
                    [entry_id],
                    |row| row.get(0),
                )
                .ok()?;
            if !success {
                return None;
            }
        }
        self.resolve_full_content(entry_id, part)
    }

    /// Full stored text of one side of an entry, bypassing the capped
    /// `input_preview` / `output_preview` columns. Reads the conversation tree
    /// without loading attached images; falls back to the preview column for
    /// simple entries, which have no tree.
    pub fn resolve_full_content(
        &self,
        entry_id: &str,
        part: HistoryContentPart,
    ) -> Option<String> {
        let (input_preview, output_preview, tree_json): (String, Option<String>, Option<String>) =
            self.db
                .conn()
                .query_row(
                    "SELECT input_content, output_content, tree_json FROM conversations WHERE id = ?1",
                    [entry_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok()?;

        let role = match part {
            HistoryContentPart::Input => "user",
            HistoryContentPart::Output => "assistant",
        };

        let from_tree = tree_json
            .and_then(|json| serde_json::from_str::<TreeJson>(&json).ok())
            .and_then(|tree| last_role_content(&tree.nodes, role));

        from_tree.or(match part {
            HistoryContentPart::Input => Some(input_preview),
            HistoryContentPart::Output => output_preview,
        })
    }

    pub fn update_entry_title(
        &self,
        entry_id: &str,
        title: String,
    ) -> Result<(), HistoryError> {
        let now = Self::now_timestamp();
        let updated = self.db.conn().execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![title, now, entry_id],
        )?;
        if updated == 0 {
            return Err(HistoryError::EntryNotFound(entry_id.to_string()));
        }
        Ok(())
    }

    /// Rewrites the result of a single-shot entry. Used when a manual retry
    /// finally produces a transcription for a previously failed recording.
    pub fn update_entry_result(
        &self,
        entry_id: &str,
        input_content: Option<String>,
        output_content: Option<String>,
        success: bool,
        error: Option<String>,
    ) -> Result<(), HistoryError> {
        let now = Self::now_timestamp();
        let updated = self.db.conn().execute(
            "UPDATE conversations
             SET input_content = COALESCE(?1, input_content),
                 output_content = ?2,
                 success = ?3,
                 error = ?4,
                 updated_at = ?5
             WHERE id = ?6",
            rusqlite::params![input_content, output_content, success, error, now, entry_id],
        )?;
        if updated == 0 {
            return Err(HistoryError::EntryNotFound(entry_id.to_string()));
        }
        Ok(())
    }

    pub fn delete_entry(&self, entry_id: &str) -> Result<(), HistoryError> {
        self.db.conn().execute(
            "DELETE FROM conversation_images WHERE conversation_id = ?1",
            rusqlite::params![entry_id],
        )?;
        let deleted = self.db.conn().execute(
            "DELETE FROM conversations WHERE id = ?1",
            rusqlite::params![entry_id],
        )?;
        if deleted == 0 {
            return Err(HistoryError::EntryNotFound(entry_id.to_string()));
        }
        Ok(())
    }

    pub fn conn(&self) -> &rusqlite::Connection {
        self.db.conn()
    }

    pub fn search_fts(
        &self,
        fts_query: &str,
        type_filter: HistoryTypeFilter,
        status_filter: HistoryStatusFilter,
        skill_ids: &[String],
        limit: usize,
    ) -> Result<Vec<(HistoryEntry, f64)>, HistoryError> {
        let mut sql = String::from(
            "SELECT c.id, c.title, c.skill_id, c.skill_name, c.entry_type, c.input_content, c.output_content,
                    c.success, c.error, c.is_multi_turn, c.quick_action, c.created_at, c.updated_at,
                    bm25(conversations_fts, 10.0, 5.0, 2.0, 1.0) AS rank
             FROM conversations_fts
             JOIN conversations c ON c.rowid = conversations_fts.rowid
             WHERE conversations_fts MATCH ?1",
        );

        match type_filter {
            HistoryTypeFilter::All => {}
            HistoryTypeFilter::Chat => sql.push_str(" AND c.quick_action = 0"),
            HistoryTypeFilter::Speech => sql.push_str(
                " AND c.quick_action = 1 AND c.entry_type = 'speech' AND c.skill_name IS NULL",
            ),
            HistoryTypeFilter::QuickAction => sql.push_str(
                " AND c.quick_action = 1 AND NOT (c.entry_type = 'speech' AND c.skill_name IS NULL)",
            ),
        }

        match status_filter {
            HistoryStatusFilter::All => {}
            HistoryStatusFilter::Success => sql.push_str(" AND c.success = 1"),
            HistoryStatusFilter::Error => sql.push_str(" AND c.success = 0"),
        }

        if !skill_ids.is_empty() {
            sql.push_str(" AND c.skill_id IN (");
            for i in 0..skill_ids.len() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push_str(&format!("?{}", i + 2));
            }
            sql.push(')');
        }

        sql.push_str(&format!(" ORDER BY rank LIMIT {}", limit));

        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&fts_query];
        for s in skill_ids {
            params.push(s);
        }

        let rows = stmt.query_map(params.as_slice(), |row| {
            let entry = row_to_entry(row)?;
            let rank: f64 = row.get(13)?;
            Ok((entry, rank))
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn fold_token(&self, token: &str) -> Result<Vec<String>, HistoryError> {
        let conn = self.db.conn();
        conn.execute("DELETE FROM temp.fold_fts", [])?;
        conn.execute("INSERT INTO temp.fold_fts (term) VALUES (?1)", [token])?;
        let mut stmt = conn.prepare("SELECT term FROM temp.fold_vocab ORDER BY term")?;
        let terms = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        conn.execute("DELETE FROM temp.fold_fts", [])?;
        Ok(terms)
    }

    pub fn vocab_terms_of_length(
        &self,
        min_chars: usize,
        max_chars: usize,
    ) -> Result<Vec<(String, i64)>, HistoryError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT term, doc FROM temp.conversations_vocab WHERE length(term) BETWEEN ?1 AND ?2",
        )?;
        let terms = stmt
            .query_map([min_chars as i64, max_chars as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<(String, i64)>, _>>()?;
        Ok(terms)
    }

    fn insert_images(
        &self,
        tx: &rusqlite::Transaction,
        conversation_id: &str,
        images: &[ImagePayload],
    ) {
        for img in images {
            let blob = BASE64.decode(&img.data).unwrap_or_default();
            if blob.is_empty() {
                continue;
            }
            let img_id = Self::generate_id();
            let _ = tx.execute(
                "INSERT INTO conversation_images (id, conversation_id, node_id, image_index, data, mime_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    img_id,
                    conversation_id,
                    img.node_id,
                    img.image_index,
                    blob,
                    img.media_type,
                ],
            );
        }
    }

    fn pragma_u64(&self, name: &str) -> u64 {
        self.db
            .conn()
            .query_row(&format!("PRAGMA {}", name), [], |row| row.get::<_, i64>(0))
            .map(|v| v.max(0) as u64)
            .unwrap_or(0)
    }

    pub fn storage_stats(&self) -> HistoryStorageStats {
        let page_size = self.pragma_u64("page_size");
        let page_count = self.pragma_u64("page_count");
        let freelist_count = self.pragma_u64("freelist_count");
        HistoryStorageStats {
            database_bytes: page_count * page_size,
            reclaimable_bytes: freelist_count * page_size,
        }
    }

    pub fn checkpoint_wal(&self) {
        if let Err(e) = self
            .db
            .conn()
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        {
            log::warn!("wal checkpoint failed: {}", e);
        }
    }

    pub fn vacuum(&self) -> Result<HistoryStorageStats, HistoryError> {
        let before = self.storage_stats();
        self.db.conn().execute_batch("VACUUM")?;
        self.checkpoint_wal();
        let after = self.storage_stats();
        log::info!(
            "history database vacuumed: {} -> {} bytes",
            before.database_bytes,
            after.database_bytes
        );
        Ok(after)
    }

    fn is_worth_vacuuming(&self) -> bool {
        let page_count = self.pragma_u64("page_count");
        if page_count == 0 {
            return false;
        }
        let freelist_count = self.pragma_u64("freelist_count");
        let reclaimable = freelist_count * self.pragma_u64("page_size");
        reclaimable > VACUUM_MIN_RECLAIMABLE_BYTES
            && (freelist_count as f64 / page_count as f64) > VACUUM_MIN_FREELIST_RATIO
    }

    pub fn compact_after_bulk_delete(&self) -> bool {
        self.checkpoint_wal();
        if !self.is_worth_vacuuming() {
            return false;
        }
        self.vacuum().is_ok()
    }

    pub fn enforce_retention(&self) -> usize {
        if self.retention_days == 0 {
            return 0;
        }
        let cutoff = format!("-{} days", self.retention_days);
        match self.db.conn().execute(
            "DELETE FROM conversations
             WHERE COALESCE(updated_at, created_at) < datetime('now', 'localtime', ?1)",
            [cutoff],
        ) {
            Ok(deleted) => deleted,
            Err(e) => {
                log::error!("history retention sweep failed: {}", e);
                0
            }
        }
    }

    fn generate_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn now_timestamp() -> String {
        chrono::Local::now()
            .format(DB_TIMESTAMP_FORMAT)
            .to_string()
    }
}
