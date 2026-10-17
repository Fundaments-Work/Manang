//! word-count: a reference Manang plugin.
//!
//! * Command `word-count.count`: input is a JSON string, `{"text": "..."}`, or
//!   empty (then the active buffer is read, which needs the `read:active`
//!   permission). Returns `{"words": n, "characters": n, "lines": n}`.
//! * Hook `on_lint`: same input; returns `{"plugin": "word-count",
//!   "diagnostics": [{"severity": "info", "message": "..."}]}`.

wit_bindgen::generate!({
    path: "wit",
    world: "plugin-buffer",
});

use exports::manang::plugin::handler::Guest;
use manang::plugin::buffer;
use manang::plugin::types::{Failure, Hook, Payload};
use serde_json::{Value, json};

/// Totals for one text.
struct Counts {
    words: usize,
    characters: usize,
    lines: usize,
}

/// Count whitespace-separated words, Unicode scalar values and lines.
fn count(text: &str) -> Counts {
    Counts {
        words: text.split_whitespace().count(),
        characters: text.chars().count(),
        lines: text.lines().count(),
    }
}

fn fail(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
    }
}

/// The text to analyse: from the payload if it carries any, else the buffer.
fn text_of(input: Payload) -> Result<String, Failure> {
    match input {
        Payload::Empty => buffer::read_active(),
        Payload::Binary(_) => Err(fail("expected text or JSON, got binary data")),
        Payload::Json(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::String(text)) => Ok(text),
            Ok(Value::Object(map)) => match map.get("text") {
                Some(Value::String(text)) => Ok(text.clone()),
                _ => buffer::read_active(),
            },
            Ok(Value::Null) => buffer::read_active(),
            Ok(_) => Err(fail("expected a string or an object with `text`")),
            Err(e) => Err(fail(format!("input is not valid JSON: {e}"))),
        },
    }
}

struct WordCount;

impl Guest for WordCount {
    fn invoke(command: String, input: Payload) -> Result<Payload, Failure> {
        match command.as_str() {
            "word-count.count" => {
                let c = count(&text_of(input)?);
                Ok(Payload::Json(
                    json!({"words": c.words, "characters": c.characters, "lines": c.lines})
                        .to_string(),
                ))
            }
            other => Err(fail(format!("unknown command `{other}`"))),
        }
    }

    fn on_hook(hook: Hook, input: Payload) -> Result<Payload, Failure> {
        match hook {
            Hook::OnLint => {
                let c = count(&text_of(input)?);
                let message = format!(
                    "{} words, {} characters, {} lines",
                    c.words, c.characters, c.lines
                );
                Ok(Payload::Json(
                    json!({
                        "plugin": "word-count",
                        "diagnostics": [{"severity": "info", "message": message}],
                    })
                    .to_string(),
                ))
            }
            _ => Ok(input),
        }
    }
}

export!(WordCount);
