use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::models::message::ProcessedMessage;
use crate::models::settings::{McpServerConfig, ModelConfig, Provider, Settings};
use crate::models::skill::SkillFrontmatter;
use crate::services::ai::AiService;
use crate::services::change_log::TestResult;
use crate::services::env_resolve::{env_ref_names, resolve_env_refs};
use crate::services::mcp::client::McpClient;
use crate::services::missing_env::{visit_refs, visit_refs_mut};
use crate::services::placeholder_registry::{substitute, PlaceholderContext};
use crate::services::secrets::Redactor;
use crate::services::skill_message::wrap_skill;
use crate::services::speech::{transcribe, SttOptions};

use super::operations::Operation;
use super::plan::Plan;

const TEXT_PROBE: &str = "Reply with the single word OK.";
const STT_SAMPLE: &[u8] = include_bytes!("../../../resources/stt-sample.wav");
const MCP_CLOSE_TIMEOUT: Duration = Duration::from_secs(4);

pub type AiFactory<'a> = &'a (dyn Fn(&Settings) -> AiService + Sync);

pub struct RowTestRequest<'a> {
    pub row: usize,
    pub operations: &'a [Operation],
    pub plan: &'a Plan,
    pub before: &'a Settings,
    pub values: &'a BTreeMap<String, String>,
    pub ai_factory: AiFactory<'a>,
    pub placeholders: &'a PlaceholderContext,
    pub config_dir: &'a Path,
    pub allow_new_destinations: bool,
}

#[derive(Debug)]
pub enum RowTest {
    NotTestable,
    Waits { names: Vec<String>, new_destination: Option<String> },
    NewDestination(String),
    Tested(TestResult),
}

impl RowTest {
    fn kind(&self) -> &'static str {
        match self {
            Self::NotTestable => "not testable",
            Self::Waits { .. } => "waits",
            Self::NewDestination(_) => "new destination",
            Self::Tested(_) => "tested",
        }
    }
}

enum Subject<'a> {
    Model(&'a ModelConfig),
    Skill {
        frontmatter: &'a SkillFrontmatter,
        body: &'a str,
        sample_input: &'a str,
        model_id: String,
        model: Option<&'a ModelConfig>,
    },
    Mcp {
        name: &'a str,
        config: McpServerConfig,
    },
}

fn subject<'a>(operation: &'a Operation, settings: &'a Settings) -> Option<Subject<'a>> {
    match operation {
        Operation::AddModel { model } | Operation::UpdateModel { model, .. } => {
            Some(Subject::Model(settings.models.iter().find(|m| m.id == model.id).unwrap_or(model)))
        }
        Operation::CreateSkill { frontmatter, body, sample_input, .. }
        | Operation::UpdateSkill { frontmatter, body, sample_input, .. } => {
            let model_id = skill_model_id(frontmatter.model.as_deref(), settings);
            let model = settings.models.iter().find(|m| m.id == model_id);
            Some(Subject::Skill { frontmatter, body, sample_input, model_id, model })
        }
        Operation::AddMcpServer { name, config } | Operation::UpdateMcpServer { name, config } => {
            let stored = settings.mcp_servers.get(name);
            let config = stored.cloned().unwrap_or_else(|| config.clone().into_server(None));
            Some(Subject::Mcp { name, config })
        }
        _ => None,
    }
}

impl<'a> Subject<'a> {
    fn binding(&self) -> Option<Binding<'_>> {
        match self {
            Self::Model(model) => Some(Binding::Model(model)),
            Self::Skill { model, .. } => model.map(Binding::Model),
            Self::Mcp { name, config } => Some(Binding::Mcp { name, config }),
        }
    }
}

enum Binding<'a> {
    Model(&'a ModelConfig),
    Mcp { name: &'a str, config: &'a McpServerConfig },
}

impl Binding<'_> {
    fn variables(&self) -> BTreeSet<String> {
        let mut alone = Settings::default();
        let prefix = match self {
            Self::Model(model) => {
                alone.models.push((*model).clone());
                format!("models[{}].", model.id)
            }
            Self::Mcp { name, config } => {
                alone.mcp_servers.insert((*name).to_string(), (*config).clone());
                format!("mcp_servers.{name}.")
            }
        };
        let mut names = BTreeSet::new();
        visit_refs(&alone, |value, key, _| {
            if key.starts_with(&prefix) {
                names.extend(env_ref_names(value));
            }
        });
        names
    }

    fn destination(&self) -> String {
        match self {
            Self::Model(model) => model_destination(model),
            Self::Mcp { config, .. } => config.command.clone(),
        }
    }

    fn is_known_in(&self, before: &Settings, variable: &str) -> bool {
        match self {
            Self::Model(model) if has_default_endpoint(model) => true,
            Self::Model(model) => {
                let destination = model_destination(model);
                before.models.iter().any(|other| {
                    model_destination(other) == destination
                        && Binding::Model(other).variables().contains(variable)
                })
            }
            Self::Mcp { config, .. } => before.mcp_servers.iter().any(|(name, other)| {
                other.command == config.command
                    && Binding::Mcp { name, config: other }.variables().contains(variable)
            }),
        }
    }
}

fn has_default_endpoint(model: &ModelConfig) -> bool {
    model.base_url.as_deref().is_none_or(str::is_empty)
}

fn model_destination(model: &ModelConfig) -> String {
    match model.base_url.as_deref().filter(|url| !url.is_empty()) {
        Some(url) => url.to_string(),
        None => format!("{} default endpoint", provider_name(&model.provider.clone().unwrap_or_default())),
    }
}

fn provider_name(provider: &Provider) -> String {
    serde_json::to_value(provider)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn staged(values: &BTreeMap<String, String>) -> impl Iterator<Item = (&String, &String)> {
    values.iter().filter(|(_, value)| !value.is_empty())
}

fn waiting_names(
    variables: &BTreeSet<String>,
    operations: &[Operation],
    values: &BTreeMap<String, String>,
) -> Vec<String> {
    variables
        .iter()
        .filter(|name| {
            operations.iter().any(|op| matches!(op, Operation::SetEnvValue { name: set } if set == *name))
                && values.get(*name).is_none_or(String::is_empty)
        })
        .cloned()
        .collect()
}

fn new_destination_text(
    binding: &Binding,
    variables: &BTreeSet<String>,
    before: &Settings,
) -> Option<String> {
    let destination = binding.destination();
    let unknown: Vec<String> = variables
        .iter()
        .filter(|variable| !binding.is_known_in(before, variable))
        .map(|variable| format!("test on click: {variable} goes to new destination {destination}"))
        .collect();
    (!unknown.is_empty()).then(|| unknown.join("; "))
}

fn with_staged_values(settings: &Settings, values: &BTreeMap<String, String>) -> Settings {
    let tokens: Vec<(String, &String)> =
        staged(values).map(|(name, value)| (format!("${{{name}}}"), value)).collect();
    let mut substituted = settings.clone();
    visit_refs_mut(&mut substituted, |field| {
        for (token, value) in &tokens {
            *field = field.replace(token.as_str(), value);
        }
    });
    substituted
}

pub async fn run_row_test(request: &RowTestRequest<'_>) -> RowTest {
    let outcome = row_test(request).await;
    log::debug!("settings agent row {}: {}", request.row, outcome.kind());
    outcome
}

async fn row_test(request: &RowTestRequest<'_>) -> RowTest {
    let Some(operation) = request.operations.get(request.row) else {
        return RowTest::NotTestable;
    };
    let Some(subject) = subject(operation, &request.plan.settings) else {
        return RowTest::NotTestable;
    };
    let binding = subject.binding();
    let variables = binding.as_ref().map(Binding::variables).unwrap_or_default();
    let waits = waiting_names(&variables, request.operations, request.values);
    let new_destination = binding
        .as_ref()
        .and_then(|binding| new_destination_text(binding, &variables, request.before));
    if !waits.is_empty() {
        return RowTest::Waits { names: waits, new_destination };
    }
    if !request.allow_new_destinations {
        if let Some(text) = new_destination {
            return RowTest::NewDestination(text);
        }
    }
    run_subject(request, operation).await
}

async fn run_subject(request: &RowTestRequest<'_>, operation: &Operation) -> RowTest {
    let settings = with_staged_values(&request.plan.settings, request.values);
    let Some(subject) = subject(operation, &settings) else {
        return RowTest::NotTestable;
    };
    let start = Instant::now();
    let outcome = match subject {
        Subject::Model(model) => test_model(model, &settings, request.ai_factory).await,
        Subject::Skill { frontmatter, body, sample_input, model_id, .. } => {
            let message = wrap_skill(
                &frontmatter.name,
                &substitute(body, request.placeholders),
                sample_input,
            );
            complete_text(&(request.ai_factory)(&settings), &model_id, &message).await
        }
        Subject::Mcp { name, config } => test_mcp(name, &config).await,
    };
    let millis = Some(start.elapsed().as_millis() as u64);
    let redactor = Redactor::new(request.config_dir, &request.plan.settings)
        .with_values(staged(request.values).map(|(_, value)| value.clone()));
    RowTest::Tested(match outcome {
        Ok(output) => TestResult { ok: true, millis, error: None, output: Some(redactor.redact(&output)) },
        Err(error) => TestResult { ok: false, millis, error: Some(redactor.redact(&error)), output: None },
    })
}

fn skill_model_id(skill_model: Option<&str>, settings: &Settings) -> String {
    let surfaces = &settings.surfaces;
    skill_model
        .map(str::to_string)
        .or_else(|| surfaces.quick_actions.generation.model_id.clone())
        .or_else(|| surfaces.chat.generation.model_id.clone())
        .unwrap_or_default()
}

async fn complete_text(ai: &AiService, model_id: &str, message: &str) -> Result<String, String> {
    ai.complete(model_id, vec![ProcessedMessage::text("user", message)])
        .await
        .map_err(|e| e.to_string())
}

async fn test_model(
    model: &ModelConfig,
    settings: &Settings,
    ai_factory: AiFactory<'_>,
) -> Result<String, String> {
    if model.is_stt() {
        transcribe(STT_SAMPLE.to_vec(), model, &SttOptions::default())
            .await
            .map_err(|e| e.to_string())
    } else {
        complete_text(&ai_factory(settings), &model.id, TEXT_PROBE).await
    }
}

pub async fn test_model_config(
    model: &ModelConfig,
    config_dir: &Path,
    ai_factory: AiFactory<'_>,
) -> TestResult {
    let settings = Settings { models: vec![model.clone()], ..Settings::default() };
    let start = Instant::now();
    let outcome = test_model(model, &settings, ai_factory).await;
    let millis = Some(start.elapsed().as_millis() as u64);
    let resolved_key = model
        .api_key
        .as_deref()
        .map(resolve_env_refs)
        .filter(|key| !key.is_empty());
    let redactor = Redactor::new(config_dir, &settings).with_values(resolved_key);
    match outcome {
        Ok(output) => TestResult { ok: true, millis, error: None, output: Some(redactor.redact(&output)) },
        Err(error) => TestResult { ok: false, millis, error: Some(redactor.redact(&error)), output: None },
    }
}

async fn test_mcp(name: &str, config: &McpServerConfig) -> Result<String, String> {
    let client = McpClient::start(
        name,
        &config.resolved_command(),
        &config.resolved_args(),
        &config.resolved_env(),
        config.timeout_secs,
        &config.tool_timeouts,
    )
    .await
    .map_err(|e| e.to_string())?;
    let tools = client.list_tools().await.map_err(|e| e.to_string());
    client.close(MCP_CLOSE_TIMEOUT).await;
    Ok(tools?.iter().map(|t| t.name.to_string()).collect::<Vec<_>>().join(", "))
}
