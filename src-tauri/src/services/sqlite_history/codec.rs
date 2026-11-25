use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::models::history::{HistoryEntry, HistoryEntryType, SerializedConversationNode};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TreeJson {
    pub nodes: Vec<SerializedConversationNode>,
    pub root_node_id: Option<String>,
    pub current_path: Vec<String>,
    pub resolved_environment_section: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

pub(super) fn parse_entry_type(s: &str) -> HistoryEntryType {
    match s {
        "speech" => HistoryEntryType::Speech,
        _ => HistoryEntryType::Text,
    }
}

pub(crate) fn entry_type_str(entry_type: &HistoryEntryType) -> &'static str {
    match entry_type {
        HistoryEntryType::Text => "text",
        HistoryEntryType::Speech => "speech",
    }
}

pub(super) fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        title: row.get(1)?,
        skill_id: row.get(2)?,
        skill_name: row.get(3)?,
        entry_type: parse_entry_type(row.get::<_, String>(4)?.as_str()),
        input_preview: row.get(5)?,
        output_preview: row.get(6)?,
        success: row.get(7)?,
        error: row.get(8)?,
        is_multi_turn: row.get(9)?,
        quick_action: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        timestamp: row.get::<_, Option<String>>(11)?.unwrap_or_default(),
        conversation_data: None,
    })
}

pub(super) const ENTRY_COLUMNS: &str =
    "id, title, skill_id, skill_name, entry_type, input_content, output_content, \
     success, error, is_multi_turn, quick_action, created_at, updated_at";

/// Full content of the last node with `role`, as stored in the conversation tree.
pub(super) fn last_role_content(
    nodes: &[SerializedConversationNode],
    role: &str,
) -> Option<String> {
    nodes
        .iter()
        .rev()
        .find(|n| n.role == role)
        .map(|n| n.content.clone())
        .filter(|c| !c.is_empty())
}

fn summarize_last_of_role(
    nodes: &[SerializedConversationNode],
    role: &str,
    empty_placeholder: &str,
) -> Option<String> {
    let last = nodes.iter().rev().find(|n| n.role == role)?;
    if last.content.is_empty() {
        return Some(empty_placeholder.to_string());
    }
    let count = nodes.iter().filter(|n| n.role == role).count();
    if count > 1 {
        let truncated: String = last.content.chars().take(100).collect();
        Some(format!("{}... (+{} more)", truncated, count - 1))
    } else {
        Some(last.content.chars().take(200).collect())
    }
}

/// Display-only summary of the last user node, capped at 200 characters.
pub(super) fn build_input_summary(nodes: &[SerializedConversationNode]) -> String {
    summarize_last_of_role(nodes, "user", "(empty)").unwrap_or_else(|| "(no input)".to_string())
}

/// Display-only summary of the last assistant node, capped at 200 characters.
pub(super) fn build_output_summary(nodes: &[SerializedConversationNode]) -> Option<String> {
    summarize_last_of_role(nodes, "assistant", "(no output yet)")
}

pub(super) fn build_applied_skill_names(nodes: &[SerializedConversationNode]) -> Option<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for node in nodes {
        for applied in &node.applied_skills {
            names.insert(applied.name.clone());
        }
    }
    if names.is_empty() {
        None
    } else {
        Some(names.into_iter().collect::<Vec<_>>().join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_node(content: &str) -> SerializedConversationNode {
        SerializedConversationNode {
            node_id: "u".into(),
            parent_id: None,
            role: "user".into(),
            content: content.into(),
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
        }
    }

    fn assistant_node(content: &str) -> SerializedConversationNode {
        SerializedConversationNode {
            role: "assistant".into(),
            content: content.into(),
            ..user_node(content)
        }
    }

    #[test]
    fn parse_entry_type_speech() {
        assert!(matches!(parse_entry_type("speech"), HistoryEntryType::Speech));
    }

    #[test]
    fn parse_entry_type_text_default() {
        assert!(matches!(parse_entry_type("text"), HistoryEntryType::Text));
        assert!(matches!(parse_entry_type("anything-else"), HistoryEntryType::Text));
    }

    #[test]
    fn input_summary_no_user_returns_no_input() {
        assert_eq!(build_input_summary(&[]), "(no input)");
        assert_eq!(
            build_input_summary(&[assistant_node("hi")]),
            "(no input)"
        );
    }

    #[test]
    fn input_summary_empty_user_returns_empty() {
        assert_eq!(build_input_summary(&[user_node("")]), "(empty)");
    }

    #[test]
    fn input_summary_single_user_returns_content() {
        assert_eq!(
            build_input_summary(&[user_node("hello")]),
            "hello"
        );
    }

    #[test]
    fn input_summary_multi_user_truncates_and_appends_count() {
        let nodes = vec![user_node("first"), assistant_node("reply"), user_node("second")];
        let summary = build_input_summary(&nodes);
        assert!(summary.contains("second"));
        assert!(summary.contains("+1 more"));
    }

    #[test]
    fn input_summary_long_single_user_truncated_to_200_chars() {
        let long_text = "a".repeat(300);
        let summary = build_input_summary(&[user_node(&long_text)]);
        assert_eq!(summary.chars().count(), 200);
    }

    #[test]
    fn output_summary_none_when_no_assistant() {
        assert!(build_output_summary(&[]).is_none());
        assert!(build_output_summary(&[user_node("hi")]).is_none());
    }

    #[test]
    fn output_summary_empty_assistant_returns_placeholder() {
        assert_eq!(
            build_output_summary(&[assistant_node("")]).unwrap(),
            "(no output yet)"
        );
    }

    #[test]
    fn output_summary_single_assistant() {
        assert_eq!(
            build_output_summary(&[assistant_node("response")]).unwrap(),
            "response"
        );
    }

    #[test]
    fn output_summary_multi_assistant_appends_count() {
        let nodes = vec![
            user_node("u1"),
            assistant_node("a1"),
            user_node("u2"),
            assistant_node("a2"),
        ];
        let summary = build_output_summary(&nodes).unwrap();
        assert!(summary.contains("a2"));
        assert!(summary.contains("+1 more"));
    }

    #[test]
    fn applied_skill_names_empty() {
        assert!(build_applied_skill_names(&[]).is_none());
        assert!(build_applied_skill_names(&[user_node("hi")]).is_none());
    }

    #[test]
    fn applied_skill_names_dedupes_and_sorts() {
        use crate::models::message::AppliedSkill;
        let mut node = user_node("hi");
        node.applied_skills = vec![
            AppliedSkill {
                name: "beta".into(),
                skill_version_id: 1,
                input: String::new(),
            },
            AppliedSkill {
                name: "alpha".into(),
                skill_version_id: 2,
                input: String::new(),
            },
            AppliedSkill {
                name: "alpha".into(),
                skill_version_id: 3,
                input: String::new(),
            },
        ];
        let result = build_applied_skill_names(&[node]).unwrap();
        assert_eq!(result, "alpha,beta");
    }

    #[test]
    fn tree_json_round_trip() {
        let tree = TreeJson {
            nodes: vec![user_node("hello")],
            root_node_id: Some("u".into()),
            current_path: vec!["u".into()],
            resolved_environment_section: Some("env".into()),
            model_id: Some("gpt-4".into()),
            reasoning_effort: Some("high".into()),
        };
        let json = serde_json::to_string(&tree).unwrap();
        let decoded: TreeJson = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.nodes.len(), 1);
        assert_eq!(decoded.root_node_id.as_deref(), Some("u"));
        assert_eq!(decoded.model_id.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn tree_json_round_trips_node_sources() {
        use crate::models::ai::Source;
        let sources = vec![
            Source { url: "https://a.example/x".into(), title: "A".into() },
            Source { url: "https://b.example".into(), title: "https://b.example".into() },
        ];
        let tree = TreeJson {
            nodes: vec![SerializedConversationNode {
                sources: sources.clone(),
                ..assistant_node("reply")
            }],
            root_node_id: None,
            current_path: vec![],
            resolved_environment_section: None,
            model_id: None,
            reasoning_effort: None,
        };

        let decoded: TreeJson = serde_json::from_str(&serde_json::to_string(&tree).unwrap()).unwrap();

        assert_eq!(decoded.nodes[0].sources, sources);
    }

    #[test]
    fn node_json_without_sources_decodes_with_empty_list() {
        let json = r#"{"node_id":"a","parent_id":null,"role":"assistant","content":"hi"}"#;

        let node: SerializedConversationNode = serde_json::from_str(json).unwrap();

        assert!(node.sources.is_empty());
    }

    #[test]
    fn tree_json_optional_model_id_missing() {
        let json = r#"{"nodes":[],"root_node_id":null,"current_path":[],"resolved_environment_section":null}"#;
        let decoded: TreeJson = serde_json::from_str(json).unwrap();
        assert!(decoded.model_id.is_none());
        assert!(decoded.reasoning_effort.is_none());
    }
}
