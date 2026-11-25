use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::models::settings::{EnvVar, McpServerConfig, ModelConfig, Settings, WebviewProvider};
use crate::models::skill::SkillFrontmatter;
use crate::services::change_log::{Effect, LoggedOperation, Target};
use crate::services::config::catalog::{self, Kind, ModelKind};
use crate::services::config::{resolve_config_relative, validate, ConfigError, ConfigService, PromptStore};
use crate::services::env_resolve::env_ref_names;
use crate::services::missing_env::{collect_missing, visit_refs_mut};
use crate::services::secrets;
use crate::services::skill::{validate_slug, SkillError};

use super::engine::describe;
use super::operations::Operation;

const SETTINGS_MANAGED_BY_OTHER_OPERATIONS: [&str; 5] =
    ["models", "mcp_servers", "webview_providers", "keymaps", "env_vars"];

const AGENT_TOOLS_KEY: &str = "surfaces.settings_agent.generation.enabled_tools";

const OPAQUE_PATHS: [&str; 5] = [
    "models[].parameters",
    "models[].capabilities",
    "mcp_servers.*.env",
    "mcp_servers.*.tool_timeouts",
    "keymaps[].bindings",
];

pub struct Current<'a> {
    pub settings: &'a Settings,
    pub config_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowError {
    pub row: usize,
    pub key: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PlannedRow {
    pub row: usize,
    pub operation: LoggedOperation,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub settings: Settings,
    pub rows: Vec<PlannedRow>,
}

struct Fail {
    key: Option<String>,
    message: String,
}

impl Fail {
    fn new(message: impl Into<String>) -> Self {
        Self { key: None, message: message.into() }
    }

    fn at(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self { key: Some(key.into()), message: message.into() }
    }
}

impl From<ConfigError> for Fail {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::InvalidSettings { key, message } => Self { key, message },
            other => Self::new(other.to_string()),
        }
    }
}

impl From<SkillError> for Fail {
    fn from(error: SkillError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for Fail {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Clone)]
struct State {
    config: ConfigService,
    files: BTreeMap<String, Option<String>>,
}

impl State {
    fn settings(&self) -> &Settings {
        self.config.settings()
    }

    fn read(&self, current: &Current, path: &str) -> Result<Option<String>, Fail> {
        match self.files.get(path) {
            Some(content) => Ok(content.clone()),
            None => read_disk(current.config_dir, path),
        }
    }

    fn write(
        &mut self,
        current: &Current,
        path: &str,
        content: Option<String>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), Fail> {
        let before = self.read(current, path)?;
        self.files.insert(path.to_string(), content.clone());
        if before != content {
            effects.push(Effect {
                target: Target::File { path: path.to_string() },
                before: before.map(Value::String),
                after: content.map(Value::String),
            });
        }
        Ok(())
    }

    fn skill_files(&self, current: &Current, slug: &str) -> BTreeSet<String> {
        let prefix = format!("skills/{slug}/");
        let mut paths = BTreeSet::new();
        collect_files(&current.config_dir.join("skills").join(slug), &prefix, &mut paths);
        for (path, content) in self.files.iter().filter(|(path, _)| path.starts_with(&prefix)) {
            if content.is_some() {
                paths.insert(path.clone());
            } else {
                paths.remove(path);
            }
        }
        paths
    }
}

fn read_disk(config_dir: &Path, path: &str) -> Result<Option<String>, Fail> {
    match std::fs::read(config_dir.join(path)) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| Fail::new(format!("{path} is not valid UTF-8"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Fail::new(format!("cannot read {path}: {e}"))),
    }
}

fn collect_files(dir: &Path, prefix: &str, out: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if path.is_dir() {
            collect_files(&path, &format!("{name}/"), out);
        } else if path.is_file() {
            out.insert(name);
        }
    }
}

fn skill_path(slug: &str) -> String {
    format!("skills/{slug}/SKILL.md")
}

fn render_skill(slug: &str, frontmatter: &SkillFrontmatter, body: &str) -> Result<String, Fail> {
    validate_slug(slug)?;
    if frontmatter.name != slug {
        return Err(Fail::new("frontmatter name must match slug"));
    }
    let yaml = serde_yaml::to_string(frontmatter).map_err(|e| Fail::new(e.to_string()))?;
    Ok(format!("---\n{yaml}---\n\n{}\n", body.trim_end()))
}

fn check_api_key(model_id: &str, api_key: Option<&str>) -> Result<(), Fail> {
    let key = api_key.unwrap_or("");
    let names = env_ref_names(key);
    let single_reference = names.len() == 1
        && secrets::validate_name(&names[0])
        && key == format!("${{{}}}", names[0]);
    if key.is_empty() || single_reference {
        return Ok(());
    }
    Err(Fail::at(
        format!("models[{model_id}].api_key"),
        "secrets only through the secret element",
    ))
}

fn find_model<'a>(settings: &'a Settings, id: &str) -> Result<&'a ModelConfig, Fail> {
    settings
        .models
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Fail::new(format!("model '{id}' not found; models: {}", model_ids(settings, |_| true))))
}

fn model_ids(settings: &Settings, keep: impl Fn(&ModelConfig) -> bool) -> String {
    let ids: Vec<&str> = settings.models.iter().filter(|m| keep(m)).map(|m| m.id.as_str()).collect();
    ids.join(", ")
}

fn skill_not_found(settings: &Settings, slug: &str) -> Fail {
    Fail::new(format!("skill '{slug}' not found; skills: {}", settings.skills_order.join(", ")))
}

fn set_mcp_servers(
    state: &mut State,
    servers: std::collections::HashMap<String, McpServerConfig>,
) -> Result<(), Fail> {
    state.config.update_setting("mcp_servers", serde_json::to_value(servers)?)?;
    Ok(())
}

fn set_webview_providers(state: &mut State, providers: Vec<WebviewProvider>) -> Result<(), Fail> {
    state
        .config
        .update_setting("webview_providers", serde_json::to_value(providers)?)?;
    Ok(())
}

fn set_env_vars(state: &mut State, vars: Vec<EnvVar>) -> Result<(), Fail> {
    state.config.update_setting("env_vars", serde_json::to_value(vars)?)?;
    Ok(())
}

fn env_var_position(settings: &Settings, name: &str) -> Result<usize, Fail> {
    settings
        .env_vars
        .iter()
        .position(|var| var.name == name)
        .ok_or_else(|| {
            let names: Vec<&str> = settings.env_vars.iter().map(|var| var.name.as_str()).collect();
            Fail::at(format!("env_vars[{name}]"), format!("variable not found; variables: {}", names.join(", ")))
        })
}

fn add_env_var(state: &mut State, name: &str, description: &str, secret: bool) -> Result<(), Fail> {
    if !secrets::validate_name(name) {
        return Err(Fail::at(format!("env_vars[{name}].name"), "not a valid variable name"));
    }
    let mut vars = state.settings().env_vars.clone();
    if vars.iter().any(|var| var.name == name) {
        return Err(Fail::at(format!("env_vars[{name}]"), "variable already exists"));
    }
    vars.push(EnvVar {
        name: name.to_string(),
        description: description.to_string(),
        secret,
        renamed_from: Vec::new(),
    });
    set_env_vars(state, vars)
}

fn update_env_var(state: &mut State, name: &str, description: &str, secret: bool) -> Result<(), Fail> {
    let mut vars = state.settings().env_vars.clone();
    let var = &mut vars[env_var_position(state.settings(), name)?];
    var.description = description.to_string();
    var.secret = secret;
    set_env_vars(state, vars)
}

fn rename_env_var(
    state: &mut State,
    current: &Current,
    name: &str,
    new_name: &str,
) -> Result<(), Fail> {
    let mut settings = state.settings().clone();
    let position = env_var_position(&settings, name)?;
    let key = format!("env_vars[{name}].name");
    if !secrets::validate_name(new_name) {
        return Err(Fail::at(key, format!("{new_name} is not a valid variable name")));
    }
    if settings.env_vars.iter().any(|var| var.name == new_name) {
        return Err(Fail::at(key, format!("{new_name} already exists")));
    }
    let new_value = secrets::current_value(current.config_dir, new_name).filter(|v| !v.is_empty());
    if let Some(new_value) = new_value {
        let old_value = secrets::current_value(current.config_dir, name);
        if old_value.as_deref() != Some(new_value.as_str()) {
            return Err(Fail::at(key, format!("{new_name} already has a different value on this machine")));
        }
    }
    let var = &mut settings.env_vars[position];
    let mut renamed_from = vec![name.to_string()];
    for old in &var.renamed_from {
        if old != new_name && !renamed_from.contains(old) {
            renamed_from.push(old.clone());
        }
    }
    var.name = new_name.to_string();
    var.renamed_from = renamed_from;
    let (old_token, new_token) = (format!("${{{name}}}"), format!("${{{new_name}}}"));
    visit_refs_mut(&mut settings, |value| *value = value.replace(&old_token, &new_token));
    state.config = ConfigService::detached(settings, current.config_dir);
    Ok(())
}

fn delete_env_var(state: &mut State, name: &str) -> Result<(), Fail> {
    let mut vars = state.settings().env_vars.clone();
    let position = env_var_position(state.settings(), name)?;
    let uses = collect_missing(state.settings(), |_| false)
        .into_iter()
        .find(|missing| missing.name == name);
    if let Some(missing) = uses {
        let labels: Vec<&str> = missing.used_in.iter().map(|u| u.label.as_str()).collect();
        return Err(Fail::at(format!("env_vars[{name}]"), format!("used in: {}", labels.join(", "))));
    }
    vars.remove(position);
    set_env_vars(state, vars)
}

fn is_live_list_item_key(key: &str, top: &str) -> bool {
    let catalog_key = catalog::catalog_path(key);
    matches!(top, "models" | "webview_providers")
        && catalog::ENTRIES
            .iter()
            .any(|entry| entry.key == catalog_key && entry.applies == catalog::Applies::Live)
}

fn check_list_item_setting(settings: &Settings, key: &str, value: &Value) -> Result<(), Fail> {
    let segments = parse_path(key);
    let Some(Segment::Item(id)) = segments.get(1) else {
        return Err(Fail::at(key, "list item id missing"));
    };
    if key.starts_with("webview_providers") {
        if !settings.webview_providers.iter().any(|p| &p.id == id) {
            return Err(Fail::at(key, format!("web provider '{id}' not found")));
        }
        return Ok(());
    }
    find_model(settings, id).map_err(|f| Fail::at(key, f.message))?;
    if catalog::catalog_path(key) == "models[].api_key" {
        let api_key = value
            .as_str()
            .ok_or_else(|| Fail::at(key, "api_key must be a string"))?;
        check_api_key(id, Some(api_key))?;
    }
    Ok(())
}

fn set_setting(state: &mut State, current: &Current, key: &str, value: Value) -> Result<(), Fail> {
    if key == AGENT_TOOLS_KEY {
        return Err(Fail::at(
            key,
            "the user picks the settings agent's tools in Settings; the agent cannot enable tools for itself",
        ));
    }
    let top = key.split(['.', '[']).next().unwrap_or("");
    let list_item = is_live_list_item_key(key, top);
    let settable = list_item
        || (!SETTINGS_MANAGED_BY_OTHER_OPERATIONS.contains(&top)
            && catalog::ENTRIES.iter().any(|entry| entry.key == key));
    if !settable {
        return Err(Fail::at(
            key,
            "not a settable catalog key (models, MCP servers, web providers and shortcuts have their own operations)",
        ));
    }
    if list_item {
        check_list_item_setting(state.settings(), key, &value)?;
    }
    let mut json = serde_json::to_value(state.settings())?;
    set_at(&mut json, &parse_path(key), Some(value)).map_err(|m| Fail::at(key, m))?;
    let settings: Settings =
        serde_json::from_value(json).map_err(|e| Fail::at(key, e.to_string()))?;
    state.config = ConfigService::detached(settings, current.config_dir);
    Ok(())
}

fn write_prompt(
    state: &mut State,
    current: &Current,
    kind: super::operations::PromptFile,
    content: &str,
    effects: &mut Vec<Effect>,
) -> Result<(), Fail> {
    let dir = current.config_dir;
    let resolved = match kind.prompt_kind() {
        Some(prompt_kind) => {
            let raw = state.config.prompt_path(prompt_kind).ok_or_else(|| {
                Fail::new(format!("no path configured for prompt kind {prompt_kind:?}"))
            })?;
            PromptStore::new(dir).resolve(raw)?
        }
        None => {
            let raw = state.config.stt_keyterms_path_or_default().to_string();
            let resolved = resolve_config_relative(&raw, dir)?;
            state.config.adopt_keyterms_path();
            resolved
        }
    };
    let relative = resolved
        .strip_prefix(dir)
        .map_err(|_| Fail::new("prompt path is outside the config directory"))?
        .to_string_lossy()
        .into_owned();
    state.write(current, &relative, Some(content.to_string()), effects)
}

fn apply(state: &mut State, current: &Current, op: &Operation) -> Result<Vec<Effect>, Fail> {
    let mut effects = Vec::new();
    match op {
        Operation::SetSetting { key, value } => set_setting(state, current, key, value.clone())?,
        Operation::AddModel { model } => {
            if uuid::Uuid::parse_str(&model.id).is_err() {
                return Err(Fail::at(
                    format!("models[{}].id", model.id),
                    "model id must be a new UUID, e.g. 123e4567-e89b-12d3-a456-426614174000",
                ));
            }
            check_api_key(&model.id, model.api_key.as_deref())?;
            state.config.add_model(model.clone());
        }
        Operation::UpdateModel { id, model } => {
            let stored_key = find_model(state.settings(), id)?.api_key.clone();
            if model.id != *id {
                return Err(Fail::at(format!("models[{id}].id"), "model id cannot change"));
            }
            let mut model = model.clone();
            if model.api_key.as_deref().is_none_or(str::is_empty) {
                model.api_key = stored_key.clone();
            }
            if model.api_key != stored_key {
                check_api_key(id, model.api_key.as_deref())?;
            }
            state.config.update_model(id, model);
        }
        Operation::DeleteModel { id } => {
            find_model(state.settings(), id)?;
            state.config.delete_model(id);
        }
        Operation::CreateSkill { slug, frontmatter, body, .. } => {
            let content = render_skill(slug, frontmatter, body)?;
            let path = skill_path(slug);
            if state.read(current, &path)?.is_some() {
                return Err(Fail::new(format!("skill '{slug}' already exists")));
            }
            state.write(current, &path, Some(content), &mut effects)?;
            let mut order = state.settings().skills_order.clone();
            if !order.contains(slug) {
                order.push(slug.clone());
            }
            state.config.update_skills_order(order);
        }
        Operation::UpdateSkill { slug, frontmatter, body, .. } => {
            let content = render_skill(slug, frontmatter, body)?;
            let path = skill_path(slug);
            if state.read(current, &path)?.is_none() {
                return Err(skill_not_found(state.settings(), slug));
            }
            state.write(current, &path, Some(content), &mut effects)?;
        }
        Operation::DeleteSkill { slug } => {
            validate_slug(slug)?;
            if state.read(current, &skill_path(slug))?.is_none() {
                return Err(skill_not_found(state.settings(), slug));
            }
            for path in state.skill_files(current, slug) {
                state.write(current, &path, None, &mut effects)?;
            }
            let mut order = state.settings().skills_order.clone();
            order.retain(|name| name != slug);
            state.config.update_skills_order(order);
        }
        Operation::ReorderSkills { order } => state.config.update_skills_order(order.clone()),
        Operation::WritePrompt { kind, content } => {
            write_prompt(state, current, *kind, content, &mut effects)?
        }
        Operation::AddMcpServer { name, config } => {
            let mut servers = state.settings().mcp_servers.clone();
            if servers.insert(name.clone(), config.clone().into_server(None)).is_some() {
                return Err(Fail::at(format!("mcp_servers.{name}"), "MCP server already exists"));
            }
            set_mcp_servers(state, servers)?;
        }
        Operation::UpdateMcpServer { name, config } => {
            let mut servers = state.settings().mcp_servers.clone();
            let updated = config.clone().into_server(servers.get(name));
            if servers.insert(name.clone(), updated).is_none() {
                return Err(Fail::at(format!("mcp_servers.{name}"), "MCP server not found"));
            }
            set_mcp_servers(state, servers)?;
        }
        Operation::DeleteMcpServer { name } => {
            let mut servers = state.settings().mcp_servers.clone();
            if servers.remove(name).is_none() {
                return Err(Fail::at(format!("mcp_servers.{name}"), "MCP server not found"));
            }
            set_mcp_servers(state, servers)?;
        }
        Operation::AddWebviewProvider { provider } => {
            let mut providers = state.settings().webview_providers.clone();
            if providers.iter().any(|p| p.id == provider.id) {
                return Err(Fail::at(
                    format!("webview_providers[{}]", provider.id),
                    "web provider already exists",
                ));
            }
            providers.push(provider.clone());
            set_webview_providers(state, providers)?;
        }
        Operation::UpdateWebviewProvider { id, provider } => {
            let mut providers = state.settings().webview_providers.clone();
            let key = format!("webview_providers[{id}]");
            let slot = providers
                .iter_mut()
                .find(|p| p.id == *id)
                .ok_or_else(|| Fail::at(key.clone(), "web provider not found"))?;
            if provider.id != *id {
                return Err(Fail::at(key, "web provider id cannot change"));
            }
            *slot = provider.clone();
            set_webview_providers(state, providers)?;
        }
        Operation::DeleteWebviewProvider { id } => {
            let mut providers = state.settings().webview_providers.clone();
            let before = providers.len();
            providers.retain(|p| p.id != *id);
            if providers.len() == before {
                return Err(Fail::at(format!("webview_providers[{id}]"), "web provider not found"));
            }
            set_webview_providers(state, providers)?;
        }
        Operation::AddShortcut { action, shortcut } => state.config.add_shortcut(action, shortcut)?,
        Operation::RemoveShortcut { action, shortcut } => {
            state.config.remove_shortcut(action, shortcut)?
        }
        Operation::AddEnvVar { name, description, secret } => {
            add_env_var(state, name, description, *secret)?
        }
        Operation::UpdateEnvVar { name, description, secret } => {
            update_env_var(state, name, description, *secret)?
        }
        Operation::RenameEnvVar { name, new_name } => rename_env_var(state, current, name, new_name)?,
        Operation::DeleteEnvVar { name } => delete_env_var(state, name)?,
        Operation::SetEnvValue { name } => {
            env_var_position(state.settings(), name)?;
            effects.push(Effect { target: Target::Secret { name: name.clone() }, before: None, after: None });
        }
    }
    Ok(effects)
}

fn check_model_references(settings: &Settings) -> Result<(), Fail> {
    let json = serde_json::to_value(settings)?;
    for entry in catalog::ENTRIES {
        let Kind::Model { model_type } = entry.kind else {
            continue;
        };
        let Some(id) = get_at(&json, &parse_path(entry.key)).and_then(Value::as_str) else {
            continue;
        };
        let wrong_type = |model: &ModelConfig| match model_type {
            ModelKind::Text => !model.is_text(),
            ModelKind::Stt => !model.is_stt(),
        };
        match settings.models.iter().find(|m| m.id == id) {
            None => {
                let valid = model_ids(settings, |model| !wrong_type(model));
                return Err(Fail::at(
                    entry.key,
                    format!("{} references unknown model id '{id}'; models: {valid}", entry.key),
                ));
            }
            Some(model) if wrong_type(model) => {
                return Err(Fail::at(
                    entry.key,
                    format!("{} model '{id}' has the wrong type ({:?} expected)", entry.key, model_type),
                ))
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn run_row(state: &State, current: &Current, op: &Operation) -> Result<(State, LoggedOperation), Fail> {
    let mut next = state.clone();
    let before = serde_json::to_value(state.settings())?;
    let file_effects = apply(&mut next, current, op)?;
    validate(next.settings())?;
    catalog::check_changed(state.settings(), next.settings())?;
    check_model_references(next.settings())?;
    let mut effects = Vec::new();
    diff("", &before, &serde_json::to_value(next.settings())?, &mut effects);
    effects.extend(file_effects);
    let operation = LoggedOperation {
        label: op.label(state.settings()),
        op: op.name().to_string(),
        effects,
        test: None,
    };
    Ok((next, operation))
}

struct Run {
    state: State,
    rows: Vec<PlannedRow>,
    errors: Vec<RowError>,
}

fn run(ops: &[Operation], selected: &[bool], current: &Current) -> Run {
    let mut state = State {
        config: ConfigService::detached(current.settings.clone(), current.config_dir),
        files: BTreeMap::new(),
    };
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (row, op) in ops.iter().enumerate().filter(|(row, _)| selected[*row]) {
        match run_row(&state, current, op) {
            Ok((next, operation)) => {
                state = next;
                rows.push(PlannedRow { row, operation });
            }
            Err(fail) => errors.push(RowError { row, key: fail.key, message: fail.message }),
        }
    }
    Run { state, rows, errors }
}

pub fn plan(ops: &[Operation], selected: &[bool], current: &Current) -> Result<Plan, Vec<RowError>> {
    let selected: Vec<bool> = (0..ops.len()).map(|i| selected.get(i).copied().unwrap_or(false)).collect();
    let chosen = run(ops, &selected, current);
    log::debug!(
        "settings plan: {} of {} rows selected, {} errors",
        chosen.rows.len(),
        ops.len(),
        chosen.errors.len()
    );
    if chosen.errors.is_empty() {
        return Ok(Plan {
            settings: chosen.state.config.settings().clone(),
            rows: chosen.rows,
        });
    }
    let everything = run(ops, &vec![true; ops.len()], current);
    Err(chosen
        .errors
        .into_iter()
        .map(|error| explain_dependency(error, ops, &selected, current, &everything))
        .collect())
}

pub(crate) fn plan_single(op: Operation, current: &Current) -> Result<Plan, String> {
    plan(&[op], &[true], current).map_err(|errors| describe(&errors))
}

fn explain_dependency(
    error: RowError,
    ops: &[Operation],
    selected: &[bool],
    current: &Current,
    everything: &Run,
) -> RowError {
    if everything.errors.iter().any(|e| e.row == error.row) {
        return error;
    }
    let passes_with = |added: &[usize]| {
        let mut with_rows = selected.to_vec();
        for row in added {
            with_rows[*row] = true;
        }
        !run(ops, &with_rows, current).errors.iter().any(|e| e.row == error.row)
    };
    let unchecked: Vec<usize> = (0..error.row).filter(|row| !selected[*row]).collect();
    let mut blockers: Vec<usize> = unchecked.iter().copied().filter(|row| passes_with(&[*row])).collect();
    if blockers.is_empty() {
        blockers = unchecked.clone();
        for row in &unchecked {
            let without: Vec<usize> = blockers.iter().copied().filter(|b| b != row).collect();
            if passes_with(&without) {
                blockers = without;
            }
        }
    }
    if blockers.is_empty() || !passes_with(&blockers) {
        return error;
    }
    let noun = if blockers.len() == 1 { "row" } else { "rows" };
    let numbers: Vec<String> = blockers.iter().map(|row| (row + 1).to_string()).collect();
    RowError { message: format!("depends on unchecked {noun} {}", numbers.join(", ")), ..error }
}

pub(super) struct Mismatch<'a> {
    pub row: usize,
    pub effect: &'a Effect,
    pub current: Option<Value>,
}

pub(super) fn mismatches<'a>(rows: &'a [PlannedRow], current: &Current) -> Vec<Mismatch<'a>> {
    let mut json = serde_json::to_value(current.settings).unwrap_or(Value::Null);
    let mut files: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut found = Vec::new();
    for planned in rows {
        for effect in &planned.operation.effects {
            let (expected, actual) = match &effect.target {
                Target::Setting { path } => {
                    let segments = parse_path(path);
                    let is_removal =
                        effect.after.is_none() && matches!(segments.last(), Some(Segment::Item(_)));
                    let expected = effect
                        .before
                        .as_ref()
                        .map(|before| if is_removal { removed_value(before) } else { before });
                    let actual = get_at(&json, &segments).cloned();
                    let _ = put_at(&mut json, &segments, effect.after.clone());
                    (expected.cloned(), actual)
                }
                Target::File { path } => {
                    let working = files
                        .entry(path.clone())
                        .or_insert_with(|| read_disk(current.config_dir, path).ok().flatten());
                    let actual = working.clone().map(Value::String);
                    *working = effect.after.as_ref().and_then(Value::as_str).map(str::to_string);
                    (effect.before.clone(), actual)
                }
                Target::Secret { .. } => continue,
            };
            let is_setting = matches!(effect.target, Target::Setting { .. });
            let as_absent = |value: &Option<Value>| match value {
                Some(Value::Null) if is_setting => None,
                other => other.clone(),
            };
            if as_absent(&actual) != as_absent(&expected) {
                found.push(Mismatch { row: planned.row, effect, current: actual });
            }
        }
    }
    found
}

pub fn stale_rows(plan: &Plan, current: &Current) -> Vec<usize> {
    let mut stale: Vec<usize> = mismatches(&plan.rows, current).iter().map(|m| m.row).collect();
    stale.dedup();
    stale
}

pub(super) enum Segment {
    Key(String),
    Item(String),
}

pub(super) fn parse_path(path: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut rest = path;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('.') {
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('[') {
            let end = tail.find(']').unwrap_or(tail.len());
            segments.push(Segment::Item(tail[..end].to_string()));
            rest = tail.get(end + 1..).unwrap_or("");
        } else {
            let end = rest.find(['.', '[']).unwrap_or(rest.len());
            segments.push(Segment::Key(rest[..end].to_string()));
            rest = &rest[end..];
        }
    }
    segments
}

fn item_key(item: &Value) -> Option<String> {
    ["id", "context", "name"]
        .iter()
        .find_map(|field| item.get(field).and_then(Value::as_str))
        .map(str::to_string)
}

pub(super) fn get_at<'a>(root: &'a Value, segments: &[Segment]) -> Option<&'a Value> {
    segments.iter().try_fold(root, |node, segment| match segment {
        Segment::Key(key) => node.get(key),
        Segment::Item(id) => node
            .as_array()?
            .iter()
            .find(|item| item_key(item).as_deref() == Some(id.as_str())),
    })
}

fn set_at(node: &mut Value, segments: &[Segment], new: Option<Value>) -> Result<(), String> {
    let Some((first, rest)) = segments.split_first() else {
        return Err("empty path".to_string());
    };
    match first {
        Segment::Key(key) => {
            let map = node.as_object_mut().ok_or_else(|| format!("'{key}' is not inside an object"))?;
            if !rest.is_empty() {
                let child = map.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()));
                return set_at(child, rest, new);
            }
            match new {
                Some(value) => map.insert(key.clone(), value),
                None => map.remove(key),
            };
            Ok(())
        }
        Segment::Item(id) => {
            let items = node.as_array_mut().ok_or_else(|| format!("'[{id}]' is not inside a list"))?;
            let position = items.iter().position(|item| item_key(item).as_deref() == Some(id.as_str()));
            match (position, rest.is_empty(), new) {
                (Some(at), true, Some(value)) => items[at] = value,
                (Some(at), true, None) => {
                    items.remove(at);
                }
                (None, true, Some(value)) => items.push(value),
                (None, true, None) => {}
                (Some(at), false, new) => set_at(&mut items[at], rest, new)?,
                (None, false, _) => return Err(format!("no list item '{id}'")),
            }
            Ok(())
        }
    }
}

pub(super) fn put_at(node: &mut Value, segments: &[Segment], new: Option<Value>) -> Result<(), String> {
    let at_index = match (segments.last(), &new) {
        (Some(Segment::Item(_)), Some(Value::Object(map))) if map.len() == 2 => map
            .get("index")
            .and_then(Value::as_u64)
            .zip(map.get("value"))
            .map(|(index, value)| (index as usize, value.clone())),
        _ => None,
    };
    let Some((index, value)) = at_index else {
        return set_at(node, segments, new);
    };
    set_at(node, segments, Some(value))?;
    let (Some(Segment::Item(id)), parent_path) = (segments.last(), &segments[..segments.len() - 1]) else {
        return Ok(());
    };
    let Some(items) = parent_mut(node, parent_path).and_then(Value::as_array_mut) else {
        return Ok(());
    };
    if let Some(at) = items.iter().position(|item| item_key(item).as_deref() == Some(id.as_str())) {
        let item = items.remove(at);
        items.insert(index.min(items.len()), item);
    }
    Ok(())
}

fn parent_mut<'a>(root: &'a mut Value, segments: &[Segment]) -> Option<&'a mut Value> {
    let mut node = root;
    for segment in segments {
        node = match segment {
            Segment::Key(key) => node.get_mut(key)?,
            Segment::Item(id) => node
                .as_array_mut()?
                .iter_mut()
                .find(|item| item_key(item).as_deref() == Some(id.as_str()))?,
        };
    }
    Some(node)
}

/// A removed list item is recorded as `{"index": <position>, "value": <item>}` in `before` so an undo can reinsert it at the same position.
fn removed_item(index: usize, item: &Value) -> Value {
    json!({ "index": index, "value": item })
}

pub(super) fn removed_value(before: &Value) -> &Value {
    match before {
        Value::Object(map) if map.len() == 2 && map.contains_key("index") => {
            map.get("value").unwrap_or(before)
        }
        _ => before,
    }
}

fn setting(path: &str, before: Option<Value>, after: Option<Value>) -> Effect {
    Effect { target: Target::Setting { path: path.to_string() }, before, after }
}

fn diff(path: &str, before: &Value, after: &Value, out: &mut Vec<Effect>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(old), Value::Object(new))
            if !OPAQUE_PATHS.contains(&catalog::catalog_path(path).as_str()) =>
        {
            let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
            for key in keys {
                let child = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                match (old.get(key), new.get(key)) {
                    (Some(b), Some(a)) => diff(&child, b, a, out),
                    (b, a) => out.push(setting(&child, b.cloned(), a.cloned())),
                }
            }
        }
        (Value::Array(old), Value::Array(new))
            if old.iter().all(|i| item_key(i).is_some()) && new.iter().all(|i| item_key(i).is_some()) =>
        {
            diff_items(path, old, new, out)
        }
        _ => out.push(setting(path, Some(before.clone()), Some(after.clone()))),
    }
}

fn diff_items(path: &str, old: &[Value], new: &[Value], out: &mut Vec<Effect>) {
    let find = |items: &[Value], key: &str| items.iter().position(|i| item_key(i).as_deref() == Some(key));
    for (index, item) in old.iter().enumerate() {
        let Some(key) = item_key(item) else { continue };
        let child = format!("{path}[{key}]");
        match find(new, &key) {
            Some(at) => diff(&child, item, &new[at], out),
            None => out.push(setting(&child, Some(removed_item(index, item)), None)),
        }
    }
    for item in new {
        let Some(key) = item_key(item) else { continue };
        if find(old, &key).is_none() {
            out.push(setting(&format!("{path}[{key}]"), None, Some(item.clone())));
        }
    }
}
