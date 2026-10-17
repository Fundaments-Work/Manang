//! A Manang plugin. Build it with `scripts/build-plugin.sh sdk/rust-plugin-template`.
//!
//! The plugin is stateless between calls: the host instantiates it fresh for
//! every command and hook, so keep nothing in `static`s and use host services
//! (buffer, filesystem) for anything that must persist.

wit_bindgen::generate!({
    path: "wit",
    // `plugin` imports nothing. Pick `plugin-buffer`, `plugin-fs`,
    // `plugin-http` or `plugin-full` to call host services, and grant the
    // matching permissions in manifest.toml.
    world: "plugin",
});

use exports::manang::plugin::handler::Guest;
use manang::plugin::types::{Failure, Hook, Payload};

struct MyPlugin;

fn fail(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
    }
}

impl Guest for MyPlugin {
    fn invoke(command: String, input: Payload) -> Result<Payload, Failure> {
        match command.as_str() {
            "my-plugin.hello" => {
                let name = match input {
                    Payload::Json(text) => serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned)),
                    _ => None,
                }
                .unwrap_or_else(|| "world".to_owned());
                let reply = serde_json::json!({ "greeting": format!("Hello, {name}!") });
                Ok(Payload::Json(reply.to_string()))
            }
            other => Err(fail(format!("unknown command `{other}`"))),
        }
    }

    fn on_hook(_hook: Hook, input: Payload) -> Result<Payload, Failure> {
        // Hooks you do not subscribe to are never called; pass the input
        // through for the ones you do not transform.
        Ok(input)
    }
}

export!(MyPlugin);
