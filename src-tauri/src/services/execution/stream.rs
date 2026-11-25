use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::watch;
use tokio::sync::Mutex as TokioMutex;
use tokio_stream::{Stream, StreamExt};

use super::types::{PendingToolCall, StreamResult, ToolExecutionResult};
use crate::models::ai::{StreamEvent, ToolCall, ToolCallStatus, ToolCallType};
use crate::models::message::{
    MessageContent, ProcessedMessage, ToolCallFunction, ToolCallPayload,
};
use crate::services::ai::provider::{StreamChunk, ToolCallEvent};
use crate::services::ai::AiError;
use crate::services::execution::lifecycle::LiveExecution;
use crate::services::mcp::McpRegistry;
use crate::Error;

fn tool_display_name(tool_name: &str) -> &str {
    match tool_name {
        "web_search" => "Web Search",
        other => other,
    }
}

fn tool_type_from_name(tool_name: &str) -> ToolCallType {
    match tool_name {
        "web_search" => ToolCallType::WebSearch,
        _ => ToolCallType::Custom,
    }
}

pub async fn run_stream_loop(
    mut stream: Pin<Box<dyn Stream<Item = Result<StreamChunk, AiError>> + Send>>,
    live: Arc<TokioMutex<LiveExecution>>,
    cancel_rx: Option<watch::Receiver<bool>>,
    text_prefix: &str,
) -> crate::Result<StreamResult> {
    let mut full_text = String::new();
    let mut full_thinking = String::new();
    let mut prompt_tokens: Option<usize> = None;
    let mut completion_tokens: Option<usize> = None;
    let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
    let mut reasoning_items: Vec<serde_json::Value> = Vec::new();
    let mut cancel_rx = cancel_rx;

    loop {
        let chunk_result: Option<Result<StreamChunk, AiError>> =
            if let Some(ref mut rx) = cancel_rx {
                tokio::select! {
                    biased;
                    result = rx.changed() => {
                        if result.is_ok() && *rx.borrow() {
                            live.lock().await.finish_with_error("Cancelled".to_string());
                            return Err(Error::Other("Cancelled".to_string()));
                        }
                        continue;
                    }
                    next = stream.next() => next,
                }
            } else {
                stream.next().await
            };

        let Some(result) = chunk_result else { break };

        match result {
            Ok(chunk) => {
                if let Some(usage) = chunk.usage {
                    prompt_tokens = Some(usage.prompt_tokens);
                    completion_tokens = Some(usage.completion_tokens);
                }

                if let Some(source) = chunk.source {
                    live.lock().await.add_source(source);
                    continue;
                }

                if let Some(item) = chunk.reasoning_item {
                    reasoning_items.push(item);
                    continue;
                }

                if let Some(tool_event) = chunk.tool_call_event {
                    let prefixed_accumulated = format!("{text_prefix}{}", chunk.accumulated);
                    full_text.clone_from(&prefixed_accumulated);
                    let mut live = live.lock().await;
                    live.snapshot.accumulated_text.clone_from(&prefixed_accumulated);

                    match tool_event {
                        ToolCallEvent::Started {
                            tool_call_id,
                            tool_name,
                        } => {
                            let display_name = tool_display_name(&tool_name).to_string();
                            let tool_type = tool_type_from_name(&tool_name);
                            let tc = ToolCall {
                                tool_call_id,
                                tool_name,
                                tool_display_name: display_name,
                                tool_type,
                                arguments: serde_json::json!({}),
                                result: None,
                                error: None,
                                status: ToolCallStatus::InProgress,
                                started_at: Some(chrono::Utc::now().to_rfc3339()),
                                completed_at: None,
                            };
                            live.snapshot.tool_calls.push(tc.clone());
                            live.send(StreamEvent::ToolCallStart { tool_call: tc });
                        }
                        ToolCallEvent::ArgumentsComplete {
                            tool_call_id,
                            tool_name,
                            arguments,
                        } => {
                            if let Some(tc) = live
                                .snapshot
                                .tool_calls
                                .iter_mut()
                                .find(|t| t.tool_call_id == tool_call_id)
                            {
                                tc.arguments = arguments.clone();
                            }
                            pending_tool_calls.push(PendingToolCall {
                                tool_call_id,
                                tool_name,
                                arguments,
                            });
                        }
                        ToolCallEvent::Done {
                            tool_call_id,
                            result,
                            error,
                        } => {
                            live.finish_tool_call(tool_call_id, result, error);
                        }
                    }
                    continue;
                }

                let has_content = !chunk.delta.is_empty() || chunk.thinking_delta.is_some();
                if has_content {
                    let prefixed_accumulated = format!("{text_prefix}{}", chunk.accumulated);
                    full_text.clone_from(&prefixed_accumulated);
                    if let Some(ref acc) = chunk.accumulated_thinking {
                        full_thinking.clone_from(acc);
                    }

                    let mut live = live.lock().await;
                    live.snapshot.accumulated_text.clone_from(&prefixed_accumulated);
                    live.snapshot.accumulated_thinking = chunk.accumulated_thinking.clone();
                    live.snapshot.is_thinking =
                        chunk.accumulated_thinking.is_some() && chunk.accumulated.is_empty();

                    live.send(StreamEvent::Chunk {
                        delta: chunk.delta,
                        accumulated: prefixed_accumulated,
                        thinking_delta: chunk.thinking_delta,
                        accumulated_thinking: chunk.accumulated_thinking,
                    });
                }
            }
            Err(e) => {
                live.lock().await.finish_with_error(e.to_string());
                return Err(Error::from(e));
            }
        }
    }

    let thinking = if full_thinking.is_empty() {
        None
    } else {
        Some(full_thinking)
    };

    if pending_tool_calls.is_empty() {
        live.lock().await.finish_done(
            full_text.clone(),
            thinking.clone(),
            prompt_tokens,
            completion_tokens,
        );
    }

    Ok(StreamResult {
        full_text,
        full_thinking: thinking,
        prompt_tokens,
        completion_tokens,
        pending_tool_calls,
        reasoning_items,
    })
}

fn extract_mcp_result_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            rmcp::model::RawContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) async fn call_mcp_tool(
    mcp: &McpRegistry,
    tool_name: &str,
    arguments: serde_json::Value,
) -> (String, bool) {
    match mcp.call_tool(tool_name, arguments).await {
        Ok(result) => (extract_mcp_result_text(&result), result.is_error.unwrap_or(false)),
        Err(e) => (format!("Error executing tool '{}': {}", tool_name, e), true),
    }
}

pub async fn execute_tool_calls(
    mcp: &Arc<McpRegistry>,
    pending: &[PendingToolCall],
    live: &Arc<TokioMutex<LiveExecution>>,
) -> Vec<ToolExecutionResult> {
    let futures: Vec<_> = pending
        .iter()
        .map(|tc| {
            let tool_call_id = tc.tool_call_id.clone();
            let tool_name = tc.tool_name.clone();
            let arguments = tc.arguments.clone();
            let mcp = Arc::clone(mcp);
            let live = Arc::clone(live);
            async move {
                let (result_text, is_error) = call_mcp_tool(&mcp, &tool_name, arguments).await;
                let execution_result = ToolExecutionResult {
                    tool_call_id,
                    result_text,
                    is_error,
                };

                let (result, error) = if execution_result.is_error {
                    (None, Some(execution_result.result_text.clone()))
                } else {
                    (Some(execution_result.result_text.clone()), None)
                };
                live.lock()
                    .await
                    .finish_tool_call(execution_result.tool_call_id.clone(), result, error);

                execution_result
            }
        })
        .collect();

    futures::future::join_all(futures).await
}

pub fn build_tool_loop_messages(
    pending: &[PendingToolCall],
    accumulated_text: &str,
    reasoning_items: &[serde_json::Value],
    results: &[ToolExecutionResult],
) -> Vec<ProcessedMessage> {
    let mut messages = Vec::with_capacity(1 + results.len());

    let tool_calls: Vec<ToolCallPayload> = pending
        .iter()
        .map(|tc| ToolCallPayload {
            id: tc.tool_call_id.clone(),
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: tc.tool_name.clone(),
                arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
            },
        })
        .collect();

    messages.push(ProcessedMessage {
        role: "assistant".to_string(),
        content: MessageContent::Text(accumulated_text.to_string()),
        tool_calls: Some(tool_calls),
        tool_call_id: None,
        reasoning_items: (!reasoning_items.is_empty()).then(|| reasoning_items.to_vec()),
    });

    for r in results {
        messages.push(ProcessedMessage {
            role: "tool".to_string(),
            content: MessageContent::Text(r.result_text.clone()),
            tool_calls: None,
            tool_call_id: Some(r.tool_call_id.clone()),
            reasoning_items: None,
        });
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ai::Source;
    use crate::models::capabilities::ModelCapabilities;
    use crate::services::ai::provider::{AiProvider, CompletionRequest};
    use crate::services::execution::lifecycle::{ExecutionSnapshot, LiveExecution};
    use async_trait::async_trait;
    use futures::stream;

    struct FakeProvider {
        chunks: std::sync::Mutex<Option<Vec<StreamChunk>>>,
    }

    impl FakeProvider {
        fn new(chunks: Vec<StreamChunk>) -> Self {
            Self {
                chunks: std::sync::Mutex::new(Some(chunks)),
            }
        }
    }

    #[async_trait]
    impl AiProvider for FakeProvider {
        async fn complete(&self, _request: CompletionRequest) -> Result<String, AiError> {
            Ok(String::new())
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, AiError>> + Send>>, AiError>
        {
            let chunks = self.chunks.lock().unwrap().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(chunks.into_iter().map(Ok))))
        }
    }

    fn empty_live() -> Arc<TokioMutex<LiveExecution>> {
        Arc::new(TokioMutex::new(LiveExecution {
            snapshot: ExecutionSnapshot {
                execution_id: "test".to_string(),
                user_message: String::new(),
                accumulated_text: String::new(),
                accumulated_thinking: None,
                tool_calls: Vec::new(),
                is_thinking: false,
                finished: false,
                error: None,
                prompt_tokens: None,
                completion_tokens: None,
                sources: Vec::new(),
            },
            channel: None,
        }))
    }

    fn text_chunk(delta: &str, accumulated: &str) -> StreamChunk {
        StreamChunk {
            delta: delta.to_string(),
            accumulated: accumulated.to_string(),
            thinking_delta: None,
            accumulated_thinking: None,
            usage: None,
            tool_call_event: None,
            reasoning_item: None,
            source: None,
        }
    }

    fn recording_live() -> (
        Arc<TokioMutex<LiveExecution>>,
        Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    ) {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let channel = tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                sink.lock().unwrap().push(serde_json::from_str(&json).unwrap());
            }
            Ok(())
        });
        let live = empty_live();
        live.try_lock().unwrap().channel = Some(channel);
        (live, events)
    }

    fn event_names(events: &Arc<std::sync::Mutex<Vec<serde_json::Value>>>) -> Vec<String> {
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e["event"].as_str().unwrap().to_string())
            .collect()
    }

    fn tool_event_chunk(event: ToolCallEvent) -> StreamChunk {
        StreamChunk {
            tool_call_event: Some(event),
            ..text_chunk("", "")
        }
    }

    fn stream_of(
        items: Vec<Result<StreamChunk, AiError>>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk, AiError>> + Send>> {
        Box::pin(stream::iter(items))
    }

    #[tokio::test]
    async fn stream_loop_emits_tool_call_events_then_done() {
        let (live, events) = recording_live();
        let stream = stream_of(vec![
            Ok(tool_event_chunk(ToolCallEvent::Started {
                tool_call_id: "tc-1".to_string(),
                tool_name: "web_search".to_string(),
            })),
            Ok(tool_event_chunk(ToolCallEvent::Done {
                tool_call_id: "tc-1".to_string(),
                result: Some("found".to_string()),
                error: None,
            })),
            Ok(text_chunk("Hi", "Hi")),
        ]);

        let result = run_stream_loop(stream, Arc::clone(&live), None, "")
            .await
            .unwrap();

        assert_eq!(
            event_names(&events),
            ["tool_call_start", "tool_call_done", "chunk", "done"]
        );
        assert_eq!(events.lock().unwrap()[1]["data"]["result"], "found");
        assert_eq!(events.lock().unwrap()[3]["data"]["full_text"], "Hi");
        assert_eq!(result.full_text, "Hi");
        let live = live.lock().await;
        assert!(live.snapshot.finished);
        assert!(matches!(live.snapshot.tool_calls[0].status, ToolCallStatus::Completed));
        assert_eq!(live.snapshot.tool_calls[0].result.as_deref(), Some("found"));
        assert!(live.snapshot.tool_calls[0].completed_at.is_some());
    }

    #[tokio::test]
    async fn stream_loop_marks_failed_tool_call_with_error() {
        let (live, events) = recording_live();
        let stream = stream_of(vec![
            Ok(tool_event_chunk(ToolCallEvent::Started {
                tool_call_id: "tc-1".to_string(),
                tool_name: "web_search".to_string(),
            })),
            Ok(tool_event_chunk(ToolCallEvent::Done {
                tool_call_id: "tc-1".to_string(),
                result: None,
                error: Some("boom".to_string()),
            })),
        ]);

        run_stream_loop(stream, Arc::clone(&live), None, "")
            .await
            .unwrap();

        assert_eq!(events.lock().unwrap()[1]["data"]["error"], "boom");
        let live = live.lock().await;
        assert!(matches!(live.snapshot.tool_calls[0].status, ToolCallStatus::Failed));
        assert_eq!(live.snapshot.tool_calls[0].error.as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn stream_loop_keeps_pending_tool_calls_without_done() {
        let (live, events) = recording_live();
        let stream = stream_of(vec![
            Ok(tool_event_chunk(ToolCallEvent::Started {
                tool_call_id: "tc-1".to_string(),
                tool_name: "srv.tool".to_string(),
            })),
            Ok(tool_event_chunk(ToolCallEvent::ArgumentsComplete {
                tool_call_id: "tc-1".to_string(),
                tool_name: "srv.tool".to_string(),
                arguments: serde_json::json!({"q": 1}),
            })),
        ]);

        let result = run_stream_loop(stream, Arc::clone(&live), None, "")
            .await
            .unwrap();

        assert_eq!(event_names(&events), ["tool_call_start"]);
        assert_eq!(result.pending_tool_calls.len(), 1);
        let live = live.lock().await;
        assert!(!live.snapshot.finished);
        assert_eq!(live.snapshot.tool_calls[0].arguments, serde_json::json!({"q": 1}));
    }

    fn reasoning_item() -> serde_json::Value {
        serde_json::json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "enc"})
    }

    #[tokio::test]
    async fn stream_loop_collects_reasoning_items_without_events() {
        let (live, events) = recording_live();
        let stream = stream_of(vec![Ok(StreamChunk::reasoning_item("", reasoning_item()))]);

        let result = run_stream_loop(stream, live, None, "").await.unwrap();

        assert_eq!(result.reasoning_items, vec![reasoning_item()]);
        assert_eq!(event_names(&events), ["done"]);
    }

    fn source(url: &str) -> Source {
        Source {
            url: url.to_string(),
            title: url.to_string(),
        }
    }

    #[tokio::test]
    async fn sources_of_all_iterations_reach_done_once_each_in_order() {
        let (live, events) = recording_live();
        let first = stream_of(vec![
            Ok(StreamChunk::source("", source("a"))),
            Ok(StreamChunk::source("", source("b"))),
            Ok(tool_event_chunk(ToolCallEvent::ArgumentsComplete {
                tool_call_id: "tc-1".to_string(),
                tool_name: "srv.tool".to_string(),
                arguments: serde_json::json!({}),
            })),
        ]);
        let second = stream_of(vec![
            Ok(StreamChunk::source("", source("b"))),
            Ok(StreamChunk::source("", source("c"))),
            Ok(text_chunk("Hi", "Hi")),
        ]);

        run_stream_loop(first, Arc::clone(&live), None, "").await.unwrap();
        run_stream_loop(second, Arc::clone(&live), None, "").await.unwrap();

        let expected = vec![source("a"), source("b"), source("c")];
        assert_eq!(live.lock().await.snapshot.sources, expected);
        let events = events.lock().unwrap();
        let done = events.last().unwrap();
        assert_eq!(done["event"], "done");
        assert_eq!(
            done["data"]["sources"],
            serde_json::json!([
                {"url": "a", "title": "a"},
                {"url": "b", "title": "b"},
                {"url": "c", "title": "c"},
            ])
        );
        assert_eq!(events.iter().filter(|e| e["event"] != "done").count(), 1);
    }

    #[test]
    fn tool_loop_messages_carry_reasoning_items_on_the_assistant_message_only() {
        let pending = vec![PendingToolCall {
            tool_call_id: "c1".to_string(),
            tool_name: "lookup".to_string(),
            arguments: serde_json::json!({}),
        }];
        let results = vec![ToolExecutionResult {
            tool_call_id: "c1".to_string(),
            result_text: "ok".to_string(),
            is_error: false,
        }];

        let messages = build_tool_loop_messages(&pending, "", &[reasoning_item()], &results);
        let without = build_tool_loop_messages(&pending, "", &[], &results);

        assert_eq!(messages[0].reasoning_items, Some(vec![reasoning_item()]));
        assert_eq!(messages[1].reasoning_items, None);
        assert_eq!(without[0].reasoning_items, None);
    }

    #[tokio::test]
    async fn stream_loop_reports_stream_error() {
        let (live, events) = recording_live();
        let stream = stream_of(vec![
            Ok(text_chunk("Hi", "Hi")),
            Err(AiError::Connection("boom".to_string())),
        ]);

        let result = run_stream_loop(stream, Arc::clone(&live), None, "").await;

        assert!(result.is_err());
        assert_eq!(event_names(&events), ["chunk", "error"]);
        assert_eq!(
            events.lock().unwrap()[1]["data"]["message"],
            "connection failed: boom"
        );
        let live = live.lock().await;
        assert!(live.snapshot.finished);
        assert_eq!(live.snapshot.error.as_deref(), Some("connection failed: boom"));
    }

    #[tokio::test]
    async fn stream_loop_reports_cancellation() {
        let (live, events) = recording_live();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        cancel_tx.send(true).unwrap();
        let stream: Pin<Box<dyn Stream<Item = Result<StreamChunk, AiError>> + Send>> =
            Box::pin(stream::pending());

        let result = run_stream_loop(stream, Arc::clone(&live), Some(cancel_rx), "").await;

        assert_eq!(result.err().unwrap().to_string(), "Cancelled");
        assert_eq!(event_names(&events), ["error"]);
        let live = live.lock().await;
        assert!(live.snapshot.finished);
        assert_eq!(live.snapshot.error.as_deref(), Some("Cancelled"));
    }

    #[tokio::test]
    async fn stream_loop_aggregates_text_chunks() {
        let provider = FakeProvider::new(vec![
            text_chunk("Hello", "Hello"),
            text_chunk(" world", "Hello world"),
        ]);

        let request = CompletionRequest {
            model: "test".to_string(),
            messages: vec![],
            parameters: Default::default(),
            tool_payloads: vec![],
            capabilities: ModelCapabilities::minimal(),
        };
        let stream = provider.complete_stream(request).await.unwrap();

        let result = run_stream_loop(stream, empty_live(), None, "")
            .await
            .unwrap();
        assert_eq!(result.full_text, "Hello world");
        assert!(result.pending_tool_calls.is_empty());
    }
}
