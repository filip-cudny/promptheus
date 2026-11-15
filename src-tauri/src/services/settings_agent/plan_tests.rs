use std::collections::HashMap;

use serde_json::{json, Value};
use tempfile::TempDir;

use super::operations::{Operation, PromptFile};
use super::plan::{plan, stale_rows, Current, Plan, RowError};
use crate::models::settings::{
    EnvVar, KeymapGroup, McpServerConfig, ModelConfig, ModelType, Provider, Settings, WebviewProvider,
};
use crate::models::skill::SkillFrontmatter;
use crate::services::change_log::{Effect, Target};

const OLD_SKILL: &str = "---\nname: old-skill\n---\n\nOld body.\n";

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

fn config_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("skills/old-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), OLD_SKILL).unwrap();
    std::fs::write(skill_dir.join("notes.txt"), "notes").unwrap();
    dir
}

fn set_setting(key: &str, value: Value) -> Operation {
    Operation::SetSetting { key: key.into(), value }
}

fn plan_all(ops: &[Operation], settings: &Settings, dir: &TempDir) -> Result<Plan, Vec<RowError>> {
    plan(ops, &vec![true; ops.len()], &Current { settings, config_dir: dir.path() })
}

fn effects_of(plan: &Plan) -> Vec<&Effect> {
    plan.rows.iter().flat_map(|row| &row.operation.effects).collect()
}

fn setting_effect<'a>(plan: &'a Plan, path: &str) -> &'a Effect {
    effects_of(plan)
        .into_iter()
        .find(|e| e.target == Target::Setting { path: path.to_string() })
        .unwrap_or_else(|| panic!("no effect for {path}"))
}

#[test]
fn dependent_row_plans_when_selected_and_names_the_unchecked_row() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        Operation::AddModel { model: model("x", ModelType::Text, "${X_KEY}") },
        set_setting("surfaces.chat.generation.model_id", json!("x")),
    ];
    let current = Current { settings: &settings, config_dir: dir.path() };

    let planned = plan(&ops, &[true, true], &current).expect("both selected");
    assert_eq!(planned.settings.surfaces.chat.generation.model_id.as_deref(), Some("x"));
    assert_eq!(planned.rows.len(), 2);

    let errors = plan(&ops, &[false, true], &current).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].row, 1);
    assert!(errors[0].message.contains("depends on unchecked row 1"), "{}", errors[0].message);
}

#[test]
fn settings_agent_model_and_reasoning_effort_plan_as_settings() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        set_setting("surfaces.settings_agent.generation.model_id", json!("def")),
        set_setting("surfaces.settings_agent.generation.parameters.reasoning_effort", json!("high")),
    ];

    let planned = plan_all(&ops, &settings, &dir).expect("both plan");
    let agent = &planned.settings.surfaces.settings_agent.generation;
    assert_eq!(agent.model_id.as_deref(), Some("def"));
    assert_eq!(agent.parameters.reasoning_effort.as_deref(), Some("high"));
}

#[test]
fn settings_agent_tools_cannot_be_set_by_the_agent() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let key = "surfaces.settings_agent.generation.enabled_tools";
    let ops = vec![set_setting(key, json!(["srv.web_search"]))];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].row, 0);
    assert_eq!(errors[0].key.as_deref(), Some(key));
    assert!(errors[0].message.contains("in Settings"), "{}", errors[0].message);
}

#[test]
fn row_needing_two_unchecked_rows_names_both() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        Operation::AddModel { model: model("x", ModelType::Text, "${X_KEY}") },
        Operation::UpdateModel { id: "x".into(), model: model("x", ModelType::Stt, "${X_KEY}") },
        set_setting("surfaces.speech_to_text.model_id", json!("x")),
    ];
    let current = Current { settings: &settings, config_dir: dir.path() };

    plan(&ops, &[true, true, true], &current).expect("all selected");
    let errors = plan(&ops, &[false, false, true], &current).unwrap_err();
    let row3 = errors.iter().find(|e| e.row == 2).expect("row 3 error");
    assert_eq!(row3.message, "depends on unchecked rows 1, 2");
}

#[test]
fn literal_api_key_is_a_row_error_and_env_reference_passes() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let literal = vec![Operation::AddModel { model: model("x", ModelType::Text, "sk-secret") }];
    let errors = plan_all(&literal, &settings, &dir).unwrap_err();
    assert_eq!(errors[0].row, 0);
    assert_eq!(errors[0].key.as_deref(), Some("models[x].api_key"));
    assert_eq!(errors[0].message, "secrets only through the secret element");

    let reference = vec![Operation::AddModel { model: model("x", ModelType::Text, "${OPENAI_KEY}") }];
    plan_all(&reference, &settings, &dir).expect("reference passes");
}

#[test]
fn update_model_keeps_the_stored_key() {
    let (mut settings, dir) = (fixture_settings(), config_dir());
    settings.models[1].api_key = Some("sk-stored".into());
    let mut updated = model("def", ModelType::Text, "sk-stored");
    updated.display_name = "Renamed".into();

    let planned = plan_all(&[Operation::UpdateModel { id: "def".into(), model: updated }], &settings, &dir)
        .expect("stored key passes");

    assert_eq!(planned.settings.models[1].api_key.as_deref(), Some("sk-stored"));
    assert_eq!(planned.settings.models[1].display_name, "Renamed");
}

fn update_tools_server_without(settings: &mut Settings, current: McpServerConfig) -> Settings {
    settings.mcp_servers.insert("tools".into(), current);
    let operation: Operation = serde_json::from_value(json!({
        "op": "update_mcp_server",
        "name": "tools",
        "config": { "command": "run-other" }
    }))
    .unwrap();
    plan_all(&[operation], settings, &config_dir()).expect("update plans").settings
}

#[test]
fn update_mcp_server_without_disabled_keeps_current_value() {
    let mut settings = fixture_settings();

    let planned = update_tools_server_without(&mut settings, McpServerConfig { disabled: true, ..mcp("run-tools") });

    assert_eq!(planned.mcp_servers["tools"].command, "run-other");
    assert!(planned.mcp_servers["tools"].disabled);
}

#[test]
fn update_mcp_server_without_disabled_tools_keeps_current_list() {
    let mut settings = fixture_settings();
    let current = McpServerConfig { disabled_tools: vec!["search".into()], ..mcp("run-tools") };

    let planned = update_tools_server_without(&mut settings, current);

    assert_eq!(planned.mcp_servers["tools"].disabled_tools, vec!["search"]);
    assert!(!planned.mcp_servers["tools"].disabled);
}

#[test]
fn out_of_range_setting_is_a_row_error_with_its_key() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let errors = plan_all(&[set_setting("history_retention_days", json!(5000))], &settings, &dir).unwrap_err();

    assert_eq!(errors[0].row, 0);
    assert_eq!(errors[0].key.as_deref(), Some("history_retention_days"));
}

#[test]
fn delete_skill_effects_hold_the_full_file_content_as_before() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let planned = plan_all(&[Operation::DeleteSkill { slug: "old-skill".into() }], &settings, &dir).unwrap();

    let skill = effects_of(&planned)
        .into_iter()
        .find(|e| e.target == Target::File { path: "skills/old-skill/SKILL.md".into() })
        .expect("SKILL.md effect");
    assert_eq!(skill.before, Some(json!(OLD_SKILL)));
    assert_eq!(skill.after, None);
    let notes = effects_of(&planned)
        .into_iter()
        .find(|e| e.target == Target::File { path: "skills/old-skill/notes.txt".into() })
        .expect("notes.txt effect");
    assert_eq!(notes.before, Some(json!("notes")));
    assert!(planned.settings.skills_order.is_empty());
}

#[test]
fn delete_skill_with_non_utf8_file_is_a_row_error() {
    let (settings, dir) = (fixture_settings(), config_dir());
    std::fs::write(dir.path().join("skills/old-skill/blob.bin"), [0xff, 0xfe, 0x00]).unwrap();

    let errors = plan_all(&[Operation::DeleteSkill { slug: "old-skill".into() }], &settings, &dir).unwrap_err();

    assert!(errors[0].message.contains("not valid UTF-8"), "{}", errors[0].message);
}

#[test]
fn delete_model_of_the_chat_default_yields_model_and_surface_effects() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let planned = plan_all(&[Operation::DeleteModel { id: "abc".into() }], &settings, &dir).unwrap();

    assert_eq!(planned.rows[0].operation.label, "Delete model Model abc");
    let removed = setting_effect(&planned, "models[abc]");
    assert_eq!(removed.after, None);
    assert_eq!(removed.before.as_ref().unwrap()["index"], json!(0));
    let surface = setting_effect(&planned, "surfaces.chat.generation.model_id");
    assert_eq!(surface.before, Some(json!("abc")));
    assert_eq!(surface.after, Some(Value::Null));
}

#[test]
fn stale_rows_returns_only_the_row_whose_before_changed() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        set_setting("theme", json!("light")),
        set_setting("theme", json!("dark")),
        set_setting("recent_apps_count", json!(7)),
    ];
    let planned = plan_all(&ops, &settings, &dir).unwrap();
    assert_eq!(stale_rows(&planned, &Current { settings: &settings, config_dir: dir.path() }), Vec::<usize>::new());

    let mut changed = settings.clone();
    changed.recent_apps_count = 99;

    assert_eq!(stale_rows(&planned, &Current { settings: &changed, config_dir: dir.path() }), vec![2]);
}

#[test]
fn stale_rows_detects_a_changed_file() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![Operation::UpdateSkill {
        slug: "old-skill".into(),
        frontmatter: frontmatter("old-skill"),
        body: "New body.".into(),
        sample_input: "x".into(),
    }];
    let planned = plan_all(&ops, &settings, &dir).unwrap();
    let current = Current { settings: &settings, config_dir: dir.path() };
    assert!(stale_rows(&planned, &current).is_empty());

    std::fs::write(dir.path().join("skills/old-skill/SKILL.md"), "edited").unwrap();

    assert_eq!(stale_rows(&planned, &current), vec![0]);
}

#[test]
fn every_operation_type_plans_on_the_fixture() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let operations = vec![
        set_setting("theme", json!("light")),
        Operation::AddModel { model: model("new", ModelType::Text, "${NEW_KEY}") },
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
        Operation::UpdateWebviewProvider { id: "claude".into(), provider: provider("claude", "https://claude.ai/new") },
        Operation::DeleteWebviewProvider { id: "claude".into() },
        Operation::AddShortcut { action: "clear_context".into(), shortcut: "ctrl+f2".into() },
        Operation::RemoveShortcut { action: "open_context_menu".into(), shortcut: "ctrl+f1".into() },
    ];

    for operation in operations {
        let name = operation.name();
        let planned = plan_all(std::slice::from_ref(&operation), &settings, &dir)
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        assert_eq!(planned.rows.len(), 1, "{name}");
        assert_eq!(planned.rows[0].operation.op, name);
        assert!(!planned.rows[0].operation.effects.is_empty(), "{name} has no effects");
    }
}

#[test]
fn operation_deserializes_from_its_tagged_form_and_labels_itself() {
    let operation: Operation = serde_json::from_value(json!({
        "op": "set_setting",
        "key": "theme",
        "value": "light"
    }))
    .unwrap();
    assert_eq!(operation.label(&Settings::default()), "Theme");

    let add: Operation = serde_json::from_value(json!({
        "op": "add_model",
        "model": serde_json::to_value(model("x", ModelType::Text, "")).unwrap()
    }))
    .unwrap();
    assert_eq!(add.label(&Settings::default()), "Add model Model x");
}

#[test]
fn shortcut_operations_label_themselves() {
    let settings = Settings::default();
    let add = Operation::AddShortcut { action: "clear_context".into(), shortcut: "ctrl+f2".into() };
    let remove = Operation::RemoveShortcut { action: "open_context_menu".into(), shortcut: "ctrl+f1".into() };

    assert_eq!(add.label(&settings), "Add shortcut ctrl+f2 to Clear context");
    assert_eq!(remove.label(&settings), "Remove shortcut ctrl+f1 from Open context menu");
}

#[test]
fn removing_a_shortcut_not_bound_to_the_action_fails_its_row() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![Operation::RemoveShortcut { action: "clear_context".into(), shortcut: "ctrl+f1".into() }];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].row, 0);
    assert_eq!(errors[0].key.as_deref(), Some("keymaps.clear_context"));
    assert!(errors[0].message.contains("is not bound to"), "{}", errors[0].message);
}

#[test]
fn env_var_operations_deserialize_with_defaults_and_label_themselves() {
    let parse = |value: Value| serde_json::from_value::<Operation>(value).unwrap();
    let settings = Settings::default();

    let add = parse(json!({ "op": "add_env_var", "name": "A" }));
    assert_eq!((add.name(), add.label(&settings).as_str()), ("add_env_var", "Add variable A"));
    let update = parse(json!({ "op": "update_env_var", "name": "A", "description": "d", "secret": true }));
    assert_eq!((update.name(), update.label(&settings).as_str()), ("update_env_var", "Update variable A"));
    let rename = parse(json!({ "op": "rename_env_var", "name": "A", "new_name": "B" }));
    assert_eq!((rename.name(), rename.label(&settings).as_str()), ("rename_env_var", "Rename variable A to B"));
    let delete = parse(json!({ "op": "delete_env_var", "name": "A" }));
    assert_eq!((delete.name(), delete.label(&settings).as_str()), ("delete_env_var", "Delete variable A"));
}

#[test]
fn skill_errors_are_attributed_to_their_row() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        Operation::CreateSkill {
            slug: "old-skill".into(),
            frontmatter: frontmatter("old-skill"),
            body: "b".into(),
            sample_input: "x".into(),
        },
        Operation::UpdateSkill {
            slug: "missing-skill".into(),
            frontmatter: frontmatter("missing-skill"),
            body: "b".into(),
            sample_input: "x".into(),
        },
        Operation::DeleteSkill { slug: "missing-skill".into() },
        Operation::CreateSkill {
            slug: "Bad Slug".into(),
            frontmatter: frontmatter("Bad Slug"),
            body: "b".into(),
            sample_input: "x".into(),
        },
        Operation::CreateSkill {
            slug: "other-skill".into(),
            frontmatter: frontmatter("different"),
            body: "b".into(),
            sample_input: "x".into(),
        },
    ];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    assert_eq!(errors.iter().map(|e| e.row).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
}

pub(super) fn env_var(name: &str) -> EnvVar {
    EnvVar { name: name.into(), description: String::new(), secret: false, renamed_from: vec![] }
}

pub(super) fn referencing_settings(name: &str) -> Settings {
    let token = format!("${{{name}}}");
    let mut settings = fixture_settings();
    settings.env_vars = vec![env_var(name), env_var("PHX_PLAN_BYSTANDER")];
    settings.models[0].api_key = Some(token.clone());
    settings.mcp_servers = HashMap::from([(
        "tools".to_string(),
        McpServerConfig {
            command: token.clone(),
            args: vec![token.clone(), format!("{token}/x"), format!("${{{name}_X}}")],
            env: HashMap::from([("LLM_API_KEY".to_string(), token.clone())]),
            ..mcp("run-tools")
        },
    )]);
    let base = &mut settings.prompt_base;
    base.system = format!("{token}/system.md");
    base.about_you = format!("{token}/about.md");
    base.environment = format!("{token}/environment.md");
    base.input_format = format!("{token}/input.md");
    settings.surfaces.title_generation.prompt = format!("{token}/title.md");
    settings.surfaces.speech_to_text.prompt = Some(format!("{token}/stt.md"));
    settings.surfaces.speech_to_text.keyterms_file = Some(format!("{token}/terms.md"));
    settings
}

fn rename(name: &str, new_name: &str) -> Operation {
    Operation::RenameEnvVar { name: name.into(), new_name: new_name.into() }
}

fn env_names(settings: &Settings) -> Vec<&str> {
    settings.env_vars.iter().map(|var| var.name.as_str()).collect()
}

#[test]
fn rename_env_var_rewrites_every_reference_and_records_the_old_name() {
    let (settings, dir) = (referencing_settings("PHX_PLAN_OLD"), config_dir());

    let planned = plan_all(&[rename("PHX_PLAN_OLD", "PHX_PLAN_NEW")], &settings, &dir).unwrap();

    let renamed = &planned.settings;
    assert_eq!(env_names(renamed), vec!["PHX_PLAN_NEW", "PHX_PLAN_BYSTANDER"]);
    assert_eq!(renamed.env_vars[0].renamed_from, vec!["PHX_PLAN_OLD"]);
    assert_eq!(renamed.models[0].api_key.as_deref(), Some("${PHX_PLAN_NEW}"));
    let server = &renamed.mcp_servers["tools"];
    assert_eq!(server.command, "${PHX_PLAN_NEW}");
    assert_eq!(
        server.args,
        vec!["${PHX_PLAN_NEW}", "${PHX_PLAN_NEW}/x", "${PHX_PLAN_OLD_X}"]
    );
    assert_eq!(server.env["LLM_API_KEY"], "${PHX_PLAN_NEW}");
    let base = &renamed.prompt_base;
    assert_eq!(base.system, "${PHX_PLAN_NEW}/system.md");
    assert_eq!(base.about_you, "${PHX_PLAN_NEW}/about.md");
    assert_eq!(base.environment, "${PHX_PLAN_NEW}/environment.md");
    assert_eq!(base.input_format, "${PHX_PLAN_NEW}/input.md");
    assert_eq!(renamed.surfaces.title_generation.prompt, "${PHX_PLAN_NEW}/title.md");
    let stt = &renamed.surfaces.speech_to_text;
    assert_eq!(stt.prompt.as_deref(), Some("${PHX_PLAN_NEW}/stt.md"));
    assert_eq!(stt.keyterms_file.as_deref(), Some("${PHX_PLAN_NEW}/terms.md"));
}

#[test]
fn a_second_rename_puts_the_newest_old_name_first() {
    let (settings, dir) = (referencing_settings("PHX_PLAN_FIRST"), config_dir());
    let ops = [rename("PHX_PLAN_FIRST", "PHX_PLAN_SECOND"), rename("PHX_PLAN_SECOND", "PHX_PLAN_THIRD")];

    let planned = plan_all(&ops, &settings, &dir).unwrap();

    assert_eq!(
        planned.settings.env_vars[0].renamed_from,
        vec!["PHX_PLAN_SECOND", "PHX_PLAN_FIRST"]
    );
}

#[test]
fn rename_env_var_refuses_unknown_taken_and_invalid_names() {
    let (settings, dir) = (referencing_settings("PHX_PLAN_KNOWN"), config_dir());
    let ops = vec![
        rename("PHX_PLAN_MISSING", "PHX_PLAN_X"),
        rename("PHX_PLAN_KNOWN", "PHX_PLAN_BYSTANDER"),
        rename("PHX_PLAN_KNOWN", "1 bad"),
    ];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    let keys: Vec<_> = errors.iter().map(|e| e.key.as_deref()).collect();
    assert_eq!(
        keys,
        vec![
            Some("env_vars[PHX_PLAN_MISSING]"),
            Some("env_vars[PHX_PLAN_KNOWN].name"),
            Some("env_vars[PHX_PLAN_KNOWN].name"),
        ]
    );
}

#[test]
fn rename_env_var_refuses_a_target_with_a_different_value_on_this_machine() {
    let (settings, dir) = (referencing_settings("PHX_PLAN_VAL_OLD"), config_dir());
    let env_file = dir.path().join(".env");

    std::fs::write(&env_file, "PHX_PLAN_VAL_OLD=a\nPHX_PLAN_VAL_NEW=b\n").unwrap();
    let errors = plan_all(&[rename("PHX_PLAN_VAL_OLD", "PHX_PLAN_VAL_NEW")], &settings, &dir).unwrap_err();
    assert_eq!(errors[0].key.as_deref(), Some("env_vars[PHX_PLAN_VAL_OLD].name"));
    assert_eq!(
        errors[0].message,
        "PHX_PLAN_VAL_NEW already has a different value on this machine"
    );

    std::fs::write(&env_file, "PHX_PLAN_VAL_NEW=b\n").unwrap();
    assert!(plan_all(&[rename("PHX_PLAN_VAL_OLD", "PHX_PLAN_VAL_NEW")], &settings, &dir).is_err());

    std::fs::write(&env_file, "PHX_PLAN_VAL_OLD=a\nPHX_PLAN_VAL_NEW=a\n").unwrap();
    assert!(plan_all(&[rename("PHX_PLAN_VAL_OLD", "PHX_PLAN_VAL_NEW")], &settings, &dir).is_ok());

    std::fs::write(&env_file, "PHX_PLAN_VAL_OLD=a\n").unwrap();
    assert!(plan_all(&[rename("PHX_PLAN_VAL_OLD", "PHX_PLAN_VAL_NEW")], &settings, &dir).is_ok());
}

#[test]
fn delete_env_var_names_every_usage_and_removes_an_unreferenced_entry() {
    let (settings, dir) = (referencing_settings("PHX_PLAN_USED"), config_dir());

    let errors = plan_all(&[Operation::DeleteEnvVar { name: "PHX_PLAN_USED".into() }], &settings, &dir)
        .unwrap_err();

    assert_eq!(errors[0].key.as_deref(), Some("env_vars[PHX_PLAN_USED]"));
    for label in ["Model abc API key", "MCP server tools command", "MCP server tools arguments", "System prompt file"] {
        assert!(errors[0].message.contains(label), "{label} missing in {}", errors[0].message);
    }

    let planned =
        plan_all(&[Operation::DeleteEnvVar { name: "PHX_PLAN_BYSTANDER".into() }], &settings, &dir).unwrap();
    assert_eq!(env_names(&planned.settings), vec!["PHX_PLAN_USED"]);
    let errors =
        plan_all(&[Operation::DeleteEnvVar { name: "PHX_PLAN_MISSING".into() }], &settings, &dir).unwrap_err();
    assert_eq!(errors[0].key.as_deref(), Some("env_vars[PHX_PLAN_MISSING]"));
}

#[test]
fn delete_env_var_passes_after_a_row_reassigns_the_only_reference() {
    let (mut settings, dir) = (fixture_settings(), config_dir());
    settings.env_vars = vec![env_var("PHX_PLAN_GONE")];
    settings.models[0].api_key = Some("${PHX_PLAN_GONE}".into());
    let ops = vec![
        Operation::UpdateModel { id: "abc".into(), model: model("abc", ModelType::Text, "${OPENAI_KEY}") },
        Operation::DeleteEnvVar { name: "PHX_PLAN_GONE".into() },
    ];

    let planned = plan_all(&ops, &settings, &dir).unwrap();

    assert!(planned.settings.env_vars.is_empty());
}

#[test]
fn set_setting_cannot_touch_env_vars() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        set_setting("env_vars", json!([])),
        set_setting("env_vars[PHX_PLAN_X].description", json!("d")),
    ];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    assert_eq!(errors.iter().map(|e| e.row).collect::<Vec<_>>(), vec![0, 1]);
}

#[test]
fn set_setting_changes_one_field_of_a_model() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let planned = plan_all(&[set_setting("models[abc].store", json!(false))], &settings, &dir).unwrap();

    assert_eq!(effects_of(&planned).len(), 1);
    let effect = setting_effect(&planned, "models[abc].store");
    assert_eq!((effect.before.clone(), effect.after.clone()), (Some(json!(true)), Some(json!(false))));
    assert!(!planned.settings.models[0].store);
    assert!(planned.settings.models[1].store);
}

#[test]
fn set_setting_row_on_a_list_item_is_labelled_with_the_item_name() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        set_setting("models[abc].store", json!(false)),
        set_setting("webview_providers[claude].url", json!("https://claude.ai/new")),
    ];

    let planned = plan_all(&ops, &settings, &dir).unwrap();

    let labels: Vec<&str> = planned.rows.iter().map(|row| row.operation.label.as_str()).collect();
    assert_eq!(labels, vec!["Model abc: Store", "Provider claude: Provider URL"]);
}

#[test]
fn list_item_label_falls_back_to_the_id_of_an_unknown_item() {
    let settings = fixture_settings();

    assert_eq!(set_setting("models[nope].store", json!(false)).label(&settings), "nope: Store");
    assert_eq!(Operation::DeleteWebviewProvider { id: "nope".into() }.label(&settings), "Delete web provider nope");
}

#[test]
fn set_setting_on_a_missing_model_or_web_provider_fails_with_not_found() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        set_setting("models[nope].store", json!(false)),
        set_setting("webview_providers[nope].name", json!("N")),
    ];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    assert_eq!(errors[0].key.as_deref(), Some("models[nope].store"));
    assert!(errors[0].message.contains("model 'nope' not found"), "{}", errors[0].message);
    assert!(errors[1].message.contains("web provider 'nope' not found"), "{}", errors[1].message);
}

#[test]
fn set_setting_model_api_key_takes_only_an_env_reference() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let literal = plan_all(&[set_setting("models[abc].api_key", json!("sk-x"))], &settings, &dir).unwrap_err();
    let number = plan_all(&[set_setting("models[abc].api_key", json!(5))], &settings, &dir).unwrap_err();

    assert!(literal[0].message.contains("secrets only through the secret element"), "{}", literal[0].message);
    assert_eq!(number[0].key.as_deref(), Some("models[abc].api_key"));

    let planned = plan_all(&[set_setting("models[abc].api_key", json!("${NEW_KEY}"))], &settings, &dir).unwrap();
    assert_eq!(planned.settings.models[0].api_key.as_deref(), Some("${NEW_KEY}"));
}

#[test]
fn set_setting_refuses_list_item_ids_and_other_managed_lists() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        set_setting("models[abc].id", json!("renamed")),
        set_setting("webview_providers[claude].id", json!("renamed")),
        set_setting("mcp_servers.tools.command", json!("other")),
        set_setting("keymaps[x].bindings", json!({})),
    ];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    assert_eq!(errors.iter().map(|e| e.row).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    assert!(errors.iter().all(|e| e.message.contains("not a settable catalog key")));
}

#[test]
fn set_setting_changes_the_name_of_a_web_provider() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let planned =
        plan_all(&[set_setting("webview_providers[claude].name", json!("Claude web"))], &settings, &dir).unwrap();

    assert_eq!(planned.settings.webview_providers[0].name, "Claude web");
    setting_effect(&planned, "webview_providers[claude].name");
}

#[test]
fn add_then_update_env_var_leaves_the_new_description_and_flag() {
    let (settings, dir) = (fixture_settings(), config_dir());
    let ops = vec![
        Operation::AddEnvVar { name: "PHX_PLAN_ADDED".into(), description: "first".into(), secret: false },
        Operation::UpdateEnvVar { name: "PHX_PLAN_ADDED".into(), description: "second".into(), secret: true },
    ];

    let planned = plan_all(&ops, &settings, &dir).unwrap();

    assert_eq!(
        planned.settings.env_vars,
        vec![EnvVar {
            name: "PHX_PLAN_ADDED".into(),
            description: "second".into(),
            secret: true,
            renamed_from: vec![],
        }]
    );
}

#[test]
fn add_env_var_refuses_an_existing_or_invalid_name_and_update_an_unknown_one() {
    let (mut settings, dir) = (fixture_settings(), config_dir());
    settings.env_vars = vec![env_var("PHX_PLAN_PRESENT")];
    let ops = vec![
        Operation::AddEnvVar { name: "PHX_PLAN_PRESENT".into(), description: String::new(), secret: false },
        Operation::AddEnvVar { name: "has space".into(), description: String::new(), secret: false },
        Operation::UpdateEnvVar { name: "PHX_PLAN_MISSING".into(), description: String::new(), secret: false },
    ];

    let errors = plan_all(&ops, &settings, &dir).unwrap_err();

    let keys: Vec<_> = errors.iter().map(|e| e.key.as_deref()).collect();
    assert_eq!(
        keys,
        vec![
            Some("env_vars[PHX_PLAN_PRESENT]"),
            Some("env_vars[has space].name"),
            Some("env_vars[PHX_PLAN_MISSING]"),
        ]
    );
}

#[test]
fn delete_model_of_an_unknown_id_lists_the_existing_ids() {
    let (settings, dir) = (fixture_settings(), config_dir());

    let errors = plan_all(&[Operation::DeleteModel { id: "nope".into() }], &settings, &dir).unwrap_err();

    assert_eq!(errors[0].message, "model 'nope' not found; models: abc, def, stt1");
}

#[test]
fn set_env_value_needs_a_listed_variable_and_plans_only_a_secret_effect() {
    let (mut settings, dir) = (fixture_settings(), config_dir());
    let set_value = || Operation::SetEnvValue { name: "PHX_PLAN_VALUE".into() };

    let errors = plan_all(&[set_value()], &settings, &dir).unwrap_err();

    assert_eq!(errors[0].key.as_deref(), Some("env_vars[PHX_PLAN_VALUE]"));
    assert!(errors[0].message.contains("variables:"), "{}", errors[0].message);

    let add = Operation::AddEnvVar { name: "PHX_PLAN_VALUE".into(), description: String::new(), secret: true };
    let planned = plan_all(&[add, set_value()], &settings, &dir).unwrap();
    let row = &planned.rows[1].operation;
    assert_eq!((row.op.as_str(), row.label.as_str()), ("set_env_value", "Set value of PHX_PLAN_VALUE"));
    assert_eq!(
        row.effects,
        vec![Effect { target: Target::Secret { name: "PHX_PLAN_VALUE".into() }, before: None, after: None }]
    );

    settings.env_vars = vec![env_var("PHX_PLAN_VALUE")];
    let listed = plan_all(&[set_value()], &settings, &dir).unwrap();
    let current = Current { settings: &settings, config_dir: dir.path() };
    assert!(stale_rows(&listed, &current).is_empty());
}

#[test]
fn update_env_var_switching_secret_off_says_the_value_becomes_visible_to_the_agent() {
    let (mut settings, dir) = (fixture_settings(), config_dir());
    settings.env_vars = vec![env_var("PHX_PLAN_VALUE")];
    settings.env_vars[0].secret = true;
    let update = |secret: bool| Operation::UpdateEnvVar {
        name: "PHX_PLAN_VALUE".into(),
        description: String::new(),
        secret,
    };

    let off = plan_all(&[update(false)], &settings, &dir).unwrap();
    let on = plan_all(&[update(true)], &settings, &dir).unwrap();

    assert_eq!(off.rows[0].operation.label, "Update variable PHX_PLAN_VALUE (value becomes visible to the agent)");
    assert_eq!(on.rows[0].operation.label, "Update variable PHX_PLAN_VALUE");
}

#[test]
fn update_env_var_of_a_variable_that_is_already_plain_keeps_the_plain_label() {
    let (mut settings, dir) = (fixture_settings(), config_dir());
    settings.env_vars = vec![env_var("PHX_PLAN_VALUE")];

    let planned = plan_all(
        &[Operation::UpdateEnvVar { name: "PHX_PLAN_VALUE".into(), description: "d".into(), secret: false }],
        &settings,
        &dir,
    )
    .unwrap();

    assert_eq!(planned.rows[0].operation.label, "Update variable PHX_PLAN_VALUE");
}
