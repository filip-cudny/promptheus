use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::{json, Value};
use tempfile::TempDir;

use super::apply::{apply, undo, ApplyError, Context, UndoOutcome};
use super::operations::{Operation, PromptFile};
use super::plan::{plan, plan_single, Current, Plan};
use super::plan_tests::{env_var, referencing_settings};
use crate::models::settings::{
    KeymapGroup, McpServerConfig, ModelConfig, ModelType, Provider, Settings, WebviewProvider,
};
use crate::models::skill::SkillFrontmatter;
use crate::services::ai::AiService;
use crate::services::change_log::{change_log_dir, load_all, secret_backup_dir, Effect, EntryKind, Target};
use crate::services::config::ConfigService;
use crate::services::env_resolve::{override_for, set_override};
use crate::services::secrets::{current_value, read_card_backup};

const OLD_SKILL: &str = "---\nname: old-skill\n---\n\nOld body.\n";
const HOST: &str = "test-host";

fn model(id: &str, model_type: ModelType, api_key: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        model: "m".to_string(),
        display_name: format!("Model {id}"),
        model_type,
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

fn provider(id: &str, url: &str) -> WebviewProvider {
    WebviewProvider { id: id.into(), name: format!("Provider {id}"), url: url.into() }
}

fn mcp(command: &str) -> McpServerConfig {
    McpServerConfig {
        command: command.into(),
        args: vec![],
        env: HashMap::new(),
        timeout_secs: 30,
        tool_timeouts: HashMap::new(),
        ..Default::default()
    }
}

fn frontmatter(slug: &str) -> SkillFrontmatter {
    SkillFrontmatter {
        name: slug.into(),
        display_name: None,
        description: Some("d".into()),
        model: None,
        parameters: None,
        offered_in: None,
    }
}

fn fixture_settings() -> Settings {
    let mut settings = Settings::default();
    settings.models = vec![
        model("abc", ModelType::Text, "${OPENAI_KEY}"),
        model("def", ModelType::Text, "${OTHER_KEY}"),
        model("stt1", ModelType::Stt, "${STT_KEY}"),
    ];
    settings.surfaces.chat.generation.model_id = Some("abc".into());
    settings.surfaces.quick_actions.generation.model_id = Some("abc".into());
    settings.surfaces.title_generation.generation.model_id = Some("abc".into());
    settings.surfaces.speech_to_text.model_id = Some("stt1".into());
    settings.skills_order = vec!["old-skill".into()];
    settings.keymaps = vec![KeymapGroup {
        context: format!("os == {}", std::env::consts::OS),
        bindings: HashMap::from([("ctrl+f1".to_string(), "open_context_menu".to_string())]),
    }];
    settings.webview_providers = vec![provider("claude", "https://claude.ai/")];
    settings.mcp_servers = HashMap::from([("tools".to_string(), mcp("run-tools"))]);
    settings
}

fn setup() -> (TempDir, ConfigService) {
    setup_with(fixture_settings())
}

fn setup_with(settings: Settings) -> (TempDir, ConfigService) {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("skills/old-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), OLD_SKILL).unwrap();
    std::fs::write(skill_dir.join("notes.txt"), "notes").unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    let config = ConfigService::load(dir.path()).unwrap();
    (dir, config)
}

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.insert(name, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out.retain(|name, _| !name.starts_with("change-log") && !name.starts_with("secret-backups"));
    out
}

fn log_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let log_dir = change_log_dir(dir);
    let mut files = BTreeMap::new();
    if let Ok(read) = std::fs::read_dir(&log_dir) {
        for entry in read.flatten() {
            files.insert(entry.file_name().to_string_lossy().into_owned(), std::fs::read(entry.path()).unwrap());
        }
    }
    files
}

fn make_plan(ops: &[Operation], config: &ConfigService, dir: &Path) -> Plan {
    plan(ops, &vec![true; ops.len()], &Current { settings: config.settings(), config_dir: dir })
        .unwrap_or_else(|errors| panic!("{errors:?}"))
}

fn run_apply(plan: &Plan, config: &mut ConfigService, dir: &Path) -> Result<String, ApplyError> {
    run_apply_with(plan, config, dir, &[])
}

fn run_apply_with(
    plan: &Plan,
    config: &mut ConfigService,
    dir: &Path,
    values: &[(&str, &str)],
) -> Result<String, ApplyError> {
    let settings = config.settings().clone();
    let log_dir = change_log_dir(dir);
    let mut ctx = Context {
        config,
        config_dir: dir,
        log_dir: &log_dir,
        host: HOST,
        request: Some("request".into()),
        reply: Some("reply".into()),
        tests: BTreeMap::new(),
        values: values.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect(),
    };
    apply(plan, &Current { settings: &settings, config_dir: dir }, &mut ctx)
}

fn run_undo(id: &str, force: bool, config: &mut ConfigService, dir: &Path) -> UndoOutcome {
    let log_dir = change_log_dir(dir);
    let mut ctx = Context {
        config,
        config_dir: dir,
        log_dir: &log_dir,
        host: HOST,
        request: None,
        reply: None,
        tests: BTreeMap::new(),
        values: BTreeMap::new(),
    };
    undo(id, force, &mut ctx).unwrap()
}

fn undone_id(outcome: UndoOutcome) -> String {
    match outcome {
        UndoOutcome::Undone { entry_id } => entry_id,
        other => panic!("expected Undone, got {other:?}"),
    }
}

fn set_setting(key: &str, value: Value) -> Operation {
    Operation::SetSetting { key: key.into(), value }
}

#[test]
fn a_failing_third_write_restores_the_first_two_and_writes_no_log() {
    let (dir, mut config) = setup();
    let ops = vec![
        set_setting("theme", json!("light")),
        Operation::WritePrompt { kind: PromptFile::System, content: "Be brief.".into() },
        Operation::CreateSkill {
            slug: "blocked".into(),
            frontmatter: frontmatter("blocked"),
            body: "Body".into(),
            sample_input: "x".into(),
        },
    ];
    let planned = make_plan(&ops, &config, dir.path());
    std::fs::write(dir.path().join("skills/blocked"), "in the way").unwrap();
    let start = tree(dir.path());

    let error = run_apply(&planned, &mut config, dir.path()).unwrap_err();

    assert!(matches!(error, ApplyError::WriteFailed { row: 2, .. }), "{error}");
    assert_eq!(tree(dir.path()), start);
    assert!(log_files(dir.path()).is_empty());
}

#[test]
fn a_stale_row_fails_apply_and_writes_nothing() {
    let (dir, mut config) = setup();
    let planned = make_plan(&[set_setting("theme", json!("light"))], &config, dir.path());
    config
        .try_mutate(|c| c.update_setting("theme", json!("light")))
        .unwrap();
    let start = tree(dir.path());

    let error = run_apply(&planned, &mut config, dir.path()).unwrap_err();

    assert!(matches!(&error, ApplyError::Stale(rows) if rows == &vec![0]), "{error}");
    assert_eq!(tree(dir.path()), start);
    assert!(log_files(dir.path()).is_empty());
}

#[test]
fn a_model_field_plan_applies_to_that_model_only() {
    let (dir, mut config) = setup();
    let planned = make_plan(&[set_setting("models[abc].store", json!(false))], &config, dir.path());

    run_apply(&planned, &mut config, dir.path()).unwrap();

    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("settings.json")).unwrap()).unwrap();
    let stores: Vec<(&str, &Value)> = written["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["id"].as_str().unwrap(), &m["store"]))
        .collect();
    assert_eq!(stores, vec![("abc", &json!(false)), ("def", &json!(true)), ("stt1", &json!(true))]);
}

fn every_operation() -> Vec<Operation> {
    vec![
        set_setting("theme", json!("light")),
        Operation::AddModel { model: model("123e4567-e89b-12d3-a456-426614174001", ModelType::Text, "${NEW_KEY}") },
        Operation::UpdateModel {
            id: "def".into(),
            model: ModelConfig { display_name: "Renamed".into(), ..model("def", ModelType::Text, "") },
        },
        Operation::DeleteModel { id: "def".into() },
        Operation::CreateSkill {
            slug: "fresh-skill".into(),
            frontmatter: frontmatter("fresh-skill"),
            body: "Body".into(),
            sample_input: "x".into(),
        },
        Operation::UpdateSkill {
            slug: "old-skill".into(),
            frontmatter: frontmatter("old-skill"),
            body: "Changed".into(),
            sample_input: "x".into(),
        },
        Operation::DeleteSkill { slug: "old-skill".into() },
        Operation::ReorderSkills { order: vec!["a-skill".into()] },
        Operation::WritePrompt { kind: PromptFile::System, content: "Be brief.".into() },
        Operation::WritePrompt { kind: PromptFile::SttKeyterms, content: "term".into() },
        Operation::AddMcpServer { name: "more".into(), config: mcp("run-more").into() },
        Operation::UpdateMcpServer { name: "tools".into(), config: mcp("run-other").into() },
        Operation::DeleteMcpServer { name: "tools".into() },
        Operation::AddWebviewProvider { provider: provider("chatgpt", "https://chatgpt.com/") },
        Operation::UpdateWebviewProvider {
            id: "claude".into(),
            provider: provider("claude", "https://claude.ai/new"),
        },
        Operation::DeleteWebviewProvider { id: "claude".into() },
        Operation::AddShortcut { action: "clear_context".into(), shortcut: "ctrl+f2".into() },
        Operation::RemoveShortcut { action: "open_context_menu".into(), shortcut: "ctrl+f1".into() },
    ]
}

#[test]
fn apply_then_undo_is_byte_equal_for_every_operation_type() {
    for operation in every_operation() {
        let name = operation.name();
        let (dir, mut config) = setup();
        let start = tree(dir.path());
        let planned = make_plan(std::slice::from_ref(&operation), &config, dir.path());

        let card_id = run_apply(&planned, &mut config, dir.path()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_ne!(tree(dir.path()), start, "{name} changed nothing");
        let card_file = log_files(dir.path());
        let outcome = run_undo(&card_id, false, &mut config, dir.path());

        let undo_id = undone_id(outcome);
        assert_eq!(tree(dir.path()), start, "{name}");
        let files = log_files(dir.path());
        assert_eq!(files.len(), 2, "{name}");
        let card_name = format!("{card_id}.json");
        assert_eq!(files[&card_name], card_file[&card_name], "{name}");
        let entries = load_all(&change_log_dir(dir.path()));
        let undo_entry = entries.iter().find(|e| e.id == undo_id).unwrap();
        assert_eq!(undo_entry.kind, EntryKind::Undo);
        assert_eq!(undo_entry.undoes.as_deref(), Some(card_id.as_str()));
    }
}

#[test]
fn a_remove_and_add_shortcut_card_changes_a_shortcut_and_undoes_to_the_start() {
    let mut settings = fixture_settings();
    settings.keymaps[0].bindings.insert("shift+f1".into(), "open_context_menu".into());
    let (dir, mut config) = setup_with(settings);
    let start = tree(dir.path());
    let ops = vec![
        Operation::RemoveShortcut { action: "open_context_menu".into(), shortcut: "ctrl+f1".into() },
        Operation::AddShortcut { action: "open_context_menu".into(), shortcut: "alt+f3".into() },
    ];
    let planned = make_plan(&ops, &config, dir.path());

    let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();

    let expected = HashMap::from([
        ("alt+f3".to_string(), "open_context_menu".to_string()),
        ("shift+f1".to_string(), "open_context_menu".to_string()),
    ]);
    assert_eq!(config.settings().keymaps[0].bindings, expected);
    undone_id(run_undo(&card_id, false, &mut config, dir.path()));
    assert_eq!(tree(dir.path()), start);
}

#[test]
fn a_whole_card_of_every_operation_type_round_trips() {
    let (dir, mut config) = setup();
    let start = tree(dir.path());
    let ops = vec![
        Operation::AddModel { model: model("123e4567-e89b-12d3-a456-426614174001", ModelType::Text, "${NEW_KEY}") },
        set_setting("theme", json!("light")),
        Operation::DeleteSkill { slug: "old-skill".into() },
        Operation::DeleteMcpServer { name: "tools".into() },
        Operation::DeleteWebviewProvider { id: "claude".into() },
        Operation::WritePrompt { kind: PromptFile::System, content: "Be brief.".into() },
    ];
    let planned = make_plan(&ops, &config, dir.path());

    let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();
    undone_id(run_undo(&card_id, false, &mut config, dir.path()));

    assert_eq!(tree(dir.path()), start);
}

#[test]
fn an_added_model_set_as_chat_default_resolves_in_the_ai_service() {
    set_override("PHX_APPLY_TEST_KEY", Some("sk-apply-test-1".into()));
    let (dir, mut config) = setup();
    let ops = vec![
        Operation::AddModel { model: model("123e4567-e89b-12d3-a456-426614174000", ModelType::Text, "${PHX_APPLY_TEST_KEY}") },
        set_setting("surfaces.chat.generation.model_id", json!("123e4567-e89b-12d3-a456-426614174000")),
    ];
    let planned = make_plan(&ops, &config, dir.path());

    run_apply(&planned, &mut config, dir.path()).unwrap();

    let settings = config.settings();
    assert!(AiService::new(&settings.models).has_model("123e4567-e89b-12d3-a456-426614174000"));
    assert_eq!(settings.surfaces.chat.generation.model_id.as_deref(), Some("123e4567-e89b-12d3-a456-426614174000"));
}

#[test]
fn undo_after_a_manual_change_returns_conflicts_and_force_restores() {
    let (dir, mut config) = setup();
    let start = tree(dir.path());
    let planned = make_plan(&[set_setting("theme", json!("light"))], &config, dir.path());
    let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();
    config
        .try_mutate(|c| c.update_setting("theme", json!("dark")))
        .unwrap();
    let changed = tree(dir.path());

    let outcome = run_undo(&card_id, false, &mut config, dir.path());

    let UndoOutcome::Conflicts { conflicts } = outcome else {
        panic!("expected conflicts");
    };
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].target, Target::Setting { path: "theme".into() });
    assert_eq!(conflicts[0].expected_after, Some(json!("light")));
    assert_eq!(conflicts[0].current, Some(json!("dark")));
    assert_eq!(tree(dir.path()), changed);
    assert_eq!(log_files(dir.path()).len(), 1);

    undone_id(run_undo(&card_id, true, &mut config, dir.path()));

    assert_eq!(tree(dir.path()), start);
}

#[test]
fn undo_of_delete_skill_restores_the_full_skill_file() {
    let (dir, mut config) = setup();
    let planned = make_plan(&[Operation::DeleteSkill { slug: "old-skill".into() }], &config, dir.path());
    let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();
    assert!(!dir.path().join("skills/old-skill").exists());

    undone_id(run_undo(&card_id, false, &mut config, dir.path()));

    let skill = dir.path().join("skills/old-skill");
    assert_eq!(std::fs::read_to_string(skill.join("SKILL.md")).unwrap(), OLD_SKILL);
    assert_eq!(std::fs::read_to_string(skill.join("notes.txt")).unwrap(), "notes");
    assert_eq!(config.settings().skills_order, vec!["old-skill".to_string()]);
}

#[test]
fn undo_of_the_undo_entry_reapplies_and_names_the_second_entry() {
    let (dir, mut config) = setup();
    let planned = make_plan(
        &[set_setting("theme", json!("light")), Operation::DeleteSkill { slug: "old-skill".into() }],
        &config,
        dir.path(),
    );
    let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();
    let applied = tree(dir.path());
    let undo_id = undone_id(run_undo(&card_id, false, &mut config, dir.path()));

    let redo_id = undone_id(run_undo(&undo_id, false, &mut config, dir.path()));

    assert_eq!(tree(dir.path()), applied);
    assert_eq!(log_files(dir.path()).len(), 3);
    let entries = load_all(&change_log_dir(dir.path()));
    let redo = entries.iter().find(|e| e.id == redo_id).unwrap();
    assert_eq!(redo.kind, EntryKind::Undo);
    assert_eq!(redo.undoes.as_deref(), Some(undo_id.as_str()));
}

fn rename(name: &str, new_name: &str) -> Operation {
    Operation::RenameEnvVar { name: name.into(), new_name: new_name.into() }
}

#[test]
fn undo_of_a_rename_restores_name_previous_names_and_every_reference() {
    let mut settings = referencing_settings("PHX_APPLY_OLD");
    settings.env_vars[0].renamed_from = vec!["PHX_APPLY_ANCIENT".into()];
    let (dir, mut config) = setup_with(settings);
    let start = tree(dir.path());
    let before = serde_json::to_value(config.settings()).unwrap();
    let planned = make_plan(&[rename("PHX_APPLY_OLD", "PHX_APPLY_NEW")], &config, dir.path());

    let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();
    let renamed = &config.settings().env_vars[1];
    assert_eq!(renamed.name, "PHX_APPLY_NEW");
    assert_eq!(renamed.renamed_from, vec!["PHX_APPLY_OLD", "PHX_APPLY_ANCIENT"]);
    undone_id(run_undo(&card_id, false, &mut config, dir.path()));

    assert_eq!(serde_json::to_value(config.settings()).unwrap(), before);
    assert_eq!(tree(dir.path()), start);
}

#[test]
fn a_separate_update_of_another_variable_does_not_conflict_with_the_rename_undo() {
    let (dir, mut config) = setup_with(referencing_settings("PHX_APPLY_A"));
    let start = tree(dir.path());
    let renamed = make_plan(&[rename("PHX_APPLY_A", "PHX_APPLY_A2")], &config, dir.path());
    let rename_id = run_apply(&renamed, &mut config, dir.path()).unwrap();
    let update = Operation::UpdateEnvVar {
        name: "PHX_PLAN_BYSTANDER".into(),
        description: "changed".into(),
        secret: true,
    };
    let updated = make_plan(&[update], &config, dir.path());
    run_apply(&updated, &mut config, dir.path()).unwrap();

    let outcome = run_undo(&rename_id, false, &mut config, dir.path());

    undone_id(outcome);
    let vars = &config.settings().env_vars;
    assert_eq!(vars[0].name, "PHX_APPLY_A");
    assert_eq!(vars[1].description, "changed");
    assert_ne!(tree(dir.path()), start);
}

#[test]
fn the_gui_path_writes_the_same_card_operations_as_the_agent_path() {
    let operation = || rename("PHX_APPLY_GUI_OLD", "PHX_APPLY_GUI_NEW");
    let operations_of = |through_gui: bool| {
        let (dir, mut config) = setup_with(referencing_settings("PHX_APPLY_GUI_OLD"));
        let current = Current { settings: config.settings(), config_dir: dir.path() };
        let planned = if through_gui {
            plan_single(operation(), &current).unwrap()
        } else {
            plan(&[operation()], &[true], &current).unwrap()
        };
        let id = run_apply(&planned, &mut config, dir.path()).unwrap();
        let entries = load_all(&change_log_dir(dir.path()));
        let entry = entries.into_iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.kind, EntryKind::Card);
        entry.operations
    };

    let (gui, agent) = (operations_of(true), operations_of(false));

    assert_eq!(gui.len(), agent.len());
    for (gui, agent) in gui.iter().zip(&agent) {
        assert_eq!(gui.label, agent.label);
        assert_eq!(gui.op, agent.op);
        assert_eq!(gui.effects, agent.effects);
        assert_eq!(gui.test, agent.test);
    }
}

#[test]
fn add_update_and_delete_env_var_each_round_trip_through_undo() {
    let mut settings = fixture_settings();
    settings.env_vars = vec![env_var("PHX_APPLY_EXISTING")];
    for operation in [
        Operation::AddEnvVar { name: "PHX_APPLY_ADDED".into(), description: "d".into(), secret: true },
        Operation::UpdateEnvVar { name: "PHX_APPLY_EXISTING".into(), description: "d".into(), secret: true },
        Operation::DeleteEnvVar { name: "PHX_APPLY_EXISTING".into() },
    ] {
        let name = operation.name();
        let (dir, mut config) = setup_with(settings.clone());
        let start = tree(dir.path());
        let planned = make_plan(std::slice::from_ref(&operation), &config, dir.path());

        let card_id = run_apply(&planned, &mut config, dir.path()).unwrap();
        assert_ne!(tree(dir.path()), start, "{name} changed nothing");
        undone_id(run_undo(&card_id, false, &mut config, dir.path()));

        assert_eq!(tree(dir.path()), start, "{name}");
    }
}

fn secret_effect(name: &str) -> Effect {
    Effect { target: Target::Secret { name: name.into() }, before: None, after: None }
}

fn card_operations(name: &str) -> Vec<Operation> {
    vec![
        Operation::AddEnvVar { name: name.into(), description: "d".into(), secret: true },
        Operation::SetEnvValue { name: name.into() },
        Operation::AddModel { model: model("123e4567-e89b-12d3-a456-426614174002", ModelType::Text, &format!("${{{name}}}")) },
    ]
}

fn env_text(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(".env")).unwrap()
}

fn backup_files(dir: &Path) -> usize {
    std::fs::read_dir(secret_backup_dir(dir)).map(|read| read.count()).unwrap_or(0)
}

#[test]
fn a_card_with_a_secret_row_writes_the_typed_value_with_the_settings() {
    let (dir, mut config) = setup();
    let planned = make_plan(&card_operations("PHX_CARD_NEW"), &config, dir.path());

    let id = run_apply_with(&planned, &mut config, dir.path(), &[("PHX_CARD_NEW", "sk-new-444")]).unwrap();

    assert_eq!(env_text(dir.path()), "PHX_CARD_NEW=\"sk-new-444\"\n");
    assert_eq!(current_value(dir.path(), "PHX_CARD_NEW").as_deref(), Some("sk-new-444"));
    assert!(config.settings().models.iter().any(|m| m.id == "123e4567-e89b-12d3-a456-426614174002"));
    let entries = load_all(&change_log_dir(dir.path()));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, EntryKind::Card);
    assert_eq!(entries[0].operations[1].effects, vec![secret_effect("PHX_CARD_NEW")]);
    let effect_json = serde_json::to_value(&entries[0].operations[1].effects[0]).unwrap();
    assert!(effect_json["before"].is_null() && effect_json["after"].is_null());
    let backup = std::fs::read_to_string(secret_backup_dir(dir.path()).join(format!("{id}.env"))).unwrap();
    assert_eq!(backup, "after__PHX_CARD_NEW=\"sk-new-444\"\n");
    let entry_file = std::fs::read_to_string(change_log_dir(dir.path()).join(format!("{id}.json"))).unwrap();
    assert!(!entry_file.contains("sk-new-444"));
}

#[test]
fn a_card_without_a_secret_row_writes_an_empty_backup() {
    let (dir, mut config) = setup();
    let planned = make_plan(&[set_setting("theme", json!("light"))], &config, dir.path());

    let id = run_apply(&planned, &mut config, dir.path()).unwrap();

    let backup = secret_backup_dir(dir.path()).join(format!("{id}.env"));
    assert_eq!(std::fs::read(backup).unwrap(), Vec::<u8>::new());
    assert!(!dir.path().join(".env").exists());
}

#[test]
fn a_value_without_a_selected_secret_row_is_never_written() {
    let (dir, mut config) = setup();
    let planned = make_plan(&card_operations("PHX_CARD_SELECTED"), &config, dir.path());
    let values = [("PHX_CARD_SELECTED", "sk-new-444"), ("PHX_CARD_UNSELECTED", "sk-other-555")];

    run_apply_with(&planned, &mut config, dir.path(), &values).unwrap();

    let env = env_text(dir.path());
    assert!(env.contains("PHX_CARD_SELECTED") && !env.contains("PHX_CARD_UNSELECTED"));
    assert_eq!(current_value(dir.path(), "PHX_CARD_UNSELECTED"), None);
}

#[test]
fn a_secret_row_without_a_value_fails_naming_the_variable_and_writes_nothing() {
    let (dir, mut config) = setup();
    let planned = make_plan(&card_operations("PHX_CARD_EMPTY"), &config, dir.path());
    let start = tree(dir.path());

    for values in [&[][..], &[("PHX_CARD_EMPTY", "")][..]] {
        let error = run_apply_with(&planned, &mut config, dir.path(), values).unwrap_err();

        assert!(matches!(&error, ApplyError::NoValue(name) if name == "PHX_CARD_EMPTY"), "{error}");
        assert!(error.to_string().contains("PHX_CARD_EMPTY"));
        assert_eq!(tree(dir.path()), start);
        assert!(log_files(dir.path()).is_empty());
        assert_eq!(backup_files(dir.path()), 0);
    }
}

#[test]
fn undo_restores_a_previous_value_removes_a_new_one_and_restores_the_settings() {
    let mut settings = fixture_settings();
    settings.env_vars = vec![env_var("PHX_CARD_OLD")];
    let (dir, mut config) = setup_with(settings);
    std::fs::write(dir.path().join(".env"), "KEEP=1\nPHX_CARD_OLD=\"old-value\"\n").unwrap();
    let start = tree(dir.path());
    let mut operations = card_operations("PHX_CARD_BOTH");
    operations.insert(2, Operation::SetEnvValue { name: "PHX_CARD_OLD".into() });
    let planned = make_plan(&operations, &config, dir.path());
    let values = [("PHX_CARD_BOTH", "sk-new-444"), ("PHX_CARD_OLD", "sk-changed-666")];
    let card_id = run_apply_with(&planned, &mut config, dir.path(), &values).unwrap();
    assert_eq!(current_value(dir.path(), "PHX_CARD_OLD").as_deref(), Some("sk-changed-666"));

    let undo_id = undone_id(run_undo(&card_id, false, &mut config, dir.path()));

    assert_eq!(tree(dir.path()), start);
    assert_eq!(current_value(dir.path(), "PHX_CARD_OLD").as_deref(), Some("old-value"));
    assert_eq!(current_value(dir.path(), "PHX_CARD_BOTH"), None);
    let backup = read_card_backup(dir.path(), &undo_id).unwrap();
    assert_eq!(backup.before.get("PHX_CARD_BOTH").map(String::as_str), Some("sk-new-444"));
    assert_eq!(backup.before.get("PHX_CARD_OLD").map(String::as_str), Some("sk-changed-666"));
    assert_eq!(backup.after, BTreeMap::from([("PHX_CARD_OLD".to_string(), "old-value".to_string())]));
}

#[test]
fn the_undo_of_a_card_with_a_secret_row_can_be_undone() {
    let (dir, mut config) = setup();
    let planned = make_plan(&card_operations("PHX_CARD_REDO"), &config, dir.path());
    let card_id = run_apply_with(&planned, &mut config, dir.path(), &[("PHX_CARD_REDO", "sk-new-444")]).unwrap();
    let applied = tree(dir.path());
    let undo_id = undone_id(run_undo(&card_id, false, &mut config, dir.path()));
    assert_eq!(current_value(dir.path(), "PHX_CARD_REDO"), None);

    undone_id(run_undo(&undo_id, false, &mut config, dir.path()));

    assert_eq!(tree(dir.path()), applied);
    assert_eq!(current_value(dir.path(), "PHX_CARD_REDO").as_deref(), Some("sk-new-444"));
}

#[test]
fn undo_after_the_value_changed_elsewhere_conflicts_without_a_value_and_force_restores() {
    let (dir, mut config) = setup();
    let planned = make_plan(&card_operations("PHX_CARD_DRIFT"), &config, dir.path());
    let card_id = run_apply_with(&planned, &mut config, dir.path(), &[("PHX_CARD_DRIFT", "sk-new-444")]).unwrap();
    set_override("PHX_CARD_DRIFT", Some("sk-drift-777".into()));
    let changed = tree(dir.path());

    let outcome = run_undo(&card_id, false, &mut config, dir.path());

    let UndoOutcome::Conflicts { conflicts } = outcome else {
        panic!("expected conflicts");
    };
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].target, Target::Secret { name: "PHX_CARD_DRIFT".into() });
    assert_eq!((conflicts[0].expected_after.clone(), conflicts[0].current.clone()), (None, None));
    assert_eq!(tree(dir.path()), changed);
    assert_eq!(log_files(dir.path()).len(), 1);

    undone_id(run_undo(&card_id, true, &mut config, dir.path()));

    assert_eq!(current_value(dir.path(), "PHX_CARD_DRIFT"), None);
    assert!(config.settings().models.iter().all(|m| m.id != "123e4567-e89b-12d3-a456-426614174002"));
}

#[test]
fn undo_without_the_backup_is_skipped_and_writes_nothing() {
    let (dir, mut config) = setup();
    let planned = make_plan(&card_operations("PHX_CARD_FOREIGN"), &config, dir.path());
    let card_id = run_apply_with(&planned, &mut config, dir.path(), &[("PHX_CARD_FOREIGN", "sk-new-444")]).unwrap();
    std::fs::remove_dir_all(secret_backup_dir(dir.path())).unwrap();
    let applied = tree(dir.path());

    let outcome = run_undo(&card_id, true, &mut config, dir.path());

    assert_eq!(outcome, UndoOutcome::Skipped { held_by: HOST.into() });
    assert_eq!(tree(dir.path()), applied);
    assert_eq!(log_files(dir.path()).len(), 1);
    assert_eq!(current_value(dir.path(), "PHX_CARD_FOREIGN").as_deref(), Some("sk-new-444"));
}

#[test]
fn a_failing_settings_write_after_the_secret_write_restores_env_overrides_and_backup() {
    let mut settings = fixture_settings();
    settings.env_vars = vec![env_var("PHX_FAIL_KEPT"), env_var("PHX_FAIL_NEW")];
    let (dir, mut config) = setup_with(settings);
    std::fs::write(dir.path().join(".env"), "PHX_FAIL_KEPT=\"prev\"\n").unwrap();
    set_override("PHX_FAIL_KEPT", Some("prev".into()));
    let operations = vec![
        Operation::SetEnvValue { name: "PHX_FAIL_KEPT".into() },
        Operation::SetEnvValue { name: "PHX_FAIL_NEW".into() },
        Operation::CreateSkill {
            slug: "blocked".into(),
            frontmatter: frontmatter("blocked"),
            body: "Body".into(),
            sample_input: "x".into(),
        },
    ];
    let planned = make_plan(&operations, &config, dir.path());
    std::fs::write(dir.path().join("skills/blocked"), "in the way").unwrap();
    let start = tree(dir.path());
    let values = [("PHX_FAIL_KEPT", "sk-a-111"), ("PHX_FAIL_NEW", "sk-b-222")];

    let error = run_apply_with(&planned, &mut config, dir.path(), &values).unwrap_err();

    assert!(matches!(error, ApplyError::WriteFailed { row: 2, .. }), "{error}");
    assert_eq!(tree(dir.path()), start);
    assert_eq!(override_for("PHX_FAIL_KEPT"), Some(Some("prev".to_string())));
    assert_eq!(override_for("PHX_FAIL_NEW"), None);
    assert_eq!(backup_files(dir.path()), 0);
    assert!(log_files(dir.path()).is_empty());
}
