use serde::{Deserialize, Serialize};

use crate::models::message::NodeUpdate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallType {
    WebSearch,
    CodeExecution,
    FileRead,
    FileWrite,
    ApiCall,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_display_name: String,
    pub tool_type: ToolCallType,
    pub arguments: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
    pub status: ToolCallStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub title: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum StreamEvent {
    Chunk {
        delta: String,
        accumulated: String,
        thinking_delta: Option<String>,
        accumulated_thinking: Option<String>,
    },
    Done {
        full_text: String,
        full_thinking: Option<String>,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
        sources: Vec<Source>,
    },
    Error {
        message: String,
    },
    NodeUpdates {
        node_id: String,
        updates: Vec<NodeUpdate>,
    },
    ToolCallStart {
        tool_call: ToolCall,
    },
    ToolCallDone {
        tool_call_id: String,
        result: Option<String>,
        error: Option<String>,
    },
}
