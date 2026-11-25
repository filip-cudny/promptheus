use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::{watch, Mutex as TokioMutex};

use crate::models::message::ProcessedMessage;
use crate::models::settings::{ApiMode, Provider, Settings};
use crate::services::ai::AiService;
use crate::services::change_log::Target;
use crate::services::config::catalog;
use crate::services::execution::lifecycle::LiveExecution;
use crate::services::execution::stream::{build_tool_loop_messages, run_stream_loop};
use crate::services::execution::types::{
    PendingToolCall, ToolExecutionResult, MAX_TOOL_LOOP_ITERATIONS,
};
use crate::services::mcp::registry::without_disabled_tools;
use crate::services::placeholder_registry::PlaceholderContext;
use crate::services::secrets;
use crate::Error;

use super::dispatch::{Context, Dispatcher, Proposal, ProposalSink, SecretViewSink};
use super::mcp::{select_enabled, McpSource};
use super::operations::Operation;
use super::plan::{plan, stale_rows, Current, PlannedRow, RowError};
use super::tools;
use super::transcript::{self, End, Iteration, ToolCallRecord, Transcript};

pub const PROPOSAL_EVENT: &str = "settings-agent-proposal";
pub const SECRET_VIEW_EVENT: &str = "settings-agent-secret-view";
const BUILT_IN_SEARCH: &str = "web_search";

pub trait Emit: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

#[derive(Default)]
pub struct Session {
    messages: Vec<ProcessedMessage>,
    pub last_request: Option<String>,
    pub last_reply: Option<String>,
    pub proposal: Option<Proposal>,
    cancel: Option<watch::Sender<bool>>,
    generation: u64,
}

pub type SharedSession = Arc<Mutex<Session>>;

pub fn lock(session: &SharedSession) -> MutexGuard<'_, Session> {
    session.lock().unwrap_or_else(|e| e.into_inner())
}

impl Session {
    pub fn cancel(&mut self) {
        if let Some(cancel) = &self.cancel {
            let _ = cancel.send(true);
        }
    }

    pub fn reset(&mut self) {
        self.cancel();
        self.messages.clear();
        self.last_request = None;
        self.last_reply = None;
        self.proposal = None;
        self.generation += 1;
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|cancel| *cancel.borrow())
    }
}

pub struct EngineContext {
    pub settings: Arc<dyn Fn() -> Settings + Send + Sync>,
    pub config_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub ai: AiService,
    pub ai_factory: Arc<dyn Fn(&Settings) -> AiService + Send + Sync>,
    pub placeholders: PlaceholderContext,
    pub emit: Arc<dyn Emit>,
    pub transcript_dir: PathBuf,
    pub mcp: Arc<dyn McpSource>,
}

struct SessionSink {
    session: SharedSession,
    settings: Arc<dyn Fn() -> Settings + Send + Sync>,
    config_dir: PathBuf,
    emit: Arc<dyn Emit>,
}

impl ProposalSink for SessionSink {
    fn proposed(&self, proposal: &Proposal) {
        {
            let mut s = lock(&self.session);
            if s.is_cancelled() {
                return;
            }
            s.proposal = Some(proposal.clone());
        }
        let settings = (self.settings)();
        let current = Current { settings: &settings, config_dir: &self.config_dir };
        self.emit.emit(PROPOSAL_EVENT, proposal_payload(proposal, &current));
    }
}

impl SecretViewSink for SessionSink {
    fn shown(&self, name: &str) {
        if lock(&self.session).is_cancelled() {
            return;
        }
        self.emit.emit(SECRET_VIEW_EVENT, json!({"name": name}));
    }
}

pub fn proposal_payload(proposal: &Proposal, current: &Current) -> Value {
    let stale = stale_rows(&proposal.plan, current);
    let rows: Vec<Value> = proposal
        .plan
        .rows
        .iter()
        .map(|planned| {
            json!({
                "row": planned.row,
                "label": planned.operation.label,
                "op": planned.operation.op,
                "effects": planned.operation.effects,
                "test": proposal.tests.get(&planned.row),
                "waits_for": proposal.waits.get(&planned.row).cloned().unwrap_or_default(),
                "new_destination": proposal.new_destinations.get(&planned.row),
                "sample_input": sample_input(proposal.operations.get(planned.row)),
                "stale": stale.contains(&planned.row),
                "env_value": env_value_of(proposal, planned.row, current),
                "model_names": model_names_of(proposal, planned, current),
            })
        })
        .collect();
    json!({"proposal_id": proposal.id, "rows": rows})
}

fn model_names_of(proposal: &Proposal, planned: &PlannedRow, current: &Current) -> Value {
    let Some(Operation::SetSetting { key, value }) = proposal.operations.get(planned.row) else {
        return Value::Null;
    };
    let catalog_key = catalog::catalog_path(key);
    let is_model_reference = catalog::ENTRIES
        .iter()
        .any(|entry| entry.key == catalog_key && matches!(entry.kind, catalog::Kind::Model { .. }));
    if !is_model_reference {
        return Value::Null;
    }
    let before = planned.operation.effects.iter().find_map(|effect| match &effect.target {
        Target::Setting { path } if path == key => effect.before.as_ref().and_then(Value::as_str),
        _ => None,
    });
    let display_name = |id: &str| {
        [&proposal.plan.settings, current.settings]
            .iter()
            .find_map(|settings| settings.models.iter().find(|model| model.id == id))
            .map(|model| model.display_name.as_str())
    };
    json!({
        "before": before.and_then(display_name),
        "after": value.as_str().and_then(display_name),
    })
}

fn env_value_of(proposal: &Proposal, row: usize, current: &Current) -> Value {
    let Some(Operation::SetEnvValue { name }) = proposal.operations.get(row) else {
        return Value::Null;
    };
    let secret = proposal
        .plan
        .settings
        .env_vars
        .iter()
        .find(|var| &var.name == name)
        .is_some_and(|var| var.secret);
    let set = secrets::current_value(current.config_dir, name).is_some_and(|value| !value.is_empty());
    json!({"name": name, "secret": secret, "set": set})
}

fn sample_input(operation: Option<&Operation>) -> Option<&str> {
    match operation? {
        Operation::CreateSkill { sample_input, .. } | Operation::UpdateSkill { sample_input, .. } => {
            Some(sample_input)
        }
        _ => None,
    }
}

pub fn refresh(proposal: &Proposal, current: &Current) -> Result<Proposal, Vec<RowError>> {
    let operations = &proposal.operations;
    let planned = plan(operations, &vec![true; operations.len()], current)?;
    Ok(Proposal { plan: planned, ..proposal.clone() })
}

pub fn describe(errors: &[RowError]) -> String {
    errors
        .iter()
        .map(|e| format!("row {}: {}", e.row + 1, e.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn system_prompt() -> String {
    let entries = serde_json::to_string(catalog::ENTRIES).unwrap_or_default();
    format!("{}{entries}\n", include_str!("system_prompt.md"))
}

fn begin(
    session: &SharedSession,
    user_text: &str,
) -> crate::Result<(Vec<ProcessedMessage>, u64, watch::Receiver<bool>)> {
    let mut s = lock(session);
    if s.cancel.is_some() {
        return Err(Error::Other("the settings agent is already working on a request".into()));
    }
    let (cancel, cancel_rx) = watch::channel(false);
    s.cancel = Some(cancel);
    if s.messages.is_empty() {
        s.messages.push(ProcessedMessage::text("system", system_prompt()));
    }
    s.last_request = Some(user_text.to_string());
    s.last_reply = None;
    let mut messages = s.messages.clone();
    messages.push(ProcessedMessage::text("user", user_text));
    Ok((messages, s.generation, cancel_rx))
}

async fn fail(live: &Arc<TokioMutex<LiveExecution>>, message: String) -> Error {
    live.lock().await.finish_with_error(message.clone());
    Error::Other(message)
}

async fn agent_model(
    settings: &Settings,
    ai: &AiService,
    live: &Arc<TokioMutex<LiveExecution>>,
) -> crate::Result<String> {
    let agent_model_id = settings
        .surfaces
        .settings_agent
        .generation
        .model_id
        .clone()
        .filter(|id| !id.is_empty());
    let Some(model_id) = agent_model_id.or_else(|| settings.surfaces.chat.generation.model_id.clone())
    else {
        return Err(fail(live, "no settings agent or chat model configured".into()).await);
    };
    if !ai.has_model(&model_id) {
        return Err(fail(live, format!("model '{model_id}' is not available")).await);
    }
    Ok(model_id)
}

async fn tool_loop(
    messages: &mut Vec<ProcessedMessage>,
    session: &SharedSession,
    ctx: &EngineContext,
    live: &Arc<TokioMutex<LiveExecution>>,
    cancel_rx: watch::Receiver<bool>,
    transcript: &mut Transcript,
) -> crate::Result<String> {
    let sink = Arc::new(SessionSink {
        session: Arc::clone(session),
        settings: Arc::clone(&ctx.settings),
        config_dir: ctx.config_dir.clone(),
        emit: Arc::clone(&ctx.emit),
    });
    let settings = (ctx.settings)();
    let (mcp_names, built_in_names): (Vec<String>, Vec<String>) = settings
        .surfaces
        .settings_agent
        .generation
        .enabled_tools
        .iter()
        .cloned()
        .partition(|name| name.contains('.'));
    let mcp_names = without_disabled_tools(&mcp_names, &settings.mcp_servers);
    let enabled = select_enabled(&*ctx.mcp, &mcp_names);
    let request_tools: Vec<_> = tools::definitions().into_iter().chain(enabled.definitions).collect();
    let settings_source = Arc::clone(&ctx.settings);
    let factory = Arc::clone(&ctx.ai_factory);
    let dispatcher = Dispatcher::new(Context {
        settings: Box::new(move || settings_source()),
        config_dir: ctx.config_dir.clone(),
        skills_dir: ctx.skills_dir.clone(),
        ai_factory: Box::new(move |settings| factory(settings)),
        placeholders: ctx.placeholders.clone(),
        proposals: sink.clone(),
        secret_views: sink,
        mcp: Arc::clone(&ctx.mcp),
        mcp_tools: enabled.qualified_by_raw,
    });
    let parameters = settings
        .surfaces
        .chat
        .generation
        .parameters
        .merged_with(&settings.surfaces.settings_agent.generation.parameters);
    transcript.parameters = parameters.clone();
    let model_id = agent_model(&settings, &ctx.ai, live).await?;
    transcript.model_id = Some(model_id.clone());
    log::debug!("settings agent turn starts on model '{model_id}'");
    let tools_override = built_in_tools(&built_in_names, &settings, &model_id);

    let mut prefix = String::new();
    let mut last_tool_error: Option<String> = None;
    for iteration in 0..=MAX_TOOL_LOOP_ITERATIONS {
        if *cancel_rx.borrow() {
            return Err(fail(live, "Cancelled".into()).await);
        }
        let round_started = Instant::now();
        let round_started_at = Utc::now();
        let stream = match ctx
            .ai
            .complete_stream(
                &model_id,
                messages.clone(),
                Some(parameters.clone()),
                tools_override.clone(),
                request_tools.clone(),
            )
            .await
        {
            Ok(stream) => stream,
            Err(e) => return Err(fail(live, e.to_string()).await),
        };
        let snapshot_calls_before = live.lock().await.snapshot.tool_calls.len();
        let result = run_stream_loop(stream, Arc::clone(live), Some(cancel_rx.clone()), &prefix).await?;
        let built_in_records = built_in_search_records(live, snapshot_calls_before, &result.pending_tool_calls).await;
        let mut round = Iteration {
            started_at: transcript::timestamp(round_started_at),
            millis: elapsed_millis(round_started),
            text: result.full_text.strip_prefix(prefix.as_str()).unwrap_or(&result.full_text).to_string(),
            thinking: result.full_thinking.clone(),
            reasoning_items: result.reasoning_items.len(),
            tool_calls: built_in_records,
        };
        if result.pending_tool_calls.is_empty() {
            transcript.iterations.push(round);
            if !result.full_text.is_empty() {
                messages.push(ProcessedMessage::text("assistant", result.full_text.clone()));
            }
            return Ok(result.full_text);
        }
        if iteration == MAX_TOOL_LOOP_ITERATIONS {
            log::warn!("settings agent tool loop reached {MAX_TOOL_LOOP_ITERATIONS} iterations");
            let error = stop_at_limit(live, &result.pending_tool_calls, last_tool_error).await;
            round.tool_calls.extend(result.pending_tool_calls.iter().map(not_run_record));
            transcript.iterations.push(round);
            transcript.end = Some(End::LoopLimit { error: error.to_string() });
            return Err(error);
        }
        prefix.clone_from(&result.full_text);
        live.lock().await.snapshot.finished = false;
        let dispatched = dispatch_all(
            &dispatcher,
            &result.pending_tool_calls,
            live,
            &cancel_rx,
            &mut round.tool_calls,
        )
        .await;
        transcript.iterations.push(round);
        let Some(results) = dispatched else {
            return Err(fail(live, "Cancelled".into()).await);
        };
        if let Some(error) = results.iter().rev().find_map(tool_error) {
            last_tool_error = Some(error);
        }
        messages.extend(build_tool_loop_messages(
            &result.pending_tool_calls,
            &result.full_text,
            &result.reasoning_items,
            &results,
        ));
    }
    unreachable!("the loop returns at its last iteration")
}

fn is_built_in_search_model(settings: &Settings, model_id: &str) -> bool {
    settings.models.iter().find(|model| model.id == model_id).is_some_and(|model| {
        matches!(model.provider, None | Some(Provider::Openai))
            && matches!(model.api_mode, None | Some(ApiMode::Responses))
    })
}

fn built_in_tools(built_in_names: &[String], settings: &Settings, model_id: &str) -> Option<Vec<String>> {
    if !built_in_names.iter().any(|name| name == BUILT_IN_SEARCH) {
        return None;
    }
    if !is_built_in_search_model(settings, model_id) {
        log::debug!("settings agent: built-in web search is not sent, model '{model_id}' is not an OpenAI Responses model");
        return None;
    }
    Some(vec![BUILT_IN_SEARCH.to_string()])
}

async fn built_in_search_records(
    live: &Arc<TokioMutex<LiveExecution>>,
    snapshot_calls_before: usize,
    pending: &[PendingToolCall],
) -> Vec<ToolCallRecord> {
    live.lock()
        .await
        .snapshot
        .tool_calls
        .iter()
        .skip(snapshot_calls_before)
        .filter(|call| !pending.iter().any(|p| p.tool_call_id == call.tool_call_id))
        .map(|call| ToolCallRecord {
            id: call.tool_call_id.clone(),
            name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
            result: call.result.clone(),
            error: call.error.clone(),
            millis: 0,
        })
        .collect()
}

fn tool_error(result: &ToolExecutionResult) -> Option<String> {
    if result.is_error {
        return Some(result.result_text.clone());
    }
    let parsed: Value = serde_json::from_str(&result.result_text).ok()?;
    let last = parsed.get("errors")?.as_array()?.last()?;
    Some(format!("row {}: {}", last["row"], last["message"].as_str().unwrap_or_default()))
}

fn not_run_text() -> String {
    format!("not run: the agent reached the limit of {MAX_TOOL_LOOP_ITERATIONS} tool rounds")
}

fn not_run_record(call: &PendingToolCall) -> ToolCallRecord {
    ToolCallRecord {
        id: call.tool_call_id.clone(),
        name: call.tool_name.clone(),
        arguments: call.arguments.clone(),
        result: None,
        error: Some(not_run_text()),
        millis: 0,
    }
}

fn elapsed_millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

async fn stop_at_limit(
    live: &Arc<TokioMutex<LiveExecution>>,
    pending: &[PendingToolCall],
    last_tool_error: Option<String>,
) -> Error {
    let not_run = not_run_text();
    {
        let mut live = live.lock().await;
        for call in pending {
            live.finish_tool_call(call.tool_call_id.clone(), None, Some(not_run.clone()));
        }
    }
    let mut message = format!("Stopped after {MAX_TOOL_LOOP_ITERATIONS} tool rounds without finishing.");
    if let Some(error) = last_tool_error {
        message.push_str(&format!(" Last tool error: {error}"));
    }
    fail(live, message).await
}

async fn dispatch_all(
    dispatcher: &Dispatcher,
    pending: &[PendingToolCall],
    live: &Arc<TokioMutex<LiveExecution>>,
    cancel_rx: &watch::Receiver<bool>,
    records: &mut Vec<ToolCallRecord>,
) -> Option<Vec<ToolExecutionResult>> {
    let mut results = Vec::with_capacity(pending.len());
    for call in pending {
        if *cancel_rx.borrow() {
            return None;
        }
        let started = Instant::now();
        let result = dispatcher.dispatch(call).await;
        records.push(ToolCallRecord {
            id: call.tool_call_id.clone(),
            name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
            result: (!result.is_error).then(|| result.result_text.clone()),
            error: result.is_error.then(|| result.result_text.clone()),
            millis: elapsed_millis(started),
        });
        let (output, error) = if result.is_error {
            (None, Some(result.result_text.clone()))
        } else {
            (Some(result.result_text.clone()), None)
        };
        live.lock().await.finish_tool_call(result.tool_call_id.clone(), output, error);
        results.push(result);
    }
    Some(results)
}

fn end_of(outcome: &crate::Result<String>) -> End {
    match outcome {
        Ok(reply) => End::Reply { reply: reply.clone() },
        Err(Error::Other(message)) if message == "Cancelled" => End::Cancelled,
        Err(e) => End::Error { error: e.to_string() },
    }
}

fn save_transcript(ctx: &EngineContext, mut transcript: Transcript, outcome: &crate::Result<String>) {
    let end = transcript.end.take().unwrap_or_else(|| end_of(outcome));
    let settings = (ctx.settings)();
    let redactor = secrets::Redactor::new(&ctx.config_dir, &settings);
    transcript.finish(end, Utc::now(), |text| redactor.redact(text));
    match transcript::write(&ctx.transcript_dir, &transcript) {
        Ok(path) => log::debug!("settings agent turn {} saved to {}", transcript.id, path.display()),
        Err(e) => log::warn!("settings agent turn {}: cannot save the transcript: {e}", transcript.id),
    }
}

pub async fn run_turn(
    session: &SharedSession,
    user_text: &str,
    ctx: EngineContext,
    live: Arc<TokioMutex<LiveExecution>>,
) -> crate::Result<()> {
    let (mut messages, generation, cancel_rx) = begin(session, user_text)?;
    log::info!("settings agent turn started");
    let mut transcript = Transcript::start(user_text, Utc::now());
    let outcome = tool_loop(&mut messages, session, &ctx, &live, cancel_rx, &mut transcript).await;
    save_transcript(&ctx, transcript, &outcome);
    let mut s = lock(session);
    s.cancel = None;
    if s.generation == generation {
        s.messages = messages;
        if let Ok(reply) = &outcome {
            s.last_reply = Some(reply.clone());
        }
    }
    log::info!("settings agent turn finished, ok={}", outcome.is_ok());
    outcome.map(|_| ())
}
