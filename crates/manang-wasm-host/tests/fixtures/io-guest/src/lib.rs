//! Test plugin for the `fs` and `http` interfaces.

wit_bindgen::generate!({
    path: "../../../wit",
    world: "plugin-full",
});

use exports::manang::plugin::handler::Guest;
use manang::plugin::types::{Failure, Hook, Payload};
use manang::plugin::{fs, http};
use serde_json::{Value, json};

struct Io;

fn fail(message: &str) -> Failure {
    Failure {
        message: message.to_owned(),
    }
}

fn args(input: &Payload) -> Result<Value, Failure> {
    match input {
        Payload::Json(text) => serde_json::from_str(text).map_err(|e| fail(&e.to_string())),
        _ => Err(fail("expected json")),
    }
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Failure> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| fail(&format!("missing `{key}`")))
}

impl Guest for Io {
    fn invoke(command: String, input: Payload) -> Result<Payload, Failure> {
        let a = args(&input)?;
        match command.as_str() {
            "io.read" => fs::read_file(text(&a, "path")?).map(Payload::Binary),
            "io.write" => fs::write_file(text(&a, "path")?, text(&a, "data")?.as_bytes())
                .map(|()| Payload::Empty),
            "io.list" => {
                let entries = fs::list_dir(text(&a, "path")?)?;
                let names: Vec<Value> = entries
                    .iter()
                    .map(|e| json!({"name": e.name, "dir": e.is_dir, "size": e.size}))
                    .collect();
                Ok(Payload::Json(Value::Array(names).to_string()))
            }
            "io.get" => {
                let response = http::fetch(&http::Request {
                    method: "GET".to_owned(),
                    url: text(&a, "url")?.to_owned(),
                    headers: Vec::new(),
                    body: None,
                })?;
                Ok(Payload::Json(
                    json!({
                        "status": response.status,
                        "body": String::from_utf8_lossy(&response.body),
                    })
                    .to_string(),
                ))
            }
            other => Err(fail(&format!("unknown command {other}"))),
        }
    }

    fn on_hook(_hook: Hook, input: Payload) -> Result<Payload, Failure> {
        Ok(input)
    }
}

export!(Io);
