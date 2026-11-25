use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::TempDir;

use super::operations::Operation;
use super::engine::proposal_payload;
use super::plan::Current;
use super::dispatch::{Context, Dispatcher, Proposal, ProposalSink, SecretViewSink};
use super::mcp::ScriptedMcp;
use crate::models::settings::{EnvVar, McpServerConfig, ModelConfig, ModelType, Provider, Settings};
use crate::services::ai::mock::{MockProvider, MockReply};
use crate::services::ai::{AiError, AiService};
use crate::services::execution::types::PendingToolCall;
use crate::services::placeholder_registry::PlaceholderContext;

const BASE_URL_A: &str = "https://alpha.example/api";
const BASE_URL_B: &str = "https://beta.example/api";

const SECRETS: [&str; 4] = ["sk-env-111", "tok-222", "sk-literal-333", "mcp-literal-555"];

fn model(id: &str, api_key: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        model: "m".to_string(),
        display_name: format!("Model {id}"),
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

fn fixture_settings() -> Settings {
    let mut settings = Settings::default();
    settings.models = vec![
        ModelConfig { base_url: Some(BASE_URL_A.into()), ..model("env-model", "${OPENAI_KEY}") },
        model("literal-model", "sk-literal-333"),
    ];
    settings.mcp_servers = HashMap::from([
        (
            "tools".to_string(),
            McpServerConfig {
                command: "run-tools".into(),
                args: vec![],
                env: HashMap::from([("TOKEN".to_string(), "${MCP_TOKEN}".to_string())]),
                timeout_secs: 30,
                tool_timeouts: HashMap::new(),
                ..Default::default()
            },
        ),
        (
            "srv".to_string(),
            McpServerConfig {
                command: "run-srv".into(),
                args: vec![],
                env: HashMap::from([("API_KEY".to_string(), "mcp-literal-555".to_string())]),
                timeout_secs: 30,
                tool_timeouts: HashMap::new(),
                ..Default::default()
            },
        ),
    ]);
    settings.surfaces.chat.generation.model_id = Some("env-model".into());
    settings.env_vars = vec![
        EnvVar {
            name: "OPENAI_KEY".into(),
            description: "OpenAI key".into(),
            secret: true,
            renamed_from: vec![],
        },
        EnvVar {
            name: "TOOLS_DIR".into(),
            description: "Tools path".into(),
            secret: false,
            renamed_from: vec![],
        },
    ];
    settings
}

#[derive(Default)]
struct Sink {
    proposals: std::sync::Mutex<Vec<Proposal>>,
    views: std::sync::Mutex<Vec<String>>,
}

impl ProposalSink for Sink {
    fn proposed(&self, proposal: &Proposal) {
        self.proposals.lock().unwrap().push(proposal.clone());
    }
}

impl SecretViewSink for Sink {
    fn shown(&self, name: &str) {
        self.views.lock().unwrap().push(name.to_string());
    }
}

fn fixture() -> (TempDir, Dispatcher, Arc<Sink>) {
    let (dir, dispatcher, sink, _) = fixture_counting_factory();
    (dir, dispatcher, sink)
}

fn fixture_counting_factory() -> (TempDir, Dispatcher, Arc<Sink>, Arc<AtomicUsize>) {
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let counted = factory_calls.clone();
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "OPENAI_KEY=sk-env-111\nMCP_TOKEN=tok-222\nTOOLS_DIR=/opt/promptheus-tools\n",
    )
    .unwrap();
    let skill_dir = dir.path().join("skills/old-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: old-skill\ndescription: Old\n---\n\nOld body.\n",
    )
    .unwrap();
    let settings = fixture_settings();
    std::fs::write(dir.path().join("settings.json"), serde_json::to_string_pretty(&settings).unwrap()).unwrap();
    let sink = Arc::new(Sink::default());
    let ctx = Context {
        settings: Box::new(move || settings.clone()),
        config_dir: dir.path().to_path_buf(),
        skills_dir: dir.path().join("skills"),
        ai_factory: Box::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            let error = AiError::ApiStatus {
                status: 401,
                message: "Incorrect API key provided: sk-env-111".into(),
            };
            let (provider, _) = MockProvider::new(vec![MockReply::Error(error)]);
            AiService::with_provider("env-model", Box::new(provider))
        }),
        placeholders: PlaceholderContext::with_apps("app", "apps"),
        proposals: sink.clone(),
        secret_views: sink.clone(),
        mcp: Arc::new(ScriptedMcp::offering(&["srv.web_search"], "found it with key mcp-literal-555")),
        mcp_tools: BTreeMap::from([("web_search".to_string(), "srv.web_search".to_string())]),
    };
    (dir, Dispatcher::new(ctx), sink, factory_calls)
}

async fn call(dispatcher: &Dispatcher, tool: &str, arguments: Value) -> (String, bool) {
    let result = dispatcher
        .dispatch(&PendingToolCall {
            tool_call_id: "call-1".into(),
            tool_name: tool.into(),
            arguments,
        })
        .await;
    (result.result_text, result.is_error)
}

fn assert_clean(text: &str) {
    for secret in SECRETS.iter().chain(&["sk-new-444"]) {
        assert!(!text.contains(secret), "{secret} leaked in {text}");
    }
}

fn update_model_row() -> Value {
    let model = ModelConfig { base_url: Some(BASE_URL_A.into()), ..model("env-model", "${OPENAI_KEY}") };
    json!({"op": "update_model", "id": "env-model", "model": model})
}

#[tokio::test]
async fn every_read_variant_is_free_of_secrets() {
    let (_dir, dispatcher, _) = fixture();
    let reads = [
        json!({"what": "values"}),
        json!({"what": "models"}),
        json!({"what": "skills"}),
        json!({"what": "skill", "name": "old-skill"}),
        json!({"what": "prompt", "name": "system"}),
        json!({"what": "prompt", "name": "stt_keyterms"}),
        json!({"what": "missing_env"}),
        json!({"what": "env_vars"}),
    ];
    for arguments in reads {
        let (text, is_error) = call(&dispatcher, "settings_read", arguments.clone()).await;
        assert!(!is_error, "{arguments}: {text}");
        assert_clean(&text);
    }
}

#[tokio::test]
async fn an_mcp_result_echoing_a_literal_mcp_env_value_is_redacted() {
    let (_dir, dispatcher, _) = fixture();

    let (text, is_error) = call(&dispatcher, "web_search", json!({"query": "model ids"})).await;

    assert!(!is_error, "{text}");
    assert_eq!(text, "found it with key [redacted]");
}

#[tokio::test]
async fn a_tool_that_is_not_enabled_is_an_unknown_tool() {
    let (_dir, dispatcher, _) = fixture();

    let (text, is_error) = call(&dispatcher, "fetch_page", json!({})).await;

    assert!(is_error);
    assert!(text.contains("unknown tool 'fetch_page'"), "{text}");
}

#[tokio::test]
async fn values_read_masks_a_literal_mcp_env_value_and_keeps_a_reference() {
    let (_dir, dispatcher, _) = fixture();

    let (text, is_error) = call(&dispatcher, "settings_read", json!({"what": "values"})).await;

    assert!(!is_error, "{text}");
    let values: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(values["mcp_servers"]["srv"]["env"]["API_KEY"], "[redacted]");
    assert_eq!(values["mcp_servers"]["tools"]["env"]["TOKEN"], "${MCP_TOKEN}");
}

#[tokio::test]
async fn env_vars_read_lists_state_usages_and_the_value_of_plain_variables_only() {
    let (_dir, dispatcher, _) = fixture();
    let (text, is_error) = call(&dispatcher, "settings_read", json!({"what": "env_vars"})).await;
    assert!(!is_error, "{text}");
    assert_clean(&text);
    let rows: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        for field in ["name", "description", "secret", "set", "used_in"] {
            assert!(row.get(field).is_some(), "{field} missing in {row}");
        }
        assert!(row["set"].is_boolean());
    }
    assert_eq!(rows[0]["name"], "OPENAI_KEY");
    assert_eq!(rows[0]["secret"], true);
    assert!(rows[0]["used_in"][0]["label"].as_str().unwrap().ends_with("API key"));
    assert_eq!(rows[1]["name"], "TOOLS_DIR");
    assert_eq!(rows[1]["secret"], false);
    assert_eq!(rows[1]["used_in"], json!([]));
    assert_eq!(rows[1]["value"], "/opt/promptheus-tools");
    assert!(rows[0].get("value").is_none(), "{}", rows[0]);
}

#[tokio::test]
async fn values_read_keeps_env_references_and_hides_literal_keys() {
    let (_dir, dispatcher, _) = fixture();
    let (text, _) = call(&dispatcher, "settings_read", json!({"what": "values"})).await;
    assert!(text.contains("${OPENAI_KEY}"));
    assert!(text.contains("[redacted]"));
}

#[tokio::test]
async fn test_tool_redacts_provider_error() {
    let (_dir, dispatcher, _) = fixture();
    let (text, is_error) =
        call(&dispatcher, "settings_test", json!({"operations": [update_model_row()]})).await;
    assert!(!is_error);
    assert!(text.contains("[redacted]"), "{text}");
    assert_clean(&text);
}

#[tokio::test]
async fn propose_tool_redacts_provider_error_and_hands_proposal_to_sink() {
    let (_dir, dispatcher, sink) = fixture();
    let (text, is_error) = call(
        &dispatcher,
        "settings_propose_changes",
        json!({"operations": [update_model_row()]}),
    )
    .await;
    assert!(!is_error);
    assert!(text.contains("[redacted]"), "{text}");
    assert_clean(&text);
    let proposals = sink.proposals.lock().unwrap();
    assert_eq!(proposals.len(), 1);
    let proposal = &proposals[0];
    assert!(text.contains(&proposal.id));
    assert!(!proposal.tests[&0].ok);
}

#[tokio::test]
async fn propose_with_row_errors_shows_no_card() {
    let (_dir, dispatcher, sink) = fixture();
    let bad = json!({"op": "delete_model", "id": "missing"});
    let (text, is_error) = call(
        &dispatcher,
        "settings_propose_changes",
        json!({"operations": [bad]}),
    )
    .await;
    assert!(!is_error);
    assert!(text.contains("\"errors\""), "{text}");
    assert!(sink.proposals.lock().unwrap().is_empty());
}

#[tokio::test]
async fn propose_reports_every_bad_row_of_the_arguments_and_shows_no_card() {
    let (_dir, dispatcher, sink) = fixture();
    let valid = json!({"op": "set_setting", "key": "theme", "value": "light"});
    let wrong_op = json!({"op": "update_model next"});
    let no_id = json!({"op": "update_model", "model": model("env-model", "${OPENAI_KEY}")});
    let (text, is_error) = call(
        &dispatcher,
        "settings_propose_changes",
        json!({"operations": [valid, wrong_op, no_id]}),
    )
    .await;
    assert!(!is_error, "{text}");
    let result: Value = serde_json::from_str(&text).unwrap();
    let errors = result["errors"].as_array().unwrap();
    assert_eq!(errors.iter().map(|e| e["row"].clone()).collect::<Vec<_>>(), [json!(2), json!(3)]);
    let unknown = errors[0]["message"].as_str().unwrap();
    assert!(unknown.contains("update_model next"), "{unknown}");
    for name in Operation::NAMES {
        assert!(unknown.contains(name), "{name} missing in {unknown}");
    }
    let missing = errors[1]["message"].as_str().unwrap();
    assert!(missing.contains("id") && missing.contains("model"), "{missing}");
    assert!(result["note"].as_str().unwrap().contains("Nothing was proposed"), "{text}");
    assert!(sink.proposals.lock().unwrap().is_empty());
}

#[tokio::test]
async fn secret_tool_shows_a_set_variable_at_once_and_returns_no_value() {
    let (_dir, dispatcher, sink) = fixture();
    let (text, is_error) = call(&dispatcher, "settings_secret", json!({"name": "OPENAI_KEY"})).await;
    assert!(!is_error, "{text}");
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!({"name": "OPENAI_KEY", "outcome": "shown"}));
    assert_clean(&text);
    assert_eq!(*sink.views.lock().unwrap(), ["OPENAI_KEY"]);
}

#[tokio::test]
async fn secret_tool_reports_not_set_and_shows_nothing() {
    let (_dir, dispatcher, sink) = fixture();
    let (text, is_error) = call(&dispatcher, "settings_secret", json!({"name": "BRAND_NEW_UNSET_VAR_097"})).await;
    assert!(!is_error, "{text}");
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"name": "BRAND_NEW_UNSET_VAR_097", "outcome": "not_set"})
    );
    assert!(sink.views.lock().unwrap().is_empty());
}

#[tokio::test]
async fn secret_tool_rejects_an_invalid_name_and_unknown_fields() {
    let (_dir, dispatcher, sink) = fixture();
    let (invalid, is_error) = call(&dispatcher, "settings_secret", json!({"name": "1 bad"})).await;
    assert!(is_error, "{invalid}");
    let (extra, is_error) =
        call(&dispatcher, "settings_secret", json!({"name": "OPENAI_KEY", "purpose": "p"})).await;
    assert!(is_error);
    assert!(extra.starts_with("invalid arguments"), "{extra}");
    assert!(sink.views.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_tool_and_bad_arguments_are_errors() {
    let (_dir, dispatcher, _) = fixture();
    let (_, unknown) = call(&dispatcher, "settings_delete_everything", json!({})).await;
    assert!(unknown);
    let (_, bad) = call(&dispatcher, "settings_read", json!({"what": "nonsense"})).await;
    assert!(bad);
    let (_, missing_name) = call(&dispatcher, "settings_read", json!({"what": "skill"})).await;
    assert!(missing_name);
}

#[test]
fn tool_names_are_valid_function_names() {
    for tool in super::tools::definitions() {
        assert!(tool.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        assert!(tool.name.starts_with("settings_"));
    }
}

fn apply_delete_of_literal_key_model(dir: &std::path::Path) -> String {
    use super::apply::{apply, Context as ApplyContext};
    use super::plan::{plan, Current};
    use crate::services::change_log::change_log_dir;
    use crate::services::config::ConfigService;

    let mut settings = fixture_settings();
    settings.models.push(model("deleted-model", "sk-deleted-999"));
    std::fs::write(dir.join("settings.json"), serde_json::to_string_pretty(&settings).unwrap()).unwrap();
    let mut config = ConfigService::load(dir).unwrap();
    let operations = [Operation::DeleteModel { id: "deleted-model".into() }];
    let current_settings = config.settings().clone();
    let current = Current { settings: &current_settings, config_dir: dir };
    let planned = plan(&operations, &[true], &current).unwrap_or_else(|errors| panic!("{errors:?}"));
    let log_dir = change_log_dir(dir);
    let mut ctx = ApplyContext {
        config: &mut config,
        config_dir: dir,
        log_dir: &log_dir,
        host: "test-host",
        request: Some("delete the model".into()),
        reply: None,
        tests: Default::default(),
        values: Default::default(),
    };
    apply(&planned, &current, &mut ctx).unwrap()
}

#[tokio::test]
async fn change_log_reads_never_return_the_literal_key_of_a_deleted_model() {
    let (dir, dispatcher, _) = fixture();
    let entry_id = apply_delete_of_literal_key_model(dir.path());
    let reads = [
        json!({"what": "change_log"}),
        json!({"what": "change_log", "key": "models[deleted-model]"}),
        json!({"what": "change_log", "name": entry_id}),
    ];
    for arguments in reads {
        let (text, is_error) = call(&dispatcher, "settings_read", arguments.clone()).await;
        assert!(!is_error, "{arguments}: {text}");
        assert!(!text.contains("sk-deleted-999"), "leaked in {text}");
        assert!(text.contains(&entry_id), "{arguments}: {text}");
        assert!(text.contains("[redacted]"), "{arguments}: {text}");
    }
}

#[tokio::test]
async fn change_log_read_with_strict_nulls_and_an_unknown_name() {
    let (_dir, dispatcher, _) = fixture();
    let all_null = json!({
        "what": "change_log", "name": null, "since": null, "until": null,
        "key": null, "op": null, "text": null, "host": null, "limit": null
    });
    let (text, is_error) = call(&dispatcher, "settings_read", all_null).await;
    assert!(!is_error, "{text}");
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!({"entries": [], "more": 0}));
    let (text, is_error) = call(&dispatcher, "settings_read", json!({"what": "change_log", "name": "nope"})).await;
    assert!(is_error);
    assert_eq!(text, "change log entry 'nope' not found");
}

fn add_model_row(id: &str, base_url: &str, api_key: &str) -> Value {
    let model = ModelConfig { base_url: Some(base_url.into()), ..model(id, api_key) };
    json!({"op": "add_model", "model": model})
}

#[tokio::test]
async fn a_row_using_an_unvalued_staged_variable_waits_and_is_not_tested() {
    const NAME: &str = "PHX_DISPATCH_NEW_KEY";
    let (dir, dispatcher, sink, factory_calls) = fixture_counting_factory();
    let operations = json!([
        {"op": "add_env_var", "name": NAME, "description": "new key", "secret": true},
        {"op": "set_env_value", "name": NAME},
        add_model_row("123e4567-e89b-12d3-a456-426614174003", BASE_URL_A, &format!("${{{NAME}}}")),
        json!({"op": "add_model", "model": model("123e4567-e89b-12d3-a456-426614174004", &format!("${{{NAME}}}"))}),
    ]);

    let (text, is_error) =
        call(&dispatcher, "settings_propose_changes", json!({"operations": operations})).await;

    assert!(!is_error, "{text}");
    let result: Value = serde_json::from_str(&text).unwrap();
    let rows = &result["rows"];
    assert_eq!(rows[2]["waits_for"], json!([NAME]));
    assert!(rows[2]["test"].is_null());
    let note = rows[2]["new_destination"].as_str().unwrap();
    assert!(note.contains(NAME) && note.contains(BASE_URL_A), "{note}");
    assert_eq!(rows[3]["waits_for"], json!([NAME]));
    assert!(rows[3]["test"].is_null());
    assert!(rows[3]["new_destination"].is_null());
    assert_eq!(rows[0]["waits_for"], json!([]));
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    let proposal = sink.proposals.lock().unwrap()[0].clone();
    let settings = fixture_settings();
    let payload = proposal_payload(&proposal, &Current { settings: &settings, config_dir: dir.path() });
    assert_eq!(payload["rows"][2]["waits_for"], json!([NAME]));
    assert_eq!(payload["rows"][1]["waits_for"], json!([]));
    assert_eq!(payload["rows"][2]["new_destination"], note);
    assert_eq!(payload["rows"][3]["waits_for"], json!([NAME]));
    assert!(payload["rows"][3]["new_destination"].is_null());
}

#[tokio::test]
async fn a_key_going_to_a_new_base_url_is_not_tested_automatically() {
    let (dir, dispatcher, sink, factory_calls) = fixture_counting_factory();
    let operations = json!({"operations": [add_model_row("123e4567-e89b-12d3-a456-426614174005", BASE_URL_B, "${OPENAI_KEY}")]});

    let (text, is_error) = call(&dispatcher, "settings_test", operations.clone()).await;

    assert!(!is_error, "{text}");
    assert_clean(&text);
    let row = &serde_json::from_str::<Value>(&text).unwrap()["rows"][0];
    let note = row["new_destination"].as_str().unwrap();
    assert!(note.contains("OPENAI_KEY") && note.contains(BASE_URL_B), "{note}");
    assert!(row["test"].is_null());
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);

    call(&dispatcher, "settings_propose_changes", operations).await;
    let proposal = sink.proposals.lock().unwrap()[0].clone();
    let settings = fixture_settings();
    let payload = proposal_payload(&proposal, &Current { settings: &settings, config_dir: dir.path() });
    assert_eq!(payload["rows"][0]["new_destination"], note);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_key_going_to_its_known_base_url_is_tested() {
    let (_dir, dispatcher, _, factory_calls) = fixture_counting_factory();
    let operations = json!({"operations": [add_model_row("123e4567-e89b-12d3-a456-426614174006", BASE_URL_A, "${OPENAI_KEY}")]});

    let (text, is_error) = call(&dispatcher, "settings_test", operations).await;

    assert!(!is_error, "{text}");
    let row = &serde_json::from_str::<Value>(&text).unwrap()["rows"][0];
    assert_eq!(row["test"]["ok"], false);
    assert!(row["new_destination"].is_null());
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
}
