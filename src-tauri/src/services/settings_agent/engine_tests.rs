use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rmcp::model::Tool;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::Mutex as TokioMutex;

use super::apply::{apply, Context as ApplyContext};
use super::dispatch::Proposal;
use super::engine::{
    lock, proposal_payload, run_turn, Emit, EngineContext, SharedSession, PROPOSAL_EVENT,
    SECRET_VIEW_EVENT,
};
use super::mcp::{McpCallResult, McpSource, ScriptedMcp};
use super::operations::Operation;
use super::plan::{plan, Current};
use super::testing::{run_row_test, RowTest, RowTestRequest};
use crate::models::ai::ToolCallStatus;
use crate::models::capabilities::{Effort, ModelCapabilities, ReasoningMode};
use crate::models::settings::{ApiMode, EnvVar, ModelConfig, ModelType, Provider, Settings};
use crate::services::ai::mock::{MockProvider, MockReply};
use crate::services::ai::{AiError, AiService};
use crate::services::change_log::{change_log_dir, load_all};
use crate::services::config::ConfigService;
use crate::services::env_resolve::set_override;
use crate::services::execution::lifecycle::{ExecutionSnapshot, LiveExecution};
use crate::services::execution::types::MAX_TOOL_LOOP_ITERATIONS;
use crate::services::placeholder_registry::PlaceholderContext;

const SECRETS: [&str; 4] = ["sk-env-111", "sk-literal-333", "sk-new-444", "mcp-literal-666"];
const STAGED_VALUE: &str = "sk-new-444";

fn model(id: &str, display_name: &str, api_key: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        model: "m".to_string(),
        display_name: display_name.to_string(),
        model_type: ModelType::Text,
        provider: Some(Provider::default()),
        group: None,
        api_key: Some(api_key.to_string()),
        base_url: None,
        parameters: None,
        context_window_size: None,
        api_mode: None,
        capabilities: None,
        store: true,
    }
}

fn fixture_settings(chat_model: Option<&str>) -> Settings {
    let mut settings = Settings::default();
    settings.models = vec![
        model("env-model", "Env", "${OPENAI_KEY}"),
        model("literal-model", "Literal", "sk-literal-333"),
    ];
    settings.surfaces.chat.generation.model_id = chat_model.map(str::to_string);
    settings.mcp_servers.insert(
        "srv".to_string(),
        serde_json::from_value(json!({"command": "run", "env": {"TOKEN": "mcp-literal-666"}})).unwrap(),
    );
    settings
}

struct Frontend {
    session: SharedSession,
    events: Mutex<Vec<(String, Value)>>,
    cancel_on_secret_view: bool,
}

impl Emit for Frontend {
    fn emit(&self, event: &str, payload: Value) {
        self.events.lock().unwrap().push((event.to_string(), payload));
        if event == SECRET_VIEW_EVENT && self.cancel_on_secret_view {
            lock(&self.session).cancel();
        }
    }
}

struct Run {
    dir: TempDir,
    session: SharedSession,
    frontend: Arc<Frontend>,
    requests: Arc<Mutex<Vec<String>>>,
    live: Arc<TokioMutex<LiveExecution>>,
    outcome: crate::Result<()>,
}

fn call(id: &str, tool: &str, arguments: Value) -> MockReply {
    MockReply::ToolCalls(vec![(id.to_string(), tool.to_string(), arguments)])
}

fn scripted_turns() -> Vec<MockReply> {
    let operations = json!([
        {"op": "add_model", "model": model("new-model", "New", "${NEW_KEY}")},
        {"op": "update_model", "id": "literal-model", "model": model("literal-model", "Literal renamed", "")},
    ]);
    vec![
        call("c1", "settings_read", json!({"what": "values"})),
        call("c2", "settings_read", json!({"what": "models"})),
        call("c3", "settings_secret", json!({"name": "OPENAI_KEY"})),
        call(
            "c4",
            "settings_propose_changes",
            json!({"operations": operations}),
        ),
        MockReply::Text("Proposed two changes.".to_string()),
    ]
}

async fn run(chat_model: Option<&str>, script: Vec<MockReply>) -> Run {
    let (provider, requests) = MockProvider::new(script);
    let ai = AiService::with_provider("env-model", Box::new(provider));
    run_with(fixture_settings(chat_model), ai, requests).await
}

async fn run_with(settings: Settings, ai: AiService, requests: Arc<Mutex<Vec<String>>>) -> Run {
    run_configured(settings, ai, requests, false, Arc::new(ScriptedMcp::offering(&[], ""))).await
}

async fn run_configured(
    settings: Settings,
    ai: AiService,
    requests: Arc<Mutex<Vec<String>>>,
    cancel_on_secret_view: bool,
    mcp: Arc<dyn McpSource>,
) -> Run {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(".env"), "OPENAI_KEY=sk-env-111\n").unwrap();
    std::fs::write(dir.path().join("settings.json"), serde_json::to_string_pretty(&settings).unwrap()).unwrap();
    std::fs::create_dir_all(dir.path().join("skills")).unwrap();

    let session = SharedSession::default();
    let frontend = Arc::new(Frontend {
        session: session.clone(),
        events: Mutex::default(),
        cancel_on_secret_view,
    });
    let ctx = EngineContext {
        settings: Arc::new(move || settings.clone()),
        config_dir: dir.path().to_path_buf(),
        skills_dir: dir.path().join("skills"),
        ai,
        ai_factory: Arc::new(|_| {
            let error = AiError::ApiStatus {
                status: 401,
                message: "Incorrect API key provided: sk-env-111".into(),
            };
            let (provider, _) = MockProvider::new(vec![MockReply::Error(error)]);
            AiService::with_provider("literal-model", Box::new(provider))
        }),
        placeholders: PlaceholderContext::with_apps("app", "apps"),
        emit: frontend.clone(),
        transcript_dir: dir.path().join("settings-agent-turns"),
        mcp,
    };
    let live = Arc::new(TokioMutex::new(LiveExecution {
        snapshot: ExecutionSnapshot {
            execution_id: "req".into(),
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
    }));
    let outcome = run_turn(&session, "add a model with a new key", ctx, live.clone()).await;
    set_override("NEW_KEY", None);
    Run { dir, session, frontend, requests, live, outcome }
}

fn only_transcript(run: &Run) -> (String, Value) {
    let dir = run.dir.path().join("settings-agent-turns");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(files.len(), 1);
    let path = files.remove(0);
    let text = std::fs::read_to_string(path).unwrap();
    let value = serde_json::from_str(&text).unwrap();
    (text, value)
}

fn env_file(run: &Run) -> String {
    std::fs::read_to_string(run.dir.path().join(".env")).unwrap()
}

struct EchoingMcp {
    offered: ScriptedMcp,
    replies: Mutex<VecDeque<String>>,
}

#[async_trait]
impl McpSource for EchoingMcp {
    fn tools(&self, qualified_names: &[String]) -> Vec<(String, Tool)> {
        self.offered.tools(qualified_names)
    }

    async fn call(&self, _qualified_name: &str, _arguments: Value) -> McpCallResult {
        let text = self.replies.lock().unwrap().pop_front().unwrap();
        McpCallResult { text, is_error: false }
    }
}

fn every_secret_path_turn() -> Vec<MockReply> {
    let update_row = json!([
        {"op": "update_model", "id": "literal-model", "model": model("literal-model", "Literal renamed", "")},
    ]);
    let card_rows = json!([
        {"op": "add_env_var", "name": "NEW_KEY", "description": "new key", "secret": true},
        {"op": "set_env_value", "name": "NEW_KEY"},
        {"op": "add_model", "model": model("new-model", "New", "${NEW_KEY}")},
    ]);
    vec![
        call("c1", "settings_read", json!({"what": "values"})),
        call("c2", "settings_secret", json!({"name": "OPENAI_KEY"})),
        call("c3", "settings_propose_changes", json!({"operations": update_row})),
        call("c4", "settings_propose_changes", json!({"operations": card_rows})),
        call("c5", "web_search", json!({"query": "first"})),
        call("c6", "web_search", json!({"query": "second"})),
        MockReply::Text("Proposed the card.".to_string()),
    ]
}

fn read_files(dir: &std::path::Path) -> String {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect()
}

fn assert_no_secret(place: &str, text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{secret} leaked in {place}: {text}");
    }
}

#[tokio::test]
async fn no_secret_reaches_any_provider_request() {
    let mut settings = fixture_settings(Some("env-model"));
    settings.surfaces.settings_agent.generation.enabled_tools = vec!["srv.web_search".to_string()];
    let mcp = Arc::new(EchoingMcp {
        offered: ScriptedMcp::offering(&["srv.web_search"], ""),
        replies: Mutex::new(VecDeque::from([
            "token is sk-env-111 end".to_string(),
            "token is mcp-literal-666 end".to_string(),
        ])),
    });
    let (provider, requests) = MockProvider::new(every_secret_path_turn());
    let ai = AiService::with_provider("env-model", Box::new(provider));

    let run = run_configured(settings.clone(), ai, requests, false, mcp).await;

    run.outcome.as_ref().unwrap();
    let requests = run.requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    for request in requests.iter() {
        assert_no_secret("a provider request", request);
    }
    assert!(requests[3].contains("[redacted]"), "provider error of the update row is redacted");
    assert!(requests[5].contains("token is [redacted] end"), "MCP result with a .env value is redacted");
    assert!(requests[6].contains("token is [redacted] end"), "MCP result with a literal MCP env value is redacted");
    {
        let events = run.frontend.events.lock().unwrap();
        let views: Vec<_> = events.iter().filter(|(name, _)| name == SECRET_VIEW_EVENT).collect();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].1, json!({"name": "OPENAI_KEY"}));
        for (name, payload) in events.iter() {
            assert_no_secret(&format!("event {name}"), &payload.to_string());
        }
    }
    let (transcript, _) = only_transcript(&run);
    assert_no_secret("the transcript", &transcript);

    let proposal = lock(&run.session).proposal.clone().unwrap();
    assert_eq!(proposal.operations.len(), 3);
    let dir = run.dir.path();
    let handed_keys = Mutex::new(Vec::new());
    let factory = |handed: &Settings| {
        let key = handed.models.iter().find(|m| m.id == "new-model").and_then(ModelConfig::resolved_api_key);
        handed_keys.lock().unwrap().push(key);
        let message = format!("Incorrect API key provided: {STAGED_VALUE}");
        let (provider, _) = MockProvider::new(vec![MockReply::Error(AiError::ApiStatus { status: 401, message })]);
        AiService::with_provider("new-model", Box::new(provider))
    };
    let values = BTreeMap::from([("NEW_KEY".to_string(), STAGED_VALUE.to_string())]);
    let placeholders = PlaceholderContext::with_apps("app", "apps");
    let row_request = RowTestRequest {
        row: 2,
        operations: &proposal.operations,
        plan: &proposal.plan,
        before: &settings,
        values: &values,
        ai_factory: &factory,
        placeholders: &placeholders,
        config_dir: dir,
        allow_new_destinations: true,
    };
    let RowTest::Tested(tested) = run_row_test(&row_request).await else {
        panic!("the add_model row is tested once its value is typed");
    };
    assert!(!tested.ok);
    let error = tested.error.clone().unwrap();
    assert!(error.contains("[redacted]"), "{error}");
    assert!(!error.contains(STAGED_VALUE), "{error}");
    assert_eq!(*handed_keys.lock().unwrap(), vec![Some(STAGED_VALUE.to_string())]);

    let mut config = ConfigService::load(dir).unwrap();
    let current_settings = config.settings().clone();
    let current = Current { settings: &current_settings, config_dir: dir };
    let planned = plan(&proposal.operations, &[true; 3], &current).unwrap();
    let log_dir = change_log_dir(dir);
    let (request, reply) = {
        let s = lock(&run.session);
        (s.last_request.clone(), s.last_reply.clone())
    };
    let mut ctx = ApplyContext {
        config: &mut config,
        config_dir: dir,
        log_dir: &log_dir,
        host: "host",
        request,
        reply,
        tests: BTreeMap::from([(2, tested)]),
        values,
    };
    let applied = apply(&planned, &current, &mut ctx);
    set_override("NEW_KEY", None);
    applied.unwrap();

    assert!(env_file(&run).contains("NEW_KEY="), "{}", env_file(&run));
    let held = format!(
        "{:?} {:?} {:?} {:?} {:?}",
        proposal.operations, proposal.plan.rows, proposal.tests, proposal.waits, proposal.new_destinations
    );
    assert_no_secret("the proposal held by the session", &held);
    let log = read_files(&log_dir);
    assert!(log.contains("NEW_KEY"), "{log}");
    assert_no_secret("the change-log entry", &log);
}

#[tokio::test]
async fn provider_error_echoing_a_key_is_redacted_in_the_transcript() {
    let error = AiError::ApiStatus { status: 401, message: "Incorrect API key provided: sk-env-111".into() };

    let run = run(Some("env-model"), vec![MockReply::Error(error)]).await;

    assert!(run.outcome.is_err());
    let (text, transcript) = only_transcript(&run);
    assert!(!text.contains("sk-env-111"), "{text}");
    assert_eq!(transcript["end"]["kind"], "error");
    assert!(transcript["end"]["error"].as_str().unwrap().contains("[redacted]"), "{text}");
}

#[tokio::test]
async fn successful_turn_writes_a_transcript_with_every_tool_call() {
    let run = run(Some("env-model"), scripted_turns()).await;

    run.outcome.as_ref().unwrap();
    let (_, transcript) = only_transcript(&run);
    assert_eq!(transcript["end"]["kind"], "reply");
    assert_eq!(transcript["end"]["reply"], "Proposed two changes.");
    assert_eq!(transcript["model_id"], "env-model");
    assert_eq!(transcript["user_text"], "add a model with a new key");
    let iterations = transcript["iterations"].as_array().unwrap();
    assert_eq!(iterations.len(), 5);
    let names: Vec<&str> = iterations
        .iter()
        .flat_map(|i| i["tool_calls"].as_array().unwrap())
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["settings_read", "settings_read", "settings_secret", "settings_propose_changes"]);
    let first = &iterations[0]["tool_calls"][0];
    assert_eq!(first["id"], "c1");
    assert_eq!(first["arguments"], json!({"what": "values"}));
    assert!(first["result"].as_str().is_some());
    assert!(first["error"].is_null());
    assert_eq!(iterations[4]["text"], "Proposed two changes.");
    assert_eq!(iterations[4]["tool_calls"], json!([]));
}

#[tokio::test]
async fn provider_error_on_the_first_request_writes_an_error_transcript() {
    let error = AiError::ApiStatus { status: 500, message: "boom".into() };

    let run = run(Some("env-model"), vec![MockReply::Error(error)]).await;

    assert!(run.outcome.is_err());
    let (_, transcript) = only_transcript(&run);
    assert_eq!(transcript["end"]["kind"], "error");
    assert!(transcript["end"]["error"].as_str().unwrap().contains("boom"));
    assert_eq!(transcript["iterations"], json!([]));
}

#[tokio::test]
async fn missing_model_writes_an_error_transcript_with_a_null_model() {
    let run = run(None, vec![MockReply::Text("never".into())]).await;

    let (_, transcript) = only_transcript(&run);
    assert_eq!(transcript["end"]["kind"], "error");
    assert!(transcript["model_id"].is_null());
}

#[tokio::test]
async fn cancelling_during_a_turn_writes_a_cancelled_transcript() {
    let (provider, requests) = MockProvider::new(scripted_turns());
    let ai = AiService::with_provider("env-model", Box::new(provider));

    let no_tools = Arc::new(ScriptedMcp::offering(&[], ""));
    let run = run_configured(fixture_settings(Some("env-model")), ai, requests, true, no_tools).await;

    assert!(run.outcome.is_err());
    let (_, transcript) = only_transcript(&run);
    assert_eq!(transcript["end"]["kind"], "cancelled");
    assert!(transcript["end"].get("error").is_none());
}

#[tokio::test]
async fn proposal_and_secret_view_reach_the_frontend() {
    let run = run(Some("env-model"), scripted_turns()).await;
    let events = run.frontend.events.lock().unwrap();
    let view = events.iter().find(|(name, _)| name == SECRET_VIEW_EVENT).unwrap();
    assert_eq!(view.1, json!({"name": "OPENAI_KEY"}));
    let proposal = events.iter().find(|(name, _)| name == PROPOSAL_EVENT).unwrap();
    assert_eq!(proposal.1["rows"].as_array().unwrap().len(), 2);
    assert_eq!(proposal.1["rows"][1]["test"]["ok"], false);
    assert_eq!(proposal.1["rows"][0]["stale"], false);
    assert_eq!(lock(&run.session).last_reply.as_deref(), Some("Proposed two changes."));
    for secret in SECRETS {
        assert!(!proposal.1.to_string().contains(secret), "{secret} leaked in the proposal");
    }
}

#[test]
fn a_set_env_value_row_carries_its_flags_and_never_a_value() {
    set_override("PHX_ENGINE_SET_VAR", Some("sk-engine-555".into()));
    let dir = TempDir::new().unwrap();
    let var = |name: &str, secret: bool| EnvVar {
        name: name.into(),
        description: String::new(),
        secret,
        renamed_from: vec![],
    };
    let mut settings = fixture_settings(Some("env-model"));
    settings.env_vars = vec![var("PHX_ENGINE_SET_VAR", true), var("PHX_ENGINE_UNSET_VAR", false)];
    let operations = vec![
        Operation::SetEnvValue { name: "PHX_ENGINE_SET_VAR".into() },
        Operation::SetEnvValue { name: "PHX_ENGINE_UNSET_VAR".into() },
        Operation::SetSetting { key: "theme".into(), value: json!("light") },
    ];
    let current = Current { settings: &settings, config_dir: dir.path() };
    let planned = plan(&operations, &[true, true, true], &current).unwrap();
    let proposal = Proposal { id: "p".into(), operations, plan: planned, tests: Default::default(), waits: Default::default(), new_destinations: Default::default() };

    let payload = proposal_payload(&proposal, &current);

    let rows = &payload["rows"];
    assert_eq!(rows[0]["env_value"], json!({"name": "PHX_ENGINE_SET_VAR", "secret": true, "set": true}));
    assert_eq!(rows[1]["env_value"], json!({"name": "PHX_ENGINE_UNSET_VAR", "secret": false, "set": false}));
    assert!(rows[2]["env_value"].is_null());
    assert_eq!(rows[0]["effects"][0]["target"], json!({"type": "secret", "name": "PHX_ENGINE_SET_VAR"}));
    assert!(!payload.to_string().contains("sk-engine-555"));
}

#[tokio::test]
async fn rejecting_writes_no_value() {
    let operations = json!([
        {"op": "add_env_var", "name": "NEW_KEY", "description": "new key", "secret": true},
        {"op": "set_env_value", "name": "NEW_KEY"},
    ]);
    let script = vec![
        call("c1", "settings_propose_changes", json!({"operations": operations})),
        MockReply::Text("Proposed.".to_string()),
    ];

    let run = run(Some("env-model"), script).await;

    run.outcome.as_ref().unwrap();
    let proposal = lock(&run.session).proposal.take().unwrap();
    assert_eq!(proposal.operations.len(), 2);
    assert!(!env_file(&run).contains("NEW_KEY"), "{}", env_file(&run));
    let settings = std::fs::read_to_string(run.dir.path().join("settings.json")).unwrap();
    assert!(!settings.contains("NEW_KEY"));
}

#[tokio::test]
async fn apply_stores_request_and_reply_in_the_log_entry() {
    let run = run(Some("env-model"), scripted_turns()).await;
    let (proposal, request, reply) = {
        let s = lock(&run.session);
        (s.proposal.clone().unwrap(), s.last_request.clone(), s.last_reply.clone())
    };
    let dir = run.dir.path();
    let mut config = ConfigService::load(dir).unwrap();
    let settings = config.settings().clone();
    let current = Current { settings: &settings, config_dir: dir };
    let planned = plan(&proposal.operations, &[true, true], &current).unwrap();
    let log_dir = change_log_dir(dir);
    let mut ctx = ApplyContext {
        config: &mut config,
        config_dir: dir,
        log_dir: &log_dir,
        host: "host",
        request,
        reply,
        tests: proposal.tests.clone(),
        values: Default::default(),
    };
    let id = apply(&planned, &current, &mut ctx).unwrap();
    let entry = load_all(&log_dir).into_iter().find(|e| e.id == id).unwrap();
    assert_eq!(entry.request.as_deref(), Some("add a model with a new key"));
    assert_eq!(entry.reply.as_deref(), Some("Proposed two changes."));
    assert_eq!(entry.operations.len(), 2);
}

#[tokio::test]
async fn six_model_field_rows_propose_without_errors() {
    let mut settings = fixture_settings(Some("env-model"));
    settings.models = (1..=6).map(|n| model(&format!("m{n}"), "M", "${OPENAI_KEY}")).collect();
    settings.models.push(model("env-model", "Env", "${OPENAI_KEY}"));
    let rows: Vec<Value> = (1..=6)
        .map(|n| json!({"op": "set_setting", "key": format!("models[m{n}].store"), "value": false}))
        .collect();
    let script = vec![
        call("c1", "settings_propose_changes", json!({"operations": rows})),
        MockReply::Text("Proposed.".to_string()),
    ];
    let (provider, requests) = MockProvider::new(script);
    let ai = AiService::with_provider("env-model", Box::new(provider));

    let run = run_with(settings, ai, requests).await;

    run.outcome.as_ref().unwrap();
    let events = run.frontend.events.lock().unwrap();
    let proposal = events.iter().find(|(name, _)| name == PROPOSAL_EVENT).unwrap();
    assert_eq!(proposal.1["rows"].as_array().unwrap().len(), 6);
    let requests = run.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(!requests[1].contains("\\\"errors\\\""), "{}", requests[1]);
}

#[tokio::test]
async fn no_chat_model_ends_the_turn_with_an_error_and_no_request() {
    let run = run(None, vec![MockReply::Text("never".into())]).await;
    assert!(run.outcome.is_err());
    assert!(run.requests.lock().unwrap().is_empty());
    let error = run.live.lock().await.snapshot.error.clone().unwrap();
    assert!(error.contains("chat model"), "{error}");
}

#[tokio::test]
async fn unavailable_chat_model_is_named_in_the_error() {
    let run = run(Some("ghost-model"), vec![MockReply::Text("never".into())]).await;
    assert!(run.outcome.is_err());
    assert!(run.requests.lock().unwrap().is_empty());
    let error = run.live.lock().await.snapshot.error.clone().unwrap();
    assert!(error.contains("ghost-model"), "{error}");
}

struct TwoModels {
    run: Run,
    chat_requests: Arc<Mutex<Vec<String>>>,
}

async fn run_on_agent_and_chat_models(
    agent_model: Option<&str>,
    agent_effort: Option<&str>,
    chat_effort: Option<&str>,
) -> TwoModels {
    let (agent_provider, agent_requests) = MockProvider::new(vec![MockReply::Text("done".into())]);
    let (chat_provider, chat_requests) = MockProvider::new(vec![MockReply::Text("done".into())]);
    let ai = AiService::with_providers(
        vec![("agent-model", Box::new(agent_provider)), ("chat-model", Box::new(chat_provider))],
        ModelCapabilities {
            reasoning: ReasoningMode::Effort { allowed: vec![Effort::Low, Effort::Medium, Effort::High] },
        },
    );
    let mut settings = fixture_settings(Some("chat-model"));
    settings.surfaces.settings_agent.generation.model_id = agent_model.map(str::to_string);
    settings.surfaces.settings_agent.generation.parameters.reasoning_effort = agent_effort.map(str::to_string);
    settings.surfaces.chat.generation.parameters.reasoning_effort = chat_effort.map(str::to_string);
    let run = run_with(settings, ai, agent_requests).await;
    TwoModels { run, chat_requests }
}

fn only_request(requests: &Arc<Mutex<Vec<String>>>) -> Value {
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    serde_json::from_str(&requests[0]).unwrap()
}

#[tokio::test]
async fn agent_model_serves_the_turn_when_set() {
    let two = run_on_agent_and_chat_models(Some("agent-model"), None, None).await;
    two.run.outcome.as_ref().unwrap();
    assert_eq!(only_request(&two.run.requests)["model"], "agent-model");
    assert!(two.chat_requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn empty_agent_model_falls_back_to_the_chat_model() {
    let two = run_on_agent_and_chat_models(None, None, None).await;
    two.run.outcome.as_ref().unwrap();
    assert_eq!(only_request(&two.chat_requests)["model"], "chat-model");
    assert!(two.run.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn agent_reasoning_effort_reaches_the_request_over_the_chat_one() {
    let two = run_on_agent_and_chat_models(Some("agent-model"), Some("low"), Some("high")).await;
    two.run.outcome.as_ref().unwrap();
    assert_eq!(only_request(&two.run.requests)["parameters"]["reasoning_effort"], "low");
}

#[tokio::test]
async fn unset_agent_reasoning_effort_falls_back_to_the_chat_one() {
    let two = run_on_agent_and_chat_models(Some("agent-model"), None, Some("high")).await;
    two.run.outcome.as_ref().unwrap();
    assert_eq!(only_request(&two.run.requests)["parameters"]["reasoning_effort"], "high");
}

#[tokio::test]
async fn reasoning_item_of_a_tool_round_goes_back_on_the_assistant_message() {
    let item = json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "enc"});
    let script = vec![
        MockReply::ReasoningThenToolCalls {
            reasoning_item: item.clone(),
            calls: vec![("c1".into(), "settings_read".into(), json!({}))],
        },
        MockReply::Text("done".into()),
    ];

    let run = run(Some("env-model"), script).await;

    run.outcome.as_ref().unwrap();
    let requests = run.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let second: Value = serde_json::from_str(&requests[1]).unwrap();
    let with_items: Vec<&Value> = second["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m.get("reasoning_items").is_some())
        .collect();
    assert_eq!(with_items.len(), 1);
    assert_eq!(with_items[0]["role"], "assistant");
    assert_eq!(with_items[0]["reasoning_items"], json!([item]));
    assert!(with_items[0]["tool_calls"].is_array());
}

#[tokio::test]
async fn reaching_the_tool_round_limit_ends_the_turn_with_the_last_tool_error() {
    let bad_row = json!({"operations": [{"op": "delete_model", "id": "ghost-id"}]});
    let script = (0..=MAX_TOOL_LOOP_ITERATIONS)
        .map(|n| call(&format!("c{n}"), "settings_propose_changes", bad_row.clone()))
        .collect();

    let run = run(Some("env-model"), script).await;

    assert!(run.outcome.is_err());
    assert_eq!(run.requests.lock().unwrap().len(), MAX_TOOL_LOOP_ITERATIONS + 1);
    let live = run.live.lock().await;
    let error = live.snapshot.error.clone().unwrap();
    assert!(error.contains("Stopped after 10 tool rounds"), "{error}");
    assert!(error.contains("model 'ghost-id' not found"), "{error}");
    let last = live.snapshot.tool_calls.last().unwrap();
    assert_eq!(last.tool_call_id, format!("c{MAX_TOOL_LOOP_ITERATIONS}"));
    assert!(matches!(last.status, ToolCallStatus::Failed));
    assert_eq!(
        last.error.as_deref(),
        Some("not run: the agent reached the limit of 10 tool rounds")
    );
}

#[tokio::test]
async fn reaching_the_tool_round_limit_writes_a_loop_limit_transcript() {
    let bad_row = json!({"operations": [{"op": "delete_model", "id": "ghost-id"}]});
    let script = (0..=MAX_TOOL_LOOP_ITERATIONS)
        .map(|n| call(&format!("c{n}"), "settings_propose_changes", bad_row.clone()))
        .collect();

    let run = run(Some("env-model"), script).await;

    let (_, transcript) = only_transcript(&run);
    assert_eq!(transcript["end"]["kind"], "loop_limit");
    assert!(transcript["end"]["error"].as_str().unwrap().contains("Stopped after 10 tool rounds"));
    let iterations = transcript["iterations"].as_array().unwrap();
    assert_eq!(iterations.len(), MAX_TOOL_LOOP_ITERATIONS + 1);
    let last = &iterations[MAX_TOOL_LOOP_ITERATIONS]["tool_calls"][0];
    assert_eq!(last["error"], "not run: the agent reached the limit of 10 tool rounds");
    assert!(last["result"].is_null());
}

const AGENT_TOOL_NAMES: [&str; 4] =
    ["settings_read", "settings_test", "settings_propose_changes", "settings_secret"];

fn payload_names(request: &str) -> Vec<String> {
    let request: Value = serde_json::from_str(request).unwrap();
    request["tool_payloads"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|payload| {
            payload["function"]["name"].as_str().or(payload["name"].as_str()).map(str::to_string)
        })
        .collect()
}

fn built_in_payload_count(request: &str) -> usize {
    let request: Value = serde_json::from_str(request).unwrap();
    request["tool_payloads"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|payload| **payload == json!({"type": "web_search_preview"}))
        .count()
}

async fn run_with_mcp_tools(
    enabled: &[&str],
    offered: &[&str],
    script: Vec<MockReply>,
) -> (Run, Arc<ScriptedMcp>) {
    let mut settings = fixture_settings(Some("env-model"));
    settings.surfaces.settings_agent.generation.enabled_tools =
        enabled.iter().map(|name| name.to_string()).collect();
    run_with_settings(settings, offered, script).await
}

async fn run_with_settings(
    settings: Settings,
    offered: &[&str],
    script: Vec<MockReply>,
) -> (Run, Arc<ScriptedMcp>) {
    let (provider, requests) = MockProvider::new(script);
    let ai = AiService::with_provider("env-model", Box::new(provider));
    let mcp = Arc::new(ScriptedMcp::offering(offered, "gpt-9 is the current model id"));
    let run = run_configured(settings, ai, requests, false, mcp.clone()).await;
    (run, mcp)
}

#[tokio::test]
async fn an_enabled_mcp_tool_is_sent_in_every_request_next_to_the_agent_tools() {
    let script = vec![
        call("c1", "web_search", json!({"query": "current model ids"})),
        MockReply::Text("done".into()),
    ];

    let (run, _) = run_with_mcp_tools(&["srv.web_search"], &["srv.web_search"], script).await;

    run.outcome.as_ref().unwrap();
    let requests = run.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        let names = payload_names(request);
        assert_eq!(names.len(), 5, "{names:?}");
        for agent_tool in AGENT_TOOL_NAMES {
            assert!(names.contains(&agent_tool.to_string()), "{agent_tool} missing in {names:?}");
        }
        assert!(names.contains(&"web_search".to_string()), "{names:?}");
    }
}

#[tokio::test]
async fn disabled_tool_is_not_offered_to_the_agent() {
    let mut settings = fixture_settings(Some("env-model"));
    settings.surfaces.settings_agent.generation.enabled_tools = vec!["srv.web_search".to_string()];
    settings.mcp_servers.get_mut("srv").unwrap().disabled_tools = vec!["web_search".to_string()];
    let script = vec![MockReply::Text("done".into())];

    let (run, _) = run_with_settings(settings, &["srv.web_search"], script).await;

    run.outcome.as_ref().unwrap();
    let requests = run.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let names = payload_names(&requests[0]);
    assert_eq!(names.len(), 4, "{names:?}");
    assert!(!names.contains(&"web_search".to_string()), "{names:?}");
}

#[tokio::test]
async fn a_call_by_raw_name_reaches_the_source_qualified_and_its_result_goes_back() {
    let script = vec![
        call("c1", "web_search", json!({"query": "current model ids"})),
        MockReply::Text("done".into()),
    ];

    let (run, mcp) = run_with_mcp_tools(&["srv.web_search"], &["srv.web_search"], script).await;

    run.outcome.as_ref().unwrap();
    assert_eq!(
        *mcp.calls.lock().unwrap(),
        vec![("srv.web_search".to_string(), json!({"query": "current model ids"}))]
    );
    let requests = run.requests.lock().unwrap();
    assert!(requests[1].contains("gpt-9 is the current model id"), "{}", requests[1]);
}

#[tokio::test]
async fn a_call_to_a_tool_that_is_not_enabled_is_an_error_result() {
    let script = vec![call("c1", "fetch_page", json!({})), MockReply::Text("done".into())];

    let (run, mcp) = run_with_mcp_tools(&["srv.web_search"], &["srv.web_search", "srv.fetch_page"], script).await;

    run.outcome.as_ref().unwrap();
    assert!(mcp.calls.lock().unwrap().is_empty());
    let live = run.live.lock().await;
    let failed = &live.snapshot.tool_calls[0];
    assert!(matches!(failed.status, ToolCallStatus::Failed));
    assert!(failed.error.as_deref().unwrap().contains("unknown tool 'fetch_page'"));
}

#[tokio::test]
async fn an_enabled_tool_named_like_an_agent_tool_is_not_sent() {
    let script = vec![MockReply::Text("done".into())];

    let (run, _) = run_with_mcp_tools(&["srv.settings_read"], &["srv.settings_read"], script).await;

    run.outcome.as_ref().unwrap();
    let names = payload_names(&run.requests.lock().unwrap()[0]);
    assert_eq!(names.iter().filter(|name| *name == "settings_read").count(), 1, "{names:?}");
    assert_eq!(names.len(), 4, "{names:?}");
}

#[tokio::test]
async fn enabled_tools_sharing_a_raw_name_are_all_dropped() {
    let script = vec![MockReply::Text("done".into())];
    let enabled = ["a.dup", "b.dup", "srv.web_search"];

    let (run, _) = run_with_mcp_tools(&enabled, &enabled, script).await;

    run.outcome.as_ref().unwrap();
    let names = payload_names(&run.requests.lock().unwrap()[0]);
    assert!(!names.contains(&"dup".to_string()), "{names:?}");
    assert!(names.contains(&"web_search".to_string()), "{names:?}");
}

fn settings_with_agent_search(agent_model: Option<&str>, enabled: &[&str]) -> Settings {
    let mut settings = fixture_settings(Some("env-model"));
    settings.surfaces.settings_agent.generation.model_id = agent_model.map(str::to_string);
    settings.surfaces.settings_agent.generation.enabled_tools =
        enabled.iter().map(|name| name.to_string()).collect();
    settings
}

fn assert_search_and_settings_tools(request: &str) {
    let names = payload_names(request);
    assert_eq!(built_in_payload_count(request), 1, "{request}");
    assert_eq!(names.len(), 4, "{names:?}");
    for agent_tool in AGENT_TOOL_NAMES {
        assert!(names.contains(&agent_tool.to_string()), "{agent_tool} missing in {names:?}");
    }
    assert!(!names.contains(&"web_search".to_string()), "{names:?}");
}

#[tokio::test]
async fn the_built_in_search_is_sent_for_an_openai_responses_agent_model_and_never_reaches_the_mcp_selection() {
    let settings = settings_with_agent_search(Some("env-model"), &["web_search"]);

    let (run, _) = run_with_settings(settings, &["web_search"], vec![MockReply::Text("done".into())]).await;

    run.outcome.as_ref().unwrap();
    assert_search_and_settings_tools(&run.requests.lock().unwrap()[0]);
}

#[tokio::test]
async fn the_built_in_search_is_sent_for_a_model_without_a_provider() {
    let mut settings = settings_with_agent_search(Some("env-model"), &["web_search"]);
    settings.models[0].provider = None;

    let (run, _) = run_with_settings(settings, &[], vec![MockReply::Text("done".into())]).await;

    run.outcome.as_ref().unwrap();
    assert_search_and_settings_tools(&run.requests.lock().unwrap()[0]);
}

#[tokio::test]
async fn the_built_in_search_is_not_sent_in_completions_mode() {
    let mut settings = settings_with_agent_search(Some("env-model"), &["web_search"]);
    settings.models[0].api_mode = Some(ApiMode::Completions);

    let (run, _) = run_with_settings(settings, &[], vec![MockReply::Text("done".into())]).await;

    run.outcome.as_ref().unwrap();
    let request = &run.requests.lock().unwrap()[0];
    assert_eq!(built_in_payload_count(request), 0, "{request}");
    assert_eq!(payload_names(request).len(), 4);
}

#[tokio::test]
async fn the_built_in_and_an_mcp_web_search_are_sent_together() {
    let settings = settings_with_agent_search(Some("env-model"), &["web_search", "srv.web_search"]);

    let (run, _) = run_with_settings(settings, &["srv.web_search"], vec![MockReply::Text("done".into())]).await;

    run.outcome.as_ref().unwrap();
    let request = &run.requests.lock().unwrap()[0];
    let names = payload_names(request);
    assert_eq!(built_in_payload_count(request), 1, "{request}");
    assert_eq!(names.len(), 5, "{names:?}");
    assert_eq!(names.iter().filter(|name| *name == "web_search").count(), 1, "{names:?}");
}

#[tokio::test]
async fn the_built_in_search_follows_the_chat_model_when_the_agent_has_none() {
    let settings = settings_with_agent_search(None, &["web_search"]);

    let (run, _) = run_with_settings(settings, &[], vec![MockReply::Text("done".into())]).await;

    run.outcome.as_ref().unwrap();
    assert_search_and_settings_tools(&run.requests.lock().unwrap()[0]);
}

#[tokio::test]
async fn a_built_in_search_is_recorded_in_the_snapshot_and_the_transcript_without_a_dispatch() {
    let settings = settings_with_agent_search(Some("env-model"), &["web_search"]);
    let script = vec![MockReply::WebSearchThen {
        id: "ws_1".into(),
        queries: vec!["latest openai model".into()],
        then: Box::new(MockReply::Text("done".into())),
    }];

    let (run, mcp) = run_with_settings(settings, &[], script).await;

    run.outcome.as_ref().unwrap();
    assert_eq!(run.requests.lock().unwrap().len(), 1);
    assert!(mcp.calls.lock().unwrap().is_empty());
    {
        let live = run.live.lock().await;
        assert_eq!(live.snapshot.tool_calls.len(), 1);
        let search = &live.snapshot.tool_calls[0];
        assert_eq!(search.tool_name, "web_search");
        assert!(matches!(search.status, ToolCallStatus::Completed));
        assert_eq!(search.result.as_deref(), Some("latest openai model"));
    }
    let (_, transcript) = only_transcript(&run);
    let iterations = transcript["iterations"].as_array().unwrap();
    assert_eq!(iterations.len(), 1);
    let calls = iterations[0]["tool_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["id"], "ws_1");
    assert_eq!(calls[0]["name"], "web_search");
    assert_eq!(calls[0]["result"], "latest openai model");
    assert!(calls[0]["error"].is_null());
    assert_eq!(calls[0]["millis"], 0);
}

#[tokio::test]
async fn a_built_in_search_item_is_replayed_and_recorded_before_the_dispatched_calls() {
    let settings = settings_with_agent_search(Some("env-model"), &["web_search"]);
    let script = vec![
        MockReply::WebSearchThen {
            id: "ws_1".into(),
            queries: vec!["latest openai model".into()],
            then: Box::new(call("c1", "settings_read", json!({}))),
        },
        MockReply::Text("done".into()),
    ];

    let (run, _) = run_with_settings(settings, &[], script).await;

    run.outcome.as_ref().unwrap();
    let requests = run.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let second: Value = serde_json::from_str(&requests[1]).unwrap();
    let assistant = second["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["tool_calls"].is_array())
        .unwrap();
    let items = assistant["reasoning_items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], "web_search_call");
    assert_eq!(items[0]["id"], "ws_1");
    let (_, transcript) = only_transcript(&run);
    let calls = transcript["iterations"][0]["tool_calls"].as_array().unwrap();
    let names: Vec<_> = calls.iter().map(|call| call["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["web_search", "settings_read"]);
}
