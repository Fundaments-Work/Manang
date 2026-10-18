//! The agent-tool contract: collection through the `register_agent_tool`
//! hook, rejection of bad declarations, schema validation before a call.

use std::sync::Arc;

use manang_kernel::{
    AgentToolError, Context, Hook, Kernel, Manifest, Payload, Plugin, PluginError,
};
use serde_json::{Value, json};

struct Tools {
    answer: Value,
}

impl Plugin for Tools {
    fn invoke(
        &self,
        _: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match (command, input) {
            ("calc.add", Payload::Json(v)) => {
                let a = v["a"].as_f64().unwrap_or_default();
                let b = v["b"].as_f64().unwrap_or_default();
                Ok(Payload::Json(json!({ "sum": a + b })))
            }
            (other, _) => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }

    fn on_hook(&self, _: &Context<'_>, hook: Hook, input: Payload) -> Result<Payload, PluginError> {
        match hook {
            Hook::RegisterAgentTool => Ok(Payload::Json(self.answer.clone())),
            _ => Ok(input),
        }
    }
}

fn manifest(name: &str) -> Manifest {
    Manifest::from_toml(&format!(
        "[plugin]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[contributes]\nhooks = [\"register_agent_tool\"]\n\n[[contributes.commands]]\nid = \"{name}.add\"\ntitle = \"Add\"\n"
    ))
    .expect("manifest")
}

fn add_tool(name: &str, command: &str) -> Value {
    json!({
        "name": name,
        "title": "Add two numbers",
        "description": "Adds a and b.",
        "input_schema": {
            "type": "object",
            "properties": { "a": { "type": "number" }, "b": { "type": "number" } },
            "required": ["a", "b"],
            "additionalProperties": false
        },
        "output_schema": { "type": "object", "properties": { "sum": { "type": "number" } } },
        "command": command
    })
}

fn kernel() -> Kernel {
    let k = Kernel::new();
    k.register(
        manifest("calc"),
        Arc::new(Tools {
            answer: json!({ "tools": [
                add_tool("calc_add", "calc.add"),
                add_tool("bad.name", "calc.add"),
                add_tool("steal", "other.add"),
                add_tool("ghost", "calc.missing"),
                { "name": "broken" }
            ] }),
        }),
    )
    .expect("calc");
    k.register(
        manifest("noisy"),
        Arc::new(Tools {
            answer: json!("not a list"),
        }),
    )
    .expect("noisy");
    k
}

#[test]
fn valid_tools_are_listed_and_bad_ones_reported() {
    let list = kernel().agent_tools();
    let names: Vec<&str> = list.tools.iter().map(|t| t.tool.name.as_str()).collect();
    assert_eq!(names, vec!["calc_add"]);
    assert_eq!(list.tools[0].plugin, "calc");
    assert_eq!(list.problems.len(), 5, "{:#?}", list.problems);
    assert!(list.problems.iter().any(|p| p.contains("bad.name")));
    assert!(list.problems.iter().any(|p| p.contains("other.add")));
    assert!(list.problems.iter().any(|p| p.contains("calc.missing")));
    assert!(list.problems.iter().any(|p| p.contains("noisy")));
    // Serialised flat, MCP-like.
    let v = serde_json::to_value(&list.tools[0]).expect("serialises");
    assert_eq!(v["name"], "calc_add");
    assert_eq!(v["command"], "calc.add");
    assert_eq!(v["plugin"], "calc");
}

#[test]
fn calls_are_validated_against_the_schema() {
    let k = kernel();
    match k.call_agent_tool("calc_add", json!({ "a": 2, "b": 3 })) {
        Ok(Payload::Json(v)) => assert_eq!(v["sum"], 5.0),
        other => panic!("{other:?}"),
    }
    match k.call_agent_tool("calc_add", json!({ "a": "2", "c": 1 })) {
        Err(AgentToolError::InvalidInput { issues, .. }) => {
            let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
            assert!(paths.contains(&"/a") && paths.contains(&"/c") && paths.contains(&""));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        k.call_agent_tool("nope", json!({})),
        Err(AgentToolError::UnknownTool(_))
    ));
}

#[test]
fn a_name_declared_twice_is_dropped() {
    let k = Kernel::new();
    for name in ["one", "two"] {
        let command = format!("{name}.add");
        k.register(
            manifest(name),
            Arc::new(Tools {
                answer: json!({ "tools": [add_tool("same", &command)] }),
            }),
        )
        .expect("registers");
    }
    let list = k.agent_tools();
    assert!(list.tools.is_empty());
    assert!(list.problems.iter().any(|p| p.contains("more than once")));
}
