use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::ai::{Source, ToolCall};
use crate::models::message::{AppliedSkill, ImageData, NodeUpdate};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HistoryEntryType {
    Speech,
    Text,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InvocationSource {
    ContextMenu,
    AlternativeExecute,
    Chat,
    Pill,
}

impl InvocationSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextMenu => "context_menu",
            Self::AlternativeExecute => "alternative_execute",
            Self::Chat => "chat",
            Self::Pill => "pill",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputRoute {
    Clipboard,
    Chat,
    ClipboardChat,
}

impl OutputRoute {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Clipboard => "clipboard",
            Self::Chat => "chat",
            Self::ClipboardChat => "clipboard_chat",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedConversationNode {
    pub node_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub updates: Vec<NodeUpdate>,
    #[serde(default)]
    pub prompt_tokens: Option<usize>,
    #[serde(default)]
    pub completion_tokens: Option<usize>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub thinking_duration: Option<f64>,
    #[serde(default)]
    pub query_duration: Option<f64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub text_attachments: Vec<String>,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub applied_skills: Vec<AppliedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHistoryData {
    pub context_text: String,
    #[serde(default)]
    pub nodes: Vec<SerializedConversationNode>,
    pub root_node_id: Option<String>,
    #[serde(default)]
    pub current_path: Vec<String>,
    #[serde(default)]
    pub resolved_environment_section: Option<String>,
    #[serde(default)]
    pub node_images: HashMap<String, Vec<ImageData>>,
    #[serde(default)]
    pub context_images: Vec<ImageData>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryChangedEvent {
    pub added_id: Option<String>,
    pub removed_id: Option<String>,
    pub version: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImagePayload {
    pub node_id: Option<String>,
    pub image_index: u32,
    pub data: String,
    pub media_type: String,
}

fn default_true() -> bool {
    true
}

/// Which side of an entry a full-text lookup refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryContentPart {
    Input,
    Output,
}

/// A history row without its conversation tree.
///
/// `input_preview` and `output_preview` are display-only. For conversation
/// entries (anything written by `add_conversation_entry`, including skill quick
/// actions) they are summaries capped at 200 characters; the full text lives in
/// `tree_json` and is reachable only via `SqliteHistoryService::resolve_full_content`
/// or `conversation_data`. For simple entries (speech) they happen to hold the
/// whole value. Never copy, export or re-execute these fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: String,
    #[serde(alias = "input_content")]
    pub input_preview: String,
    pub entry_type: HistoryEntryType,
    #[serde(alias = "output_content")]
    pub output_preview: Option<String>,
    pub skill_id: Option<String>,
    #[serde(default = "default_true")]
    pub success: bool,
    pub error: Option<String>,
    #[serde(default, alias = "is_conversation")]
    pub is_multi_turn: bool,
    pub skill_name: Option<String>,
    pub conversation_data: Option<ConversationHistoryData>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub quick_action: bool,

    #[serde(default)]
    pub title: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn pill_and_clipboard_chat_round_trip_and_name_themselves() {
        let source: InvocationSource = serde_json::from_str("\"pill\"").unwrap();
        assert_eq!(source, InvocationSource::Pill);
        assert_eq!(serde_json::to_string(&source).unwrap(), "\"pill\"");
        assert_eq!(source.as_str(), "pill");

        let route: OutputRoute = serde_json::from_str("\"clipboard_chat\"").unwrap();
        assert_eq!(route, OutputRoute::ClipboardChat);
        assert_eq!(serde_json::to_string(&route).unwrap(), "\"clipboard_chat\"");
        assert_eq!(route.as_str(), "clipboard_chat");
    }

    #[test]
    fn test_simple_text_entry_round_trip() {
        let entry = HistoryEntry {
            id: "entry-1".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            input_preview: "Hello world".into(),
            entry_type: HistoryEntryType::Text,
            output_preview: Some("Response text".into()),
            skill_id: Some("prompt-1".into()),
            success: true,
            error: None,
            is_multi_turn: false,
            skill_name: Some("Test Prompt".into()),
            conversation_data: None,
            created_at: Some("2026-01-01T00:00:00Z".into()),
            updated_at: None,
            quick_action: false,
            title: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: HistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "entry-1");
        assert_eq!(deserialized.entry_type, HistoryEntryType::Text);
        assert!(deserialized.success);
        assert!(!deserialized.is_multi_turn);
        assert!(deserialized.conversation_data.is_none());
    }

    #[test]
    fn test_entry_with_conversation_data_round_trip() {
        let entry = HistoryEntry {
            id: "entry-2".into(),
            timestamp: "2026-01-01T12:00:00Z".into(),
            input_preview: "Conversation start".into(),
            entry_type: HistoryEntryType::Speech,
            output_preview: Some("Final output".into()),
            skill_id: Some("prompt-2".into()),
            success: true,
            error: None,
            is_multi_turn: true,
            skill_name: Some("Chat Prompt".into()),
            conversation_data: Some(ConversationHistoryData {
                context_text: "Some context".into(),
                nodes: vec![
                    SerializedConversationNode {
                        node_id: "node-root".into(),
                        parent_id: None,
                        role: "user".into(),
                        content: "Hello".into(),
                        timestamp: "2026-01-01T12:00:00Z".into(),
                        children: vec!["node-reply".into()],
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
                        node_id: "node-reply".into(),
                        parent_id: Some("node-root".into()),
                        role: "assistant".into(),
                        content: "Hi there".into(),
                        timestamp: "2026-01-01T12:00:01Z".into(),
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
                ],
                root_node_id: Some("node-root".into()),
                current_path: vec!["node-root".into(), "node-reply".into()],
                resolved_environment_section: None,
                node_images: HashMap::new(),
                context_images: vec![],
                model_id: None,
                reasoning_effort: None,
            }),
            created_at: Some("2026-01-01T12:00:00Z".into()),
            updated_at: Some("2026-01-01T12:00:01Z".into()),
            quick_action: false,
            title: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: HistoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.entry_type, HistoryEntryType::Speech);
        assert!(deserialized.is_multi_turn);

        let conv = deserialized.conversation_data.unwrap();
        assert_eq!(conv.nodes.len(), 2);
        assert_eq!(conv.root_node_id, Some("node-root".into()));
        assert_eq!(conv.current_path.len(), 2);
    }

    #[test]
    fn test_minimal_json_defaults() {
        let json = r#"{
            "id": "entry-3",
            "timestamp": "2026-01-01",
            "input_content": "test",
            "entry_type": "text"
        }"#;

        let entry: HistoryEntry = serde_json::from_str(json).unwrap();

        assert_eq!(entry.id, "entry-3");
        assert_eq!(entry.input_preview, "test");
        assert_eq!(entry.entry_type, HistoryEntryType::Text);
        assert!(entry.success);
        assert!(!entry.is_multi_turn);
        assert!(entry.output_preview.is_none());
        assert!(entry.skill_id.is_none());
        assert!(entry.error.is_none());
        assert!(entry.skill_name.is_none());
        assert!(entry.conversation_data.is_none());
        assert!(entry.created_at.is_none());
        assert!(entry.updated_at.is_none());
    }
}
