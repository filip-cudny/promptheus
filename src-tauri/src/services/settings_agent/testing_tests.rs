use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use tempfile::TempDir;

use super::operations::Operation;
use super::plan::Plan;
use super::testing::{run_row_test, test_model_config, RowTest, RowTestRequest};
use crate::models::settings::{McpServerConfig, ModelConfig, ModelType, Provider, Settings};
use crate::models::skill::SkillFrontmatter;
use crate::services::ai::mock::{MockProvider, MockReply};
use crate::services::ai::{AiError, AiService};
use crate::services::change_log::TestResult;
use crate::services::env_resolve::override_for;
use crate::services::placeholder_registry::PlaceholderContext;

fn model(id: &str, model_type: ModelType) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        model: "m".to_string(),
        display_name: format!("Model {id}"),
        model_type,
        provider: Some(Provider::default()),
        group: None,
        api_key: None,
        base_url: None,
        parameters: None,
        context_window_size: None,
        api_mode: None,
        capabilities: None,
        store: true,
    }
}

fn plan_with(settings: Settings) -> Plan {
    Plan { settings, rows: vec![] }
}

fn ctx() -> PlaceholderContext {
    PlaceholderContext::with_apps("app", "apps")
}

struct Case {
    operations: Vec<Operation>,
    row: usize,
    plan: Plan,
    before: Settings,
    values: BTreeMap<String, String>,
    allow_new_destinations: bool,
}

impl Case {
    fn new(operations: Vec<Operation>, plan_settings: Settings) -> Self {
        Self {
            row: operations.len() - 1,
            operations,
            plan: plan_with(plan_settings),
            before: Settings::default(),
            values: BTreeMap::new(),
            allow_new_destinations: true,
        }
    }

    fn values(mut self, values: &[(&str, &str)]) -> Self {
        self.values = values.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
        self
    }

    fn before(mut self, before: Settings) -> Self {
        self.before = before;
        self
    }

    fn on_click_only(mut self) -> Self {
        self.allow_new_destinations = false;
        self
    }

    async fn run(&self, ai_factory: &(dyn Fn(&Settings) -> AiService + Sync)) -> RowTest {
        let dir = TempDir::new().unwrap();
        run_row_test(&RowTestRequest {
            row: self.row,
            operations: &self.operations,
            plan: &self.plan,
            before: &self.before,
            values: &self.values,
            ai_factory,
            placeholders: &ctx(),
            config_dir: dir.path(),
            allow_new_destinations: self.allow_new_destinations,
        })
        .await
    }
}

async fn test_row(operation: &Operation, plan: &Plan, ai: &AiService) -> Option<TestResult> {
    let case = Case {
        operations: vec![operation.clone()],
        row: 0,
        plan: plan.clone(),
        before: Settings::default(),
        values: BTreeMap::new(),
        allow_new_destinations: true,
    };
    match case.run(&|_| ai.clone()).await {
        RowTest::Tested(result) => Some(result),
        RowTest::NotTestable => None,
        other => panic!("{other:?}"),
    }
}

fn with_key(id: &str, api_key: &str, base_url: Option<&str>) -> ModelConfig {
    ModelConfig {
        api_key: Some(api_key.to_string()),
        base_url: base_url.map(str::to_string),
        ..model(id, ModelType::Text)
    }
}

fn settings_of(models: Vec<ModelConfig>) -> Settings {
    Settings { models, ..Settings::default() }
}

fn set_value(name: &str) -> Operation {
    Operation::SetEnvValue { name: name.into() }
}

fn echoing_ai(model_id: &str, reply: &str) -> AiService {
    let (provider, _) = MockProvider::new(vec![MockReply::Text(reply.into())]);
    AiService::with_provider(model_id, Box::new(provider))
}

fn skill_operation(model: Option<&str>) -> Operation {
    Operation::CreateSkill {
        slug: "s".into(),
        frontmatter: SkillFrontmatter {
            name: "s".into(),
            display_name: None,
            description: None,
            model: model.map(str::to_string),
            parameters: None,
            offered_in: None,
        },
        body: "Do it.".into(),
        sample_input: "sample input text".into(),
    }
}

#[tokio::test]
async fn text_model_ok() {
    let (provider, _) = MockProvider::new(vec![MockReply::Text("OK".into())]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let operation = Operation::AddModel { model: model("x", ModelType::Text) };
    let result = test_row(&operation, &plan_with(Settings::default()), &ai)
        .await
        .unwrap();
    assert!(result.ok);
    assert!(result.millis.is_some());
    assert_eq!(result.output.as_deref(), Some("OK"));
}

#[tokio::test]
async fn text_model_error_text_is_unchanged() {
    let error = AiError::ApiStatus { status: 400, message: "model 'x' does not exist".into() };
    let expected = error.to_string();
    let (provider, _) = MockProvider::new(vec![MockReply::Error(error)]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let operation = Operation::AddModel { model: model("x", ModelType::Text) };
    let result = test_row(&operation, &plan_with(Settings::default()), &ai)
        .await
        .unwrap();
    assert!(!result.ok);
    assert_eq!(result.error, Some(expected));
}

#[tokio::test]
async fn skill_test_sends_one_request_without_tools() {
    let (provider, requests) = MockProvider::new(vec![MockReply::Text("done".into())]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let result = test_row(&skill_operation(Some("x")), &plan_with(Settings::default()), &ai)
        .await
        .unwrap();
    assert!(result.ok);
    assert_eq!(result.output.as_deref(), Some("done"));
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let recorded: serde_json::Value = serde_json::from_str(&requests[0]).unwrap();
    assert_eq!(recorded["tool_payloads"], serde_json::json!([]));
    assert!(recorded["messages"].to_string().contains("sample input text"));
}

#[tokio::test]
async fn skill_without_model_uses_quick_actions_model() {
    let (provider, requests) = MockProvider::new(vec![MockReply::Text("done".into())]);
    let ai = AiService::with_provider("quick", Box::new(provider));
    let mut settings = Settings::default();
    settings.surfaces.quick_actions.generation.model_id = Some("quick".into());
    let result = test_row(&skill_operation(None), &plan_with(settings), &ai)
        .await
        .unwrap();
    assert!(result.ok);
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn mcp_missing_command_fails_with_start_error() {
    let config = McpServerConfig {
        command: "/nonexistent/promptheus-test-binary".into(),
        args: vec![],
        env: HashMap::new(),
        timeout_secs: 5,
        tool_timeouts: HashMap::new(),
        ..Default::default()
    };
    let operation = Operation::AddMcpServer { name: "bad".into(), config: config.clone().into() };
    let mut settings = Settings::default();
    settings.mcp_servers.insert("bad".into(), config);
    let (provider, _) = MockProvider::new(vec![]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let result = test_row(&operation, &plan_with(settings), &ai).await.unwrap();
    assert!(!result.ok);
    assert!(result.error.unwrap().contains("failed to start MCP server 'bad'"));
}

#[tokio::test]
async fn untestable_row_returns_none() {
    let (provider, _) = MockProvider::new(vec![]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let operation = Operation::DeleteModel { id: "x".into() };
    assert!(test_row(&operation, &plan_with(Settings::default()), &ai).await.is_none());
}

#[tokio::test]
async fn staged_value_reaches_the_factory_settings_and_sets_no_override() {
    const KEY: &str = "PHX_TEST_STAGED_KEY";
    let staged = with_key("staged", &format!("${{{KEY}}}"), None);
    let operations = vec![
        Operation::AddEnvVar { name: KEY.into(), description: String::new(), secret: true },
        set_value(KEY),
        Operation::AddModel { model: staged.clone() },
    ];
    let case = Case::new(operations, settings_of(vec![staged])).values(&[(KEY, "sk-staged-555")]);
    let seen = Mutex::new(Vec::new());
    let ai = echoing_ai("staged", "OK");

    let result = case
        .run(&|settings| {
            seen.lock().unwrap().push(settings.models[0].resolved_api_key());
            ai.clone()
        })
        .await;

    assert!(matches!(result, RowTest::Tested(ref tested) if tested.ok), "{result:?}");
    assert_eq!(*seen.lock().unwrap(), vec![Some("sk-staged-555".to_string())]);
    assert_eq!(override_for(KEY), None);
    assert_eq!(case.plan.settings.models[0].api_key.as_deref(), Some("${PHX_TEST_STAGED_KEY}"));
}

#[tokio::test]
async fn provider_error_echoing_a_staged_value_is_redacted() {
    const KEY: &str = "PHX_TEST_REDACT_KEY";
    let staged = with_key("echo", &format!("${{{KEY}}}"), None);
    let operations = vec![set_value(KEY), Operation::AddModel { model: staged.clone() }];
    let error = AiError::ApiStatus { status: 401, message: "Incorrect API key provided: sk-staged-666".into() };
    let (provider, _) = MockProvider::new(vec![MockReply::Error(error)]);
    let ai = AiService::with_provider("echo", Box::new(provider));

    let result = Case::new(operations, settings_of(vec![staged]))
        .values(&[(KEY, "sk-staged-666")])
        .run(&|_| ai.clone())
        .await;

    let RowTest::Tested(tested) = result else { panic!("{result:?}") };
    let message = tested.error.unwrap();
    assert!(message.contains("[redacted]"), "{message}");
    assert!(!message.contains("sk-staged-666"), "{message}");
}

#[tokio::test]
async fn row_waits_for_every_unvalued_name_once_sorted_and_runs_nothing() {
    let (a, b) = ("PHX_TEST_WAIT_A", "PHX_TEST_WAIT_B");
    let config = McpServerConfig {
        command: "/nonexistent/promptheus-test-binary".into(),
        args: vec![],
        env: HashMap::from([("X".into(), format!("${{{b}}}")), ("Y".into(), format!("${{{a}}}${{{a}}}"))]),
        timeout_secs: 5,
        tool_timeouts: HashMap::new(),
        ..Default::default()
    };
    let mut settings = Settings::default();
    settings.mcp_servers.insert("waiting".into(), config.clone());
    let operations = vec![
        set_value(b),
        set_value(a),
        Operation::AddMcpServer { name: "waiting".into(), config: config.into() },
    ];
    let calls = AtomicUsize::new(0);
    let ai = echoing_ai("x", "OK");
    let factory = |_: &Settings| {
        calls.fetch_add(1, Ordering::SeqCst);
        ai.clone()
    };

    let nothing = Case::new(operations.clone(), settings.clone()).on_click_only().run(&factory).await;
    let empty_value = Case::new(operations.clone(), settings.clone()).values(&[(a, ""), (b, "v")]).run(&factory).await;
    let one_left = Case::new(operations, settings).values(&[(a, "v")]).run(&factory).await;

    let waits = |result: RowTest| match result {
        RowTest::Waits { names, .. } => names,
        other => panic!("{other:?}"),
    };
    assert_eq!(waits(nothing), [a, b]);
    assert_eq!(waits(empty_value), [a]);
    assert_eq!(waits(one_left), [b]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn skill_waits_for_and_receives_staged_values_of_its_model() {
    const KEY: &str = "PHX_TEST_SKILL_KEY";
    let settings = settings_of(vec![with_key("skill-model", &format!("${{{KEY}}}"), None)]);
    let operations = vec![set_value(KEY), skill_operation(Some("skill-model"))];
    let seen = Mutex::new(Vec::new());
    let ai = echoing_ai("skill-model", "done");
    let factory = |settings: &Settings| {
        seen.lock().unwrap().push(settings.models[0].resolved_api_key());
        ai.clone()
    };

    let waiting = Case::new(operations.clone(), settings.clone()).run(&factory).await;
    let tested = Case::new(operations, settings).values(&[(KEY, "sk-skill-777")]).run(&factory).await;

    assert!(matches!(waiting, RowTest::Waits { ref names, .. } if names == &[KEY]), "{waiting:?}");
    assert!(matches!(tested, RowTest::Tested(ref result) if result.ok), "{tested:?}");
    assert_eq!(*seen.lock().unwrap(), vec![Some("sk-skill-777".to_string())]);
}

#[tokio::test]
async fn new_model_destination_is_not_tested_unless_allowed() {
    const KEY: &str = "PHX_TEST_DEST_KEY";
    let reference = format!("${{{KEY}}}");
    let before = settings_of(vec![with_key("known", &reference, Some("https://alpha.example/api"))]);
    let moved = with_key("moved", &reference, Some("https://beta.example/api"));
    let same = with_key("same", &reference, Some("https://alpha.example/api"));
    let default_endpoint = with_key("default", &reference, None);
    let empty_base_url = with_key("empty", &reference, Some(""));
    let calls = AtomicUsize::new(0);
    let ai = echoing_ai("moved", "OK");
    let factory = |_: &Settings| {
        calls.fetch_add(1, Ordering::SeqCst);
        ai.clone()
    };
    let case = |model: &ModelConfig| {
        Case::new(vec![Operation::AddModel { model: model.clone() }], settings_of(vec![model.clone()]))
            .before(before.clone())
    };

    let blocked = case(&moved).on_click_only().run(&factory).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let allowed = case(&moved).run(&factory).await;
    let known = case(&same).on_click_only().run(&factory).await;
    let default_tested = case(&default_endpoint).on_click_only().run(&factory).await;
    let empty_tested = case(&empty_base_url).on_click_only().run(&factory).await;

    let RowTest::NewDestination(text) = blocked else { panic!("{blocked:?}") };
    assert_eq!(text, "test on click: PHX_TEST_DEST_KEY goes to new destination https://beta.example/api");
    assert!(matches!(allowed, RowTest::Tested(_)), "{allowed:?}");
    assert!(matches!(known, RowTest::Tested(_)), "{known:?}");
    assert!(matches!(default_tested, RowTest::Tested(_)), "{default_tested:?}");
    assert!(matches!(empty_tested, RowTest::Tested(_)), "{empty_tested:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn waiting_row_reports_its_new_destination_and_default_endpoint_is_known() {
    const KEY: &str = "PHX_TEST_WAIT_DEST_KEY";
    let reference = format!("${{{KEY}}}");
    let custom = with_key("custom", &reference, Some("https://beta.example/api"));
    let default_endpoint = with_key("default", &reference, None);
    let operations = |model: &ModelConfig| vec![set_value(KEY), Operation::AddModel { model: model.clone() }];
    let calls = AtomicUsize::new(0);
    let seen = Mutex::new(Vec::new());
    let ai = echoing_ai("default", "OK");
    let factory = |settings: &Settings| {
        calls.fetch_add(1, Ordering::SeqCst);
        seen.lock().unwrap().push(settings.models[0].resolved_api_key());
        ai.clone()
    };
    let case = |model: &ModelConfig| Case::new(operations(model), settings_of(vec![model.clone()]));

    let waiting_custom = case(&custom).on_click_only().run(&factory).await;
    let waiting_default = case(&default_endpoint).on_click_only().run(&factory).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let staged_custom = case(&custom).values(&[(KEY, "sk-dest-1")]).on_click_only().run(&factory).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let staged_default = case(&default_endpoint).values(&[(KEY, "sk-dest-2")]).on_click_only().run(&factory).await;

    let RowTest::Waits { names, new_destination } = waiting_custom else { panic!("{waiting_custom:?}") };
    assert_eq!(names, [KEY]);
    assert_eq!(
        new_destination.as_deref(),
        Some("test on click: PHX_TEST_WAIT_DEST_KEY goes to new destination https://beta.example/api")
    );
    let RowTest::Waits { names, new_destination } = waiting_default else { panic!("{waiting_default:?}") };
    assert_eq!(names, [KEY]);
    assert_eq!(new_destination, None);
    assert!(matches!(staged_custom, RowTest::NewDestination(_)), "{staged_custom:?}");
    assert!(matches!(staged_default, RowTest::Tested(ref result) if result.ok), "{staged_default:?}");
    assert_eq!(*seen.lock().unwrap(), vec![Some("sk-dest-2".to_string())]);
}

#[tokio::test]
async fn mcp_server_with_a_variable_of_another_servers_command_is_not_started() {
    const VAR: &str = "PHX_TEST_CMD_VAR";
    let command = "/nonexistent/promptheus-test-binary";
    let server = |command: &str, env: HashMap<String, String>| McpServerConfig {
        command: command.into(),
        args: vec![],
        env,
        timeout_secs: 5,
        tool_timeouts: HashMap::new(),
        ..Default::default()
    };
    let mut before = Settings::default();
    before.mcp_servers.insert("other".into(), server(&format!("${{{VAR}}}"), HashMap::new()));
    let added = server(command, HashMap::from([("TOKEN".into(), format!("${{{VAR}}}"))]));
    let mut planned = before.clone();
    planned.mcp_servers.insert("added".into(), added.clone());
    let case = || {
        Case::new(vec![Operation::AddMcpServer { name: "added".into(), config: added.clone().into() }], planned.clone())
            .before(before.clone())
    };
    let ai = echoing_ai("x", "OK");

    let blocked = case().on_click_only().run(&|_| ai.clone()).await;
    let allowed = case().run(&|_| ai.clone()).await;

    assert!(
        matches!(&blocked, RowTest::NewDestination(text)
            if text == &format!("test on click: {VAR} goes to new destination {command}")),
        "{blocked:?}"
    );
    let RowTest::Tested(tested) = allowed else { panic!("{allowed:?}") };
    assert!(tested.error.unwrap().contains("failed to start MCP server 'added'"));
}

#[test]
fn stt_sample_is_mono_16k_16bit_2_to_4_seconds() {
    let reader = hound::WavReader::new(std::io::Cursor::new(include_bytes!(
        "../../../resources/stt-sample.wav"
    )))
    .unwrap();
    let spec = reader.spec();
    assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 16000, 16));
    let seconds = reader.duration() as f64 / spec.sample_rate as f64;
    assert!((2.0..=4.0).contains(&seconds), "{seconds}");
}

#[tokio::test]
async fn test_model_config_reports_a_provider_error() {
    let error = AiError::ApiStatus { status: 401, message: "invalid api key".into() };
    let expected = error.to_string();
    let (provider, _) = MockProvider::new(vec![MockReply::Error(error)]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let dir = TempDir::new().unwrap();
    let result = test_model_config(&model("x", ModelType::Text), dir.path(), &|_| ai.clone()).await;
    assert!(!result.ok);
    assert_eq!(result.error, Some(expected));
    assert!(result.millis.is_some());
}

#[tokio::test]
async fn test_model_config_text_reply_is_ok() {
    let ai = echoing_ai("x", "OK");
    let dir = TempDir::new().unwrap();
    let result = test_model_config(&model("x", ModelType::Text), dir.path(), &|_| ai.clone()).await;
    assert!(result.ok);
    assert_eq!(result.output.as_deref(), Some("OK"));
    assert_eq!(result.error, None);
}

#[tokio::test]
async fn test_model_config_redacts_the_resolved_key() {
    let error = AiError::ApiStatus { status: 401, message: "bad key sk-secret-value".into() };
    let (provider, _) = MockProvider::new(vec![MockReply::Error(error)]);
    let ai = AiService::with_provider("x", Box::new(provider));
    let dir = TempDir::new().unwrap();
    let result =
        test_model_config(&with_key("x", "sk-secret-value", None), dir.path(), &|_| ai.clone()).await;
    let message = result.error.unwrap();
    assert!(message.contains("[redacted]"), "{message}");
    assert!(!message.contains("sk-secret-value"), "{message}");
}
