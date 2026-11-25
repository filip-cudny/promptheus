use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::models::ai::Source;
use crate::models::capabilities::ModelCapabilities;
use crate::models::message::ProcessedMessage;
use crate::models::settings::ModelParameters;

use super::AiError;

pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ProcessedMessage>,
    pub parameters: ModelParameters,
    pub tool_payloads: Vec<serde_json::Value>,
    pub capabilities: ModelCapabilities,
}

pub struct StreamChunk {
    pub delta: String,
    pub accumulated: String,
    pub thinking_delta: Option<String>,
    pub accumulated_thinking: Option<String>,
    pub usage: Option<TokenUsage>,
    pub tool_call_event: Option<ToolCallEvent>,
    pub reasoning_item: Option<serde_json::Value>,
    pub source: Option<Source>,
}

impl StreamChunk {
    pub fn content(
        delta: String,
        accumulated: &str,
        thinking_delta: Option<String>,
        accumulated_thinking: &str,
    ) -> Self {
        Self {
            delta,
            accumulated: accumulated.to_string(),
            thinking_delta,
            accumulated_thinking: (!accumulated_thinking.is_empty())
                .then(|| accumulated_thinking.to_string()),
            usage: None,
            tool_call_event: None,
            reasoning_item: None,
            source: None,
        }
    }

    pub fn tool_call(accumulated: &str, event: ToolCallEvent) -> Self {
        Self {
            tool_call_event: Some(event),
            ..Self::content(String::new(), accumulated, None, "")
        }
    }

    pub fn reasoning_item(accumulated: &str, item: serde_json::Value) -> Self {
        Self {
            reasoning_item: Some(item),
            ..Self::content(String::new(), accumulated, None, "")
        }
    }

    pub fn source(accumulated: &str, source: Source) -> Self {
        Self {
            source: Some(source),
            ..Self::content(String::new(), accumulated, None, "")
        }
    }

    pub fn usage(accumulated: &str, usage: TokenUsage) -> Self {
        Self {
            usage: Some(usage),
            ..Self::content(String::new(), accumulated, None, "")
        }
    }
}

#[derive(Debug, Clone)]
pub enum ToolCallEvent {
    Started {
        tool_call_id: String,
        tool_name: String,
    },
    ArgumentsComplete {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    Done {
        tool_call_id: String,
        result: Option<String>,
        error: Option<String>,
    },
}

impl ToolCallEvent {
    pub fn arguments_complete(tool_call_id: String, tool_name: String, raw_arguments: &str) -> Self {
        Self::ArgumentsComplete {
            tool_call_id,
            tool_name,
            arguments: serde_json::from_str(raw_arguments)
                .unwrap_or_else(|_| serde_json::json!({})),
        }
    }
}

pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<String, AiError>;

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, AiError>> + Send>>, AiError>;
}
