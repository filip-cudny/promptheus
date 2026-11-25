use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::models::ai::Source;
use crate::models::message::{ContentPart, MessageContent, ProcessedMessage};
use crate::models::settings::ModelConfig;

use super::encoder::encode_responses_capabilities;
use super::http::OpenAiHttp;
use super::provider::{AiProvider, CompletionRequest, StreamChunk, TokenUsage, ToolCallEvent};
use super::sse::parse_sse_stream;
use super::tools::ToolRegistry;
use super::AiError;

pub struct OpenAiResponsesProvider {
    http: OpenAiHttp,
    store: bool,
}

impl OpenAiResponsesProvider {
    pub fn new(model: &ModelConfig) -> Result<Self, AiError> {
        Ok(Self {
            http: OpenAiHttp::new(model)?,
            store: model.store,
        })
    }
}

fn to_responses_messages(msg: &ProcessedMessage) -> Vec<serde_json::Value> {
    if msg.role == "tool" {
        if let Some(ref call_id) = msg.tool_call_id {
            let output = match &msg.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Parts(_) => String::new(),
            };
            return vec![serde_json::json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output,
            })];
        }
    }

    if let Some(ref tool_calls) = msg.tool_calls {
        if !tool_calls.is_empty() {
            let mut results: Vec<serde_json::Value> =
                msg.reasoning_items.iter().flatten().cloned().collect();
            results.extend(tool_calls.iter().map(|tc| {
                serde_json::json!({
                    "type": "function_call",
                    "call_id": tc.id,
                    "name": tc.function.name,
                    "arguments": tc.function.arguments,
                })
            }));
            return results;
        }
    }

    let content = match &msg.content {
        MessageContent::Text(text) => serde_json::json!(text),
        MessageContent::Parts(parts) => {
            let mapped: Vec<serde_json::Value> = parts
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => serde_json::json!({
                        "type": "input_text",
                        "text": text,
                    }),
                    ContentPart::ImageUrl { image_url } => serde_json::json!({
                        "type": "input_image",
                        "image_url": image_url.url,
                    }),
                })
                .collect();
            serde_json::json!(mapped)
        }
    };
    vec![serde_json::json!({
        "role": msg.role,
        "content": content,
    })]
}

fn build_request_body(request: &CompletionRequest, stream: bool, store: bool) -> serde_json::Value {
    let mut instructions: Option<String> = None;
    let input_messages: Vec<serde_json::Value> = request
        .messages
        .iter()
        .filter(|m| {
            if m.role == "system" || m.role == "developer" {
                if let MessageContent::Text(ref text) = m.content {
                    instructions = Some(match instructions.take() {
                        Some(existing) => format!("{existing}\n{text}"),
                        None => text.clone(),
                    });
                }
                false
            } else {
                true
            }
        })
        .flat_map(|m| to_responses_messages(m))
        .collect();

    let mut body = serde_json::json!({
        "model": request.model,
        "input": input_messages,
        "stream": stream,
        "store": store,
    });

    let obj = body.as_object_mut().unwrap();
    if let Some(instructions) = instructions {
        obj.insert("instructions".into(), serde_json::json!(instructions));
    }

    if let Some(temp) = request.parameters.temperature {
        obj.insert("temperature".into(), serde_json::json!(temp));
    }
    if let Some(max) = request.parameters.max_tokens {
        obj.insert("max_output_tokens".into(), serde_json::json!(max));
    }
    if let Some(top_p) = request.parameters.top_p {
        obj.insert("top_p".into(), serde_json::json!(top_p));
    }

    encode_responses_capabilities(&request.capabilities, &request.parameters, obj);

    if !request.tool_payloads.is_empty() {
        obj.insert("tools".into(), serde_json::json!(request.tool_payloads));
    }

    for (key, value) in &request.parameters.extra {
        obj.insert(key.clone(), value.clone());
    }

    log::debug!(
        "responses: model={}, reasoning={:?}",
        request.model,
        obj.get("reasoning")
    );

    body
}

#[derive(Deserialize)]
struct NonStreamingResponse {
    output: Vec<OutputItem>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    item_type: String,
    content: Option<Vec<ContentItem>>,
}

#[derive(Deserialize)]
struct ContentItem {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ResponseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    response: Option<ResponseWrapper>,
    #[serde(default)]
    item: Option<serde_json::Value>,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
    #[serde(default)]
    annotation: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct OutputItemEvent {
    #[serde(rename = "type")]
    item_type: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    action: Option<serde_json::Value>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ResponseWrapper {
    #[serde(default)]
    usage: Option<ResponseUsage>,
}

#[derive(Deserialize)]
struct ResponseUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[async_trait]
impl AiProvider for OpenAiResponsesProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<String, AiError> {
        let body = build_request_body(&request, false, self.store);
        let response = self
            .http
            .post("/responses", &body, Some(Duration::from_secs(120)))
            .await?;

        let response_text = response
            .text()
            .await
            .map_err(|e| AiError::Request(format!("failed to read response: {e}")))?;

        log::debug!("responses: complete response_len={}", response_text.len());

        let parsed: NonStreamingResponse = serde_json::from_str(&response_text)
            .map_err(|e| AiError::Request(format!("failed to parse response: {e}")))?;

        parsed
            .output
            .iter()
            .find(|item| item.item_type == "message")
            .and_then(|item| item.content.as_ref())
            .and_then(|parts| parts.first())
            .and_then(|part| part.text.clone())
            .ok_or_else(|| AiError::Request("empty response from API".into()))
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, AiError>> + Send>>, AiError> {
        let body = build_request_body(&request, true, self.store);
        let response = self.http.post("/responses", &body, None).await?;
        Ok(Box::pin(parse_responses_events(parse_sse_stream(response))))
    }
}

const CITATION_SPAN_OPEN: char = '\u{E200}';
const CITATION_SPAN_CLOSE: char = '\u{E201}';
const CITATION_FIELD_SEPARATOR: char = '\u{E202}';
const CITE_SPAN_TYPE: &str = "cite";

#[derive(Default)]
struct StreamState {
    accumulated: String,
    open_span: Option<String>,
    sources: Vec<Source>,
    accumulated_thinking: String,
    active_tool_call_ids: Vec<String>,
    fn_call_args: HashMap<String, (String, String)>,
    item_id_to_call_id: HashMap<String, String>,
    pending_chunks: VecDeque<StreamChunk>,
}

impl StreamState {
    fn push_content(&mut self, delta: String, thinking_delta: Option<String>) {
        self.pending_chunks.push_back(StreamChunk::content(
            delta,
            &self.accumulated,
            thinking_delta,
            &self.accumulated_thinking,
        ));
    }

    fn visible_text(&mut self, delta: &str) -> String {
        let mut visible = String::new();
        for ch in delta.chars() {
            match self.open_span.as_mut() {
                None if ch == CITATION_SPAN_OPEN => self.open_span = Some(ch.to_string()),
                None => visible.push(ch),
                Some(span) => {
                    span.push(ch);
                    if ch == CITATION_SPAN_CLOSE {
                        let span = self.open_span.take().unwrap_or_default();
                        visible.push_str(&resolve_closed_span(span));
                    }
                }
            }
        }
        visible
    }

    fn drop_open_span(&mut self) {
        if let Some(span) = self.open_span.take() {
            log::debug!(
                "responses: open citation span dropped at text end, len={}",
                span.chars().count()
            );
        }
    }

    fn add_url_citation(&mut self, annotation: &serde_json::Value) {
        let text = |key: &str| {
            annotation
                .get(key)
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
        };
        let Some(url) = text("url") else { return };
        if self.sources.iter().any(|s| s.url == url) {
            return;
        }
        let source = Source {
            url: url.to_string(),
            title: text("title").unwrap_or(url).to_string(),
        };
        log::debug!("responses: url_citation source added");
        self.sources.push(source.clone());
        self.pending_chunks
            .push_back(StreamChunk::source(&self.accumulated, source));
    }

    fn push_tool_event(&mut self, event: ToolCallEvent) {
        self.pending_chunks
            .push_back(StreamChunk::tool_call(&self.accumulated, event));
    }

    fn push_reasoning_item(&mut self, item: serde_json::Value) {
        self.pending_chunks
            .push_back(StreamChunk::reasoning_item(&self.accumulated, item));
    }

    fn resolve_call_id(&self, raw_id: String) -> String {
        self.item_id_to_call_id.get(&raw_id).cloned().unwrap_or(raw_id)
    }

    fn start_tool_call(&mut self, tool_call_id: String, tool_name: String) {
        self.active_tool_call_ids.push(tool_call_id.clone());
        self.accumulated
            .push_str(&format!("{{{{tool_call:{tool_call_id}}}}}"));
        self.push_tool_event(ToolCallEvent::Started {
            tool_call_id,
            tool_name,
        });
    }

    fn complete_function_call(
        &mut self,
        call_id: String,
        tool_name: Option<String>,
        arguments: Option<String>,
    ) {
        let (stored_name, stored_arguments) = self.fn_call_args.remove(&call_id).unwrap_or_default();
        let tool_name = tool_name.unwrap_or(stored_name);
        let arguments = arguments.unwrap_or(stored_arguments);
        log::debug!("responses: function_call complete id={call_id} name={tool_name}");
        self.active_tool_call_ids.retain(|id| id != &call_id);
        self.push_tool_event(ToolCallEvent::arguments_complete(call_id, tool_name, &arguments));
    }
}

fn resolve_closed_span(span: String) -> String {
    let span_type: String = span
        .trim_start_matches(CITATION_SPAN_OPEN)
        .split([CITATION_FIELD_SEPARATOR, CITATION_SPAN_CLOSE])
        .next()
        .unwrap_or_default()
        .chars()
        .take(32)
        .collect();
    if span_type == CITE_SPAN_TYPE {
        return String::new();
    }
    log::debug!("responses: non-cite span kept as text, type={span_type}");
    span
}

fn parse_output_item(raw: serde_json::Value) -> Option<OutputItemEvent> {
    serde_json::from_value(raw)
        .map_err(|e| log::warn!("responses: failed to parse output item: {e}"))
        .ok()
}

fn process_event(state: &mut StreamState, event: ResponseEvent) {
    log::trace!("responses: event_type={}", event.event_type);

    match event.event_type.as_str() {
        "response.reasoning_summary_text.delta" => {
            let thinking = event.delta.unwrap_or_default();
            log::trace!("responses: reasoning delta len={}", thinking.len());
            if thinking.is_empty() {
                return;
            }
            state.accumulated_thinking.push_str(&thinking);
            state.push_content(String::new(), Some(thinking));
        }
        "response.output_text.delta" => {
            let delta = state.visible_text(&event.delta.unwrap_or_default());
            if delta.is_empty() {
                return;
            }
            state.accumulated.push_str(&delta);
            state.push_content(delta, None);
        }
        "response.output_text.done" => state.drop_open_span(),
        "response.output_text.annotation.added" => {
            let Some(annotation) = event.annotation else { return };
            match annotation.get("type").and_then(|t| t.as_str()) {
                Some("url_citation") => state.add_url_citation(&annotation),
                other => log::trace!("responses: annotation ignored type={other:?}"),
            }
        }
        "response.output_item.added" => {
            let Some(item) = event.item.and_then(parse_output_item) else {
                return;
            };
            match item.item_type.as_deref() {
                Some("web_search_call") => {
                    let tool_call_id = item.id.unwrap_or_default();
                    log::debug!("responses: web_search_call started id={tool_call_id}");
                    state.start_tool_call(tool_call_id, "web_search".to_string());
                }
                Some("function_call") => {
                    let tool_call_id = item.call_id.unwrap_or_default();
                    let tool_name = item.name.unwrap_or_default();
                    log::debug!("responses: function_call started call_id={tool_call_id} item_id={:?} name={tool_name}", item.id);
                    state
                        .fn_call_args
                        .insert(tool_call_id.clone(), (tool_name.clone(), String::new()));
                    if let Some(item_id) = item.id {
                        state.item_id_to_call_id.insert(item_id, tool_call_id.clone());
                    }
                    state.start_tool_call(tool_call_id, tool_name);
                }
                _ => {}
            }
        }
        "response.function_call_arguments.delta" => {
            let Some(raw_id) = event.call_id.or(event.item_id) else {
                return;
            };
            let call_id = state.resolve_call_id(raw_id);
            if let (Some(delta), Some((_, arguments))) =
                (event.delta, state.fn_call_args.get_mut(&call_id))
            {
                arguments.push_str(&delta);
            }
        }
        "response.function_call_arguments.done" => {
            let raw_id = event.call_id.or(event.item_id);
            log::debug!("responses: fn_call_args.done raw_id={raw_id:?} name={:?} has_arguments={}", event.name, event.arguments.is_some());
            let Some(raw_id) = raw_id else { return };
            let call_id = state.resolve_call_id(raw_id);
            state.complete_function_call(call_id, event.name, event.arguments);
        }
        "response.output_item.done" => {
            let Some(raw) = event.item else { return };
            if raw.get("type").and_then(|t| t.as_str()) == Some("reasoning") {
                log::trace!("responses: reasoning item done");
                state.push_reasoning_item(raw);
                return;
            }
            let Some(item) = parse_output_item(raw.clone()) else {
                return;
            };
            match item.item_type.as_deref() {
                Some("web_search_call") => {
                    let tool_call_id = item.id.unwrap_or_default();
                    log::debug!("responses: web_search_call done id={tool_call_id}");
                    state.active_tool_call_ids.retain(|id| id != &tool_call_id);
                    let result = item
                        .action
                        .as_ref()
                        .and_then(ToolRegistry::format_web_search_result);
                    state.push_tool_event(ToolCallEvent::Done {
                        tool_call_id,
                        result,
                        error: None,
                    });
                    log::trace!("responses: web_search_call item kept for replay");
                    state.push_reasoning_item(raw);
                }
                Some("function_call") => {
                    let tool_call_id = item.call_id.unwrap_or_default();
                    if !state.active_tool_call_ids.contains(&tool_call_id) {
                        log::debug!("responses: function_call output_item.done skipped (already handled) id={tool_call_id}");
                        return;
                    }
                    state.complete_function_call(tool_call_id, item.name, item.arguments);
                }
                _ => {}
            }
        }
        "response.completed" => {
            if let Some(usage) = event.response.and_then(|r| r.usage) {
                state.pending_chunks.push_back(StreamChunk::usage(
                    &state.accumulated,
                    TokenUsage {
                        prompt_tokens: usage.input_tokens,
                        completion_tokens: usage.output_tokens,
                    },
                ));
            }
        }
        _ => {}
    }
}

fn parse_responses_events<E: std::fmt::Display>(
    sse_stream: impl Stream<Item = Result<String, E>> + Unpin + Send + 'static,
) -> impl Stream<Item = Result<StreamChunk, AiError>> + Send {
    futures::stream::unfold(
        (StreamState::default(), sse_stream),
        |(mut state, mut sse_stream)| async move {
            loop {
                if let Some(chunk) = state.pending_chunks.pop_front() {
                    return Some((Ok(chunk), (state, sse_stream)));
                }
                match sse_stream.next().await {
                    Some(Ok(data)) => {
                        log::trace!("responses: SSE event len={}", data.len());
                        match serde_json::from_str::<ResponseEvent>(&data) {
                            Ok(event) => process_event(&mut state, event),
                            Err(e) => log::warn!("responses: failed to parse SSE event: {e}"),
                        }
                    }
                    Some(Err(e)) => {
                        return Some((Err(AiError::Stream(e.to_string())), (state, sse_stream)));
                    }
                    None => return None,
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(events: &[&str]) -> Vec<StreamChunk> {
        let source = futures::stream::iter(
            events
                .iter()
                .map(|e| Ok::<String, String>(e.to_string()))
                .collect::<Vec<_>>(),
        );
        Box::pin(parse_responses_events(source))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect()
    }

    fn tool_events(chunks: &[StreamChunk]) -> Vec<String> {
        chunks
            .iter()
            .filter_map(|c| c.tool_call_event.as_ref())
            .map(|e| match e {
                ToolCallEvent::Started { tool_call_id, tool_name } => {
                    format!("started:{tool_call_id}:{tool_name}")
                }
                ToolCallEvent::ArgumentsComplete { tool_call_id, tool_name, arguments } => {
                    format!("complete:{tool_call_id}:{tool_name}:{arguments}")
                }
                ToolCallEvent::Done { tool_call_id, result, error } => {
                    format!("done:{tool_call_id}:{result:?}:{error:?}")
                }
            })
            .collect()
    }

    const ADDED_1: &str = r#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup"}}"#;
    const ADDED_2: &str = r#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc_2","call_id":"call_2","name":"fetch"}}"#;

    #[tokio::test]
    async fn text_deltas_accumulate_and_usage_follows() {
        let chunks = run(&[
            r#"{"type":"response.output_text.delta","delta":"Hel"}"#,
            r#"{"type":"response.output_text.delta","delta":""}"#,
            "not json",
            r#"{"type":"response.in_progress"}"#,
            r#"{"type":"response.output_text.delta","delta":"lo"}"#,
            r#"{"type":"response.completed","response":{"usage":{"input_tokens":5,"output_tokens":3}}}"#,
        ])
        .await;

        assert_eq!(chunks.len(), 3);
        assert_eq!((chunks[0].delta.as_str(), chunks[0].accumulated.as_str()), ("Hel", "Hel"));
        assert_eq!((chunks[1].delta.as_str(), chunks[1].accumulated.as_str()), ("lo", "Hello"));
        assert!(chunks[0].usage.is_none() && chunks[1].usage.is_none());
        assert_eq!((chunks[2].delta.as_str(), chunks[2].accumulated.as_str()), ("", "Hello"));
        let usage = chunks[2].usage.as_ref().unwrap();
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (5, 3));
    }

    #[tokio::test]
    async fn completed_without_usage_emits_nothing() {
        let chunks = run(&[r#"{"type":"response.completed","response":{}}"#]).await;
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn reasoning_summary_deltas_accumulate_beside_text() {
        let chunks = run(&[
            r#"{"type":"response.reasoning_summary_text.delta","delta":"think"}"#,
            r#"{"type":"response.reasoning_summary_text.delta","delta":"ing"}"#,
            r#"{"type":"response.reasoning_summary_text.delta","delta":""}"#,
            r#"{"type":"response.output_text.delta","delta":"ans"}"#,
        ])
        .await;

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].delta, "");
        assert_eq!(chunks[0].thinking_delta.as_deref(), Some("think"));
        assert_eq!(chunks[0].accumulated_thinking.as_deref(), Some("think"));
        assert_eq!(chunks[1].thinking_delta.as_deref(), Some("ing"));
        assert_eq!(chunks[1].accumulated_thinking.as_deref(), Some("thinking"));
        assert_eq!(chunks[2].delta, "ans");
        assert_eq!(chunks[2].accumulated, "ans");
        assert_eq!(chunks[2].thinking_delta, None);
        assert_eq!(chunks[2].accumulated_thinking.as_deref(), Some("thinking"));
    }

    #[tokio::test]
    async fn one_tool_call_streams_started_then_arguments_complete() {
        let chunks = run(&[
            ADDED_1,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"q\":"}"#,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"\"x\"}"}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1"}"#,
        ])
        .await;

        assert_eq!(
            tool_events(&chunks),
            vec!["started:call_1:lookup", r#"complete:call_1:lookup:{"q":"x"}"#]
        );
        assert_eq!(chunks[0].delta, "");
        assert_eq!(chunks[0].accumulated, "{{tool_call:call_1}}");
        assert_eq!(chunks[1].accumulated, "{{tool_call:call_1}}");
    }

    #[tokio::test]
    async fn arguments_done_payload_wins_over_accumulated_deltas() {
        let chunks = run(&[
            ADDED_1,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"q\":\"old\"}"}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1","name":"renamed","arguments":"{\"q\":\"new\"}"}"#,
        ])
        .await;

        assert_eq!(
            tool_events(&chunks),
            vec!["started:call_1:lookup", r#"complete:call_1:renamed:{"q":"new"}"#]
        );
    }

    #[tokio::test]
    async fn two_interleaved_tool_calls_complete_independently() {
        let chunks = run(&[
            ADDED_1,
            ADDED_2,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"a\":"}"#,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_2","delta":"{\"b\":2}"}"#,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"1}"}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_2"}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1"}"#,
        ])
        .await;

        assert_eq!(
            tool_events(&chunks),
            vec![
                "started:call_1:lookup",
                "started:call_2:fetch",
                r#"complete:call_2:fetch:{"b":2}"#,
                r#"complete:call_1:lookup:{"a":1}"#,
            ]
        );
        assert_eq!(chunks[1].accumulated, "{{tool_call:call_1}}{{tool_call:call_2}}");
    }

    #[tokio::test]
    async fn output_item_done_after_arguments_done_is_not_emitted_again() {
        let chunks = run(&[
            ADDED_1,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1","arguments":"{\"q\":\"x\"}"}"#,
            r#"{"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{\"q\":\"x\"}"}}"#,
        ])
        .await;

        assert_eq!(
            tool_events(&chunks),
            vec!["started:call_1:lookup", r#"complete:call_1:lookup:{"q":"x"}"#]
        );
    }

    #[tokio::test]
    async fn output_item_done_completes_a_call_still_active() {
        let chunks = run(&[
            ADDED_1,
            r#"{"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{\"q\":\"x\"}"}}"#,
            r#"{"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{\"q\":\"x\"}"}}"#,
        ])
        .await;

        assert_eq!(
            tool_events(&chunks),
            vec!["started:call_1:lookup", r#"complete:call_1:lookup:{"q":"x"}"#]
        );
    }

    #[tokio::test]
    async fn unparseable_arguments_become_empty_object() {
        let via_arguments_done = run(&[
            ADDED_1,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1","arguments":"{not json"}"#,
        ])
        .await;
        let via_output_item = run(&[
            ADDED_1,
            r#"{"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{not json"}}"#,
        ])
        .await;

        let expected = vec!["started:call_1:lookup", "complete:call_1:lookup:{}"];
        assert_eq!(tool_events(&via_arguments_done), expected);
        assert_eq!(tool_events(&via_output_item), expected);
    }

    #[tokio::test]
    async fn web_search_call_emits_started_then_done_with_queries() {
        let chunks = run(&[
            r#"{"type":"response.output_item.added","item":{"type":"web_search_call","id":"ws_1"}}"#,
            r#"{"type":"response.web_search_call.searching","item_id":"ws_1"}"#,
            r#"{"type":"response.output_item.done","item":{"type":"web_search_call","id":"ws_1","action":{"queries":["a","b"]}}}"#,
        ])
        .await;

        assert_eq!(
            tool_events(&chunks),
            vec!["started:ws_1:web_search", r#"done:ws_1:Some("a\nb"):None"#]
        );
        assert_eq!(chunks[0].accumulated, "{{tool_call:ws_1}}");
        let items: Vec<_> = chunks.iter().filter_map(|c| c.reasoning_item.as_ref()).collect();
        assert_eq!(
            items,
            vec![&serde_json::json!({"type":"web_search_call","id":"ws_1","action":{"queries":["a","b"]}})]
        );
    }

    #[tokio::test]
    async fn web_search_call_is_replayed_in_stream_order_among_reasoning_items() {
        use crate::models::capabilities::ModelCapabilities;

        let chunks = run(&[
            r#"{"type":"response.output_item.done","item":{"type":"reasoning","id":"rs_1","summary":[]}}"#,
            r#"{"type":"response.output_item.added","item":{"type":"web_search_call","id":"ws_1"}}"#,
            r#"{"type":"response.output_item.done","item":{"type":"web_search_call","id":"ws_1","action":{"queries":["a"]}}}"#,
            r#"{"type":"response.output_item.done","item":{"type":"reasoning","id":"rs_2","summary":[]}}"#,
            ADDED_1,
            r#"{"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{}"}}"#,
        ])
        .await;
        let items: Vec<serde_json::Value> = chunks
            .iter()
            .filter_map(|c| c.reasoning_item.clone())
            .collect();
        let request = request_with(
            vec![
                ProcessedMessage::text("user", "hi"),
                ProcessedMessage {
                    tool_calls: Some(vec![tool_call_payload("call_1")]),
                    reasoning_items: Some(items),
                    ..ProcessedMessage::text("assistant", "")
                },
                ProcessedMessage {
                    tool_call_id: Some("call_1".into()),
                    ..ProcessedMessage::text("tool", "ok")
                },
            ],
            ModelCapabilities::minimal(),
        );

        let body = build_request_body(&request, true, false);

        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 6);
        assert_eq!(input[1], serde_json::json!({"type":"reasoning","id":"rs_1","summary":[]}));
        assert_eq!(
            input[2],
            serde_json::json!({"type":"web_search_call","id":"ws_1","action":{"queries":["a"]}})
        );
        assert_eq!(input[3], serde_json::json!({"type":"reasoning","id":"rs_2","summary":[]}));
        assert_eq!(input[4]["type"], "function_call");
        assert_eq!(input[4]["call_id"], "call_1");
        assert_eq!(input[5]["type"], "function_call_output");
        assert_eq!(input[5]["call_id"], "call_1");
    }

    #[tokio::test]
    async fn reasoning_item_done_is_emitted_whole_beside_a_function_call() {
        let chunks = run(&[
            r#"{"type":"response.output_item.done","item":{"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"enc"}}"#,
            ADDED_1,
            r#"{"type":"response.output_item.done","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"{}"}}"#,
        ])
        .await;

        let items: Vec<_> = chunks.iter().filter_map(|c| c.reasoning_item.as_ref()).collect();
        assert_eq!(items, vec![&reasoning_item()]);
        assert_eq!(
            tool_events(&chunks),
            vec!["started:call_1:lookup", "complete:call_1:lookup:{}"]
        );
    }

    fn reasoning_item() -> serde_json::Value {
        serde_json::json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "enc"})
    }

    fn tool_call_payload(id: &str) -> crate::models::message::ToolCallPayload {
        crate::models::message::ToolCallPayload {
            id: id.into(),
            call_type: "function".into(),
            function: crate::models::message::ToolCallFunction {
                name: "lookup".into(),
                arguments: "{}".into(),
            },
        }
    }

    fn request_with(
        messages: Vec<ProcessedMessage>,
        capabilities: crate::models::capabilities::ModelCapabilities,
    ) -> CompletionRequest {
        CompletionRequest {
            model: "m".into(),
            messages,
            parameters: Default::default(),
            tool_payloads: vec![],
            capabilities,
        }
    }

    fn assistant_tool_call_message() -> ProcessedMessage {
        ProcessedMessage {
            tool_calls: Some(vec![tool_call_payload("call_1")]),
            reasoning_items: Some(vec![reasoning_item()]),
            ..ProcessedMessage::text("assistant", "")
        }
    }

    #[test]
    fn request_puts_reasoning_item_right_before_its_function_call() {
        use crate::models::capabilities::{Effort, ModelCapabilities, ReasoningMode};

        let capabilities = ModelCapabilities {
            reasoning: ReasoningMode::Effort { allowed: vec![Effort::Medium] },
        };
        let request = request_with(
            vec![
                ProcessedMessage::text("user", "hi"),
                assistant_tool_call_message(),
                ProcessedMessage {
                    tool_call_id: Some("call_1".into()),
                    ..ProcessedMessage::text("tool", "ok")
                },
            ],
            capabilities,
        );

        let body = build_request_body(&request, true, false);

        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 4);
        assert_eq!(input[1], reasoning_item());
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "call_1");
        assert_eq!(body["include"], serde_json::json!(["reasoning.encrypted_content"]));
    }

    #[test]
    fn request_has_no_include_without_effort_capability() {
        let request = request_with(
            vec![assistant_tool_call_message()],
            crate::models::capabilities::ModelCapabilities::minimal(),
        );

        let body = build_request_body(&request, true, false);

        assert!(body.get("include").is_none());
    }

    fn text_delta(text: &str) -> String {
        serde_json::json!({"type": "response.output_text.delta", "delta": text}).to_string()
    }

    fn is_marker_char(c: char) -> bool {
        ('\u{E200}'..='\u{E202}').contains(&c)
    }

    #[tokio::test]
    async fn cite_span_split_across_deltas_never_reaches_a_chunk() {
        let deltas = [
            text_delta("ol. \u{E200}ci"),
            text_delta("te\u{E202}turn777310view1\u{E202}turn7773"),
            text_delta("10view0\u{E202}turn777310view2\u{E201}\n\nPrzetestuję"),
        ];
        let events: Vec<&str> = deltas.iter().map(String::as_str).collect();

        let chunks = run(&events).await;

        assert!(chunks
            .iter()
            .all(|c| !c.delta.chars().any(is_marker_char) && !c.accumulated.chars().any(is_marker_char)));
        assert_eq!(chunks.last().unwrap().accumulated, "ol. \n\nPrzetestuję");
    }

    #[tokio::test]
    async fn cite_span_whole_in_one_delta_is_dropped() {
        let chunks = run(&[&text_delta("A \u{E200}cite\u{E202}turn1view0\u{E201}B")]).await;

        assert_eq!(chunks.last().unwrap().accumulated, "A B");
    }

    #[tokio::test]
    async fn span_open_at_text_done_is_dropped_before_the_next_item() {
        let chunks = run(&[
            &text_delta("A "),
            &text_delta("\u{E200}cite\u{E202}turn1view0"),
            r#"{"type":"response.output_text.done"}"#,
            ADDED_1,
        ])
        .await;

        let accumulated: Vec<&str> = chunks.iter().map(|c| c.accumulated.as_str()).collect();
        assert_eq!(accumulated, vec!["A ", "A {{tool_call:call_1}}"]);
    }

    #[tokio::test]
    async fn non_cite_span_is_kept_verbatim() {
        let input = "x \u{E200}other\u{E202}y\u{E201} z";

        let chunks = run(&[&text_delta(input)]).await;

        assert_eq!(chunks.last().unwrap().accumulated, input);
    }

    #[tokio::test]
    async fn url_citation_annotations_become_deduplicated_source_chunks() {
        let annotation = |body: &str| {
            format!(
                r#"{{"type":"response.output_text.annotation.added","item_id":"msg_1","output_index":1,"content_index":0,"annotation_index":0,"annotation":{body}}}"#
            )
        };
        let a = annotation(r#"{"type":"url_citation","url":"https://a.example/x","title":"A","start_index":0,"end_index":5}"#);
        let b = annotation(r#"{"type":"url_citation","url":"https://b.example","start_index":0,"end_index":5}"#);
        let file = annotation(r#"{"type":"file_citation","file_id":"file_1","filename":"f.txt","index":0}"#);

        let chunks = run(&[&a, &file, &a, &b]).await;

        let sources: Vec<Source> = chunks.into_iter().filter_map(|c| c.source).collect();
        assert_eq!(
            sources,
            vec![
                Source { url: "https://a.example/x".into(), title: "A".into() },
                Source { url: "https://b.example".into(), title: "https://b.example".into() },
            ]
        );
    }

    #[tokio::test]
    async fn source_error_becomes_stream_error() {
        let source = futures::stream::iter(vec![Err::<String, String>("boom".into())]);
        let results = Box::pin(parse_responses_events(source)).collect::<Vec<_>>().await;

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], Err(AiError::Stream(m)) if m == "boom"));
    }
}
