use std::sync::Arc;

use rmcp::model::{Meta, Tool};
use serde_json::{json, Map, Value};

use super::operations::Operation;
use crate::services::ai::tools::STRICT_META_KEY;

pub const READ: &str = "settings_read";
pub const TEST: &str = "settings_test";
pub const PROPOSE_CHANGES: &str = "settings_propose_changes";
pub const SECRET: &str = "settings_secret";

fn tool(name: &'static str, description: &'static str, schema: Value) -> Tool {
    let Value::Object(schema) = schema else {
        unreachable!("tool schemas are objects");
    };
    Tool::new(name, description, Arc::new(schema))
}

fn strict(tool: Tool) -> Tool {
    tool.with_meta(Meta(Map::from_iter([(STRICT_META_KEY.to_string(), json!(true))])))
}

fn string() -> Value {
    json!({"type": "string"})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn free_object() -> Value {
    json!({"type": "object"})
}

fn object(properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let properties: Map<String, Value> =
        properties.into_iter().map(|(name, schema)| (name.to_string(), schema)).collect();
    json!({"type": "object", "properties": properties, "required": required})
}

fn model_schema() -> Value {
    object(
        vec![
            ("id", string()),
            ("model", string()),
            ("display_name", string()),
            ("type", string_enum(&["text", "stt"])),
            ("provider", string_enum(&["openai", "anthropic", "gemini", "elevenlabs"])),
            ("group", string()),
            ("api_key", string()),
            ("base_url", string()),
            ("parameters", free_object()),
            ("context_window_size", json!({"type": "integer"})),
            ("api_mode", string_enum(&["responses", "completions"])),
            ("capabilities", free_object()),
            ("store", json!({"type": "boolean"})),
        ],
        &["id", "model", "display_name"],
    )
}

fn mcp_config_schema() -> Value {
    object(
        vec![
            ("command", string()),
            ("args", json!({"type": "array", "items": string()})),
            ("env", json!({"type": "object", "additionalProperties": string()})),
            ("timeout_secs", json!({"type": "integer"})),
            ("tool_timeouts", json!({"type": "object", "additionalProperties": {"type": "integer"}})),
            ("disabled", json!({"type": "boolean"})),
            ("disabled_tools", json!({"type": "array", "items": string()})),
        ],
        &["command"],
    )
}

fn web_provider_schema() -> Value {
    object(vec![("id", string()), ("name", string()), ("url", string())], &["id", "name", "url"])
}

fn frontmatter_schema() -> Value {
    object(
        vec![
            ("name", string()),
            ("display_name", string()),
            ("description", string()),
            ("model", string()),
            ("parameters", free_object()),
            ("offered_in", json!({"type": "array", "items": string_enum(&["context_menu", "pill"])})),
        ],
        &["name"],
    )
}

fn branch(op: &str, description: &str, properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut all_properties = vec![("op", string_enum(&[op]))];
    all_properties.extend(properties);
    let mut all_required = vec!["op"];
    all_required.extend_from_slice(required);
    let mut schema = object(all_properties, &all_required);
    schema["description"] = json!(description);
    schema
}

fn skill_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("slug", string()),
        ("frontmatter", frontmatter_schema()),
        ("body", string()),
        ("sample_input", string()),
    ]
}

fn env_var_properties() -> Vec<(&'static str, Value)> {
    vec![("name", string()), ("description", string()), ("secret", json!({"type": "boolean"}))]
}

fn operation_branches() -> Vec<Value> {
    let skill_required = ["slug", "frontmatter", "body", "sample_input"];
    vec![
        branch(
            "set_setting",
            "Sets one setting to a value. Use it for every setting of the catalog, including one field of a model or web provider by its concrete key such as `models[<id>].store`.",
            vec![("key", string()), ("value", json!({}))],
            &["key", "value"],
        ),
        branch(
            "add_model",
            "Adds a new model whose `model.id` is a new UUID (e.g. `123e4567-e89b-12d3-a456-426614174000`). Put an API key only as a `${VAR}` reference.",
            vec![("model", model_schema())],
            &["model"],
        ),
        branch(
            "update_model",
            "Replaces the whole model `id` with `model`, whose `id` stays the same. A field left out falls back to its default; an empty `api_key` keeps the stored one.",
            vec![("id", string()), ("model", model_schema())],
            &["id", "model"],
        ),
        branch(
            "delete_model",
            "Deletes the model `id` and clears every model choice of the settings that pointed to it.",
            vec![("id", string())],
            &["id"],
        ),
        branch(
            "create_skill",
            "Creates a skill whose `slug` does not exist yet. `frontmatter.name` equals the slug, `body` is the prompt, `sample_input` the text the skill is tested with.",
            skill_properties(),
            &skill_required,
        ),
        branch(
            "update_skill",
            "Replaces the frontmatter, body and sample input of the existing skill `slug`.",
            skill_properties(),
            &skill_required,
        ),
        branch("delete_skill", "Deletes the skill `slug` with its files.", vec![("slug", string())], &["slug"]),
        branch(
            "reorder_skills",
            "Sets the display order of the skills to the list of slugs `order`.",
            vec![("order", json!({"type": "array", "items": string()}))],
            &["order"],
        ),
        branch(
            "write_prompt",
            "Replaces the content of one prompt file.",
            vec![
                (
                    "kind",
                    string_enum(&[
                        "system",
                        "about_you",
                        "environment",
                        "input_format",
                        "title_generation",
                        "speech_to_text",
                        "stt_keyterms",
                    ]),
                ),
                ("content", string()),
            ],
            &["kind", "content"],
        ),
        branch(
            "add_mcp_server",
            "Adds an MCP server called `name`, which must not exist yet.",
            vec![("name", string()), ("config", mcp_config_schema())],
            &["name", "config"],
        ),
        branch(
            "update_mcp_server",
            "Replaces the whole configuration of the existing MCP server `name`; an omitted `disabled` or `disabled_tools` keeps the current value.",
            vec![("name", string()), ("config", mcp_config_schema())],
            &["name", "config"],
        ),
        branch("delete_mcp_server", "Deletes the MCP server `name`.", vec![("name", string())], &["name"]),
        branch(
            "add_webview_provider",
            "Adds a web provider; `provider.id` must not exist yet.",
            vec![("provider", web_provider_schema())],
            &["provider"],
        ),
        branch(
            "update_webview_provider",
            "Replaces the whole web provider `id` with `provider`, whose `id` stays the same.",
            vec![("id", string()), ("provider", web_provider_schema())],
            &["id", "provider"],
        ),
        branch(
            "delete_webview_provider",
            "Deletes the web provider `id`.",
            vec![("id", string())],
            &["id"],
        ),
        branch(
            "add_shortcut",
            "Binds the keyboard shortcut `shortcut` to the action `action`, next to the action's other shortcuts.",
            vec![("action", string()), ("shortcut", string())],
            &["action", "shortcut"],
        ),
        branch(
            "remove_shortcut",
            "Unbinds the keyboard shortcut `shortcut` from the action `action`; the action's other shortcuts stay. To change a shortcut, remove the old one and add the new one in the same proposal.",
            vec![("action", string()), ("shortcut", string())],
            &["action", "shortcut"],
        ),
        branch(
            "add_env_var",
            "Lists a new environment variable `name` so that settings can reference it as `${NAME}`; set `secret` to true for keys and tokens. Its value is entered with a `set_env_value` row.",
            env_var_properties(),
            &["name"],
        ),
        branch(
            "update_env_var",
            "Changes the description and secret flag of the listed variable `name`.",
            env_var_properties(),
            &["name"],
        ),
        branch(
            "rename_env_var",
            "Renames the listed variable `name` to `new_name` and rewrites every `${name}` reference.",
            vec![("name", string()), ("new_name", string())],
            &["name", "new_name"],
        ),
        branch(
            "delete_env_var",
            "Removes the listed variable `name`; fails while a setting still references it.",
            vec![("name", string())],
            &["name"],
        ),
        branch(
            "set_env_value",
            "Asks the user for the value of the variable `name` in the card. The user types the value in the card; the operation carries no value. `name` must be listed in `env_vars`; an earlier `add_env_var` row of the same proposal counts.",
            vec![("name", string())],
            &["name"],
        ),
    ]
}

pub fn operation_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"op": string_enum(&Operation::NAMES)},
        "required": ["op"],
        "anyOf": operation_branches()
    })
}

pub fn operation_fields(op: &str) -> Option<(Vec<String>, Vec<String>)> {
    let branch = operation_branches().into_iter().find(|branch| branch["properties"]["op"]["enum"][0] == op)?;
    let names = |value: &Value| -> Vec<String> {
        value.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect()
    };
    let required: Vec<String> = names(&branch["required"]).into_iter().filter(|name| name != "op").collect();
    let optional = branch["properties"]
        .as_object()
        .into_iter()
        .flat_map(|properties| properties.keys())
        .filter(|name| *name != "op" && !required.contains(name))
        .cloned()
        .collect();
    Some((required, optional))
}

fn operations_schema() -> Value {
    json!({
        "type": "array",
        "description": "Operations in execution order. A later operation may reference a model or skill added by an earlier one.",
        "items": operation_schema(),
        "minItems": 1
    })
}

pub fn definitions() -> Vec<Tool> {
    vec![
        strict(tool(
            READ,
            "Reads the live settings and returns them as JSON. `what` picks the part: `values` all settings; `models` the models (api_key shown as a ${VAR} reference or [redacted]); `skills` name, description, model and offered_in per skill; `skill` one skill's frontmatter and body (needs `name`); `prompt` one prompt (needs `name`: system, about_you, environment, input_format, title_generation, speech_to_text, stt_keyterms). `missing_env` lists the ${VAR} references without a value; `env_vars` lists the environment variables with description, secret flag, whether set on this machine, where used, and the `value` of a variable whose secret flag is false. `change_log` lists the earlier changes (cards, secret changes, undos), newest first, with request, reply, state (applied or undone) and the before and after of every setting; narrow it with `since`, `until`, `key`, `op`, `text`, `host` and `limit`, or read one whole entry, file contents included, with `name` set to its id. It never returns a secret value and changes nothing.",
            json!({
                "type": "object",
                "properties": {
                    "what": {"type": "string", "enum": ["values", "models", "skills", "skill", "prompt", "missing_env", "env_vars", "change_log"]},
                    "name": {"type": ["string", "null"], "description": "For `skill` and `prompt`: skill slug or prompt name. For `change_log`: an entry id, which returns that entry whole and ignores the other filters. Otherwise null."},
                    "since": {"type": ["string", "null"], "description": "For `change_log`: RFC 3339 time; only entries created at or after it. Otherwise null."},
                    "until": {"type": ["string", "null"], "description": "For `change_log`: RFC 3339 time; only entries created at or before it. Otherwise null."},
                    "key": {"type": ["string", "null"], "description": "For `change_log`: only entries with a setting effect whose path starts with it, e.g. `models[<id>]`. Otherwise null."},
                    "op": {"type": ["string", "null"], "description": "For `change_log`: only entries with an operation of this type, e.g. `delete_model`. Otherwise null."},
                    "text": {"type": ["string", "null"], "description": "For `change_log`: only entries whose request, reply or an operation label contains it, ignoring case. Otherwise null."},
                    "host": {"type": ["string", "null"], "description": "For `change_log`: only entries of this host. Otherwise null."},
                    "limit": {"type": ["integer", "null"], "description": "For `change_log`: number of entries returned, default 20, maximum 100. Otherwise null."}
                },
                "required": ["what", "name", "since", "until", "key", "op", "text", "host", "limit"],
                "additionalProperties": false
            }),
        )),
        tool(
            TEST,
            "Checks a list of operations without proposing it. It validates the operations in order against the live settings, then runs the test of each model, skill and MCP server row on the settings as they would be after the operations. It returns `rows` with a label and test result per row, or `errors` with the row number and message of every operation that is invalid, in which case nothing was tested. It shows nothing to the user and writes nothing.",
            json!({
                "type": "object",
                "properties": {"operations": operations_schema()},
                "required": ["operations"]
            }),
        ),
        tool(
            PROPOSE_CHANGES,
            "Proposes a list of operations as one card for the user to accept or reject; the card is the only way a change reaches the settings. The operations are validated and tested first, as in `settings_test`. It returns `proposal_id` and `rows` with a label and test result per row, or `errors` with the row number and message of every invalid operation, in which case no card is shown and nothing was proposed. A new proposal replaces the previous one.",
            json!({
                "type": "object",
                "properties": {"operations": operations_schema()},
                "required": ["operations"]
            }),
        ),
        strict(tool(
            SECRET,
            "Shows the user the current value of the listed environment variable `name` as a read-only masked field with a reveal toggle in the conversation. You never see the value. It returns `shown` when the variable has a value, or `not_set` when it has none and nothing is shown. To change a value propose a `set_env_value` row; this tool never changes one.",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Environment variable name, letters, digits and underscores."}
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        )),
    ]
}
