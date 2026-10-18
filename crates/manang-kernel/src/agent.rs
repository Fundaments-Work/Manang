//! Agent tools: plugin commands declared for AI agents and other plugins
//! (ADR 0012, "Agent tools").
//!
//! A plugin subscribes to [`Hook::RegisterAgentTool`] and answers it with
//! `{ "tools": [AgentTool, ...] }`. Each tool names one of the plugin's own
//! commands and describes its input and output with JSON Schema (draft
//! 2020-12). The shape is MCP's `Tool` plus the command to run, so a shell
//! can hand the list to an MCP client or a model API unchanged.
//!
//! [`Kernel::agent_tools`] collects the tools; [`Kernel::call_agent_tool`]
//! validates the input against the declared schema before running the
//! command, so an agent gets a precise, structured complaint instead of a
//! plugin's best guess.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::error::KernelError;
use crate::kernel::Kernel;
use crate::manifest::Hook;
use crate::payload::Payload;

/// One tool, as a plugin declares it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTool {
    /// Model-facing name: `^[a-zA-Z0-9_-]{1,64}$`, unique across plugins.
    pub name: String,
    /// Short human title.
    pub title: String,
    /// What the tool does and when to use it, written for a model.
    pub description: String,
    /// JSON Schema of the input (an object schema).
    pub input_schema: Value,
    /// JSON Schema of the output.
    pub output_schema: Value,
    /// The `<plugin>.<command>` that runs it.
    pub command: String,
}

/// A tool together with the plugin that declared it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegisteredTool {
    /// Owning plugin.
    pub plugin: String,
    /// The declaration.
    #[serde(flatten)]
    pub tool: AgentTool,
}

/// Every registered tool, and why any declaration was left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AgentToolList {
    /// Valid tools, sorted by name.
    pub tools: Vec<RegisteredTool>,
    /// One line per rejected declaration or failing plugin.
    pub problems: Vec<String>,
}

/// One way an input does not match a schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchemaIssue {
    /// JSON pointer to the offending value (`""` is the whole input).
    pub path: String,
    /// What is wrong.
    pub message: String,
}

/// Why an agent tool call failed.
#[derive(Debug, Error)]
pub enum AgentToolError {
    /// No tool has this name.
    #[error("no agent tool named `{0}`")]
    UnknownTool(String),
    /// The input does not match the tool's schema.
    #[error("invalid input for `{tool}`: {}", describe(issues))]
    InvalidInput {
        /// The tool.
        tool: String,
        /// Every problem found.
        issues: Vec<SchemaIssue>,
    },
    /// The command failed.
    #[error(transparent)]
    Kernel(#[from] KernelError),
}

/// The issues as one line: `path: message; path: message`.
pub fn describe(issues: &[SchemaIssue]) -> String {
    issues
        .iter()
        .map(|i| {
            let at = if i.path.is_empty() { "input" } else { &i.path };
            format!("{at}: {}", i.message)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn valid_tool_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn check_tool(kernel: &Kernel, plugin: &str, tool: &AgentTool) -> Result<(), String> {
    if !valid_tool_name(&tool.name) {
        return Err(format!(
            "tool name `{}` must match ^[a-zA-Z0-9_-]{{1,64}}$",
            tool.name
        ));
    }
    if !tool.command.starts_with(&format!("{plugin}.")) {
        return Err(format!(
            "tool `{}` runs `{}`, which is not one of `{plugin}`'s commands",
            tool.name, tool.command
        ));
    }
    if !kernel.has_command(&tool.command) {
        return Err(format!(
            "tool `{}` runs `{}`, which is not registered",
            tool.name, tool.command
        ));
    }
    if tool.description.trim().is_empty() {
        return Err(format!("tool `{}` needs a description", tool.name));
    }
    if tool.input_schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(format!(
            "tool `{}`: input_schema must be an object schema",
            tool.name
        ));
    }
    if !tool.output_schema.is_object() {
        return Err(format!(
            "tool `{}`: output_schema must be a schema",
            tool.name
        ));
    }
    Ok(())
}

impl Kernel {
    /// Collects the tools every subscriber of
    /// [`Hook::RegisterAgentTool`] declares. A plugin that fails or answers
    /// nonsense is reported in `problems`; the others are still listed.
    pub fn agent_tools(&self) -> AgentToolList {
        let mut out = AgentToolList::default();
        for (plugin, result) in self.run_hook_each(Hook::RegisterAgentTool, Payload::Empty) {
            let value = match result {
                Ok(Payload::Json(v)) => v,
                Ok(_) => {
                    out.problems.push(format!(
                        "`{plugin}` answered the agent tool hook without JSON"
                    ));
                    continue;
                }
                Err(e) => {
                    out.problems.push(format!("`{plugin}`: {e}"));
                    continue;
                }
            };
            let Some(items) = value.get("tools").and_then(Value::as_array) else {
                out.problems
                    .push(format!("`{plugin}` must answer {{ \"tools\": [...] }}"));
                continue;
            };
            for item in items {
                match serde_json::from_value::<AgentTool>(item.clone()) {
                    Ok(tool) => match check_tool(self, &plugin, &tool) {
                        Ok(()) => out.tools.push(RegisteredTool {
                            plugin: plugin.clone(),
                            tool,
                        }),
                        Err(problem) => out.problems.push(format!("`{plugin}`: {problem}")),
                    },
                    Err(e) => out
                        .problems
                        .push(format!("`{plugin}`: a tool declaration is malformed: {e}")),
                }
            }
        }
        out.tools.sort_by(|a, b| a.tool.name.cmp(&b.tool.name));
        // A name declared twice is ambiguous: keep neither.
        let mut i = 0;
        while i + 1 < out.tools.len() {
            if out.tools[i].tool.name == out.tools[i + 1].tool.name {
                let name = out.tools[i].tool.name.clone();
                out.problems
                    .push(format!("tool name `{name}` is declared more than once"));
                out.tools.retain(|t| t.tool.name != name);
            } else {
                i += 1;
            }
        }
        out
    }

    /// Runs the tool `name` with `input` after validating it against the
    /// tool's input schema.
    pub fn call_agent_tool(&self, name: &str, input: Value) -> Result<Payload, AgentToolError> {
        let list = self.agent_tools();
        let tool = list
            .tools
            .into_iter()
            .find(|t| t.tool.name == name)
            .ok_or_else(|| AgentToolError::UnknownTool(name.to_owned()))?;
        let issues = validate(&tool.tool.input_schema, &input);
        if !issues.is_empty() {
            return Err(AgentToolError::InvalidInput {
                tool: name.to_owned(),
                issues,
            });
        }
        Ok(self.invoke(&tool.tool.command, Payload::Json(input))?)
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "integer" => value
            .as_f64()
            .is_some_and(|f| f.fract() == 0.0 && f.is_finite()),
        _ => true,
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Validates `value` against `schema`, supporting the keywords tool
/// schemas use: `type` (string or list), `properties`, `required`,
/// `additionalProperties` (bool or schema), `enum`, `const`, `items`,
/// `minItems`, `maxItems`, `minimum`, `maximum`, `minLength`, `maxLength`,
/// `oneOf`, `anyOf` and local `$ref` to `#/$defs/<name>`. Unknown keywords
/// are ignored, as JSON Schema requires. Returns every problem found.
pub fn validate(schema: &Value, value: &Value) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    check(schema, schema, value, "", &mut issues, 0);
    issues
}

fn issue(issues: &mut Vec<SchemaIssue>, path: &str, message: String) {
    issues.push(SchemaIssue {
        path: path.to_owned(),
        message,
    });
}

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    match schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| r.strip_prefix("#/$defs/"))
    {
        Some(name) => root
            .get("$defs")
            .and_then(|d| d.get(name))
            .unwrap_or(&Value::Bool(true)),
        None => schema,
    }
}

fn check(
    root: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    issues: &mut Vec<SchemaIssue>,
    depth: usize,
) {
    if depth > 32 {
        issue(issues, path, "schema nests too deeply".to_owned());
        return;
    }
    let schema = resolve(root, schema);
    match schema {
        Value::Bool(true) => return,
        Value::Bool(false) => {
            issue(issues, path, "no value is allowed here".to_owned());
            return;
        }
        Value::Object(_) => {}
        _ => return,
    }
    if let Some(t) = schema.get("type") {
        let allowed: Vec<&str> = match t {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !allowed.is_empty() && !allowed.iter().any(|t| type_matches(t, value)) {
            issue(
                issues,
                path,
                format!("expected {}, got {}", allowed.join(" or "), kind_of(value)),
            );
            return;
        }
    }
    if let Some(Value::Array(options)) = schema.get("enum")
        && !options.contains(value)
    {
        let list: Vec<String> = options.iter().map(Value::to_string).collect();
        issue(
            issues,
            path,
            format!("{value} is not one of {}", list.join(", ")),
        );
    }
    if let Some(c) = schema.get("const")
        && c != value
    {
        issue(issues, path, format!("must be {c}"));
    }
    if let Some(n) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
            && n < min
        {
            issue(issues, path, format!("{n} is below the minimum {min}"));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
            && n > max
        {
            issue(issues, path, format!("{n} is above the maximum {max}"));
        }
    }
    if let Some(s) = value.as_str() {
        let len = s.chars().count() as u64;
        if let Some(min) = schema.get("minLength").and_then(Value::as_u64)
            && len < min
        {
            issue(issues, path, format!("must be at least {min} characters"));
        }
        if let Some(max) = schema.get("maxLength").and_then(Value::as_u64)
            && len > max
        {
            issue(issues, path, format!("must be at most {max} characters"));
        }
    }
    if let Some(items) = value.as_array() {
        let len = items.len() as u64;
        if let Some(min) = schema.get("minItems").and_then(Value::as_u64)
            && len < min
        {
            issue(issues, path, format!("needs at least {min} items"));
        }
        if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
            && len > max
        {
            issue(issues, path, format!("takes at most {max} items"));
        }
        if let Some(item_schema) = schema.get("items") {
            for (i, item) in items.iter().enumerate() {
                check(
                    root,
                    item_schema,
                    item,
                    &format!("{path}/{i}"),
                    issues,
                    depth + 1,
                );
            }
        }
    }
    if let Some(map) = value.as_object() {
        let props = schema.get("properties").and_then(Value::as_object);
        if let Some(Value::Array(required)) = schema.get("required") {
            for key in required.iter().filter_map(Value::as_str) {
                if !map.contains_key(key) {
                    issue(issues, path, format!("`{key}` is required"));
                }
            }
        }
        for (key, v) in map {
            let child = format!("{path}/{key}");
            match props.and_then(|p| p.get(key)) {
                Some(s) => check(root, s, v, &child, issues, depth + 1),
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        let known: Vec<&str> = props
                            .map(|p| p.keys().map(String::as_str).collect())
                            .unwrap_or_default();
                        issue(
                            issues,
                            &child,
                            format!("unknown field; expected one of {}", known.join(", ")),
                        );
                    }
                    Some(extra @ Value::Object(_)) => {
                        check(root, extra, v, &child, issues, depth + 1);
                    }
                    _ => {}
                },
            }
        }
    }
    if let Some(Value::Array(options)) = schema.get("anyOf") {
        let ok = options
            .iter()
            .any(|o| validate_sub(root, o, value, path, depth).is_empty());
        if !ok {
            issue(issues, path, "matches none of the allowed forms".to_owned());
        }
    }
    if let Some(Value::Array(options)) = schema.get("oneOf") {
        let matching: Vec<Vec<SchemaIssue>> = options
            .iter()
            .map(|o| validate_sub(root, o, value, path, depth))
            .collect();
        let ok = matching.iter().filter(|m| m.is_empty()).count();
        if ok != 1 {
            if ok == 0 {
                // Report the closest form's problems: the one with fewest.
                if let Some(best) = matching.into_iter().min_by_key(Vec::len) {
                    issues.extend(best);
                }
            } else {
                issue(
                    issues,
                    path,
                    "matches more than one allowed form".to_owned(),
                );
            }
        }
    }
}

fn validate_sub(
    root: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    depth: usize,
) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    check(root, schema, value, path, &mut issues, depth + 1);
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "count": { "type": "integer", "minimum": 0, "maximum": 10 },
                "kind": { "enum": ["a", "b"] },
                "items": { "type": "array", "items": { "$ref": "#/$defs/item" }, "maxItems": 2 },
                "either": { "oneOf": [{ "type": "string" }, { "type": "number" }] }
            },
            "required": ["name"],
            "additionalProperties": false,
            "$defs": { "item": { "type": "object", "properties": { "x": { "type": "number" } }, "required": ["x"] } }
        })
    }

    #[test]
    fn accepts_a_valid_input() {
        let ok =
            json!({ "name": "n", "count": 3, "kind": "a", "items": [{ "x": 1 }], "either": 2 });
        assert_eq!(validate(&schema(), &ok), Vec::new());
    }

    #[test]
    fn reports_every_problem_with_a_path() {
        let bad = json!({ "count": 3.5, "kind": "c", "items": [{}, { "x": "1" }, { "x": 2 }], "extra": 1, "either": true });
        let issues = validate(&schema(), &bad);
        let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
        assert!(paths.contains(&""), "{issues:?}");
        assert!(paths.contains(&"/count"));
        assert!(paths.contains(&"/kind"));
        assert!(paths.contains(&"/items"));
        assert!(paths.contains(&"/items/0"));
        assert!(paths.contains(&"/items/1/x"));
        assert!(paths.contains(&"/extra"));
        assert!(paths.contains(&"/either"));
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("`name` is required"))
        );
    }

    #[test]
    fn tool_names_follow_the_model_api_pattern() {
        assert!(valid_tool_name("figure_add_shapes"));
        assert!(!valid_tool_name("figure.add"));
        assert!(!valid_tool_name(""));
        assert!(!valid_tool_name(&"x".repeat(65)));
    }
}
