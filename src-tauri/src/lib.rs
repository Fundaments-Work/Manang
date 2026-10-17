//! Manang desktop shell.
//!
//! AGENTS.md: all compute-heavy work lives in the Rust core, and the frontend
//! is a thin presentation layer. This crate owns only the IPC surface and the
//! application lifecycle; the document engine arrives as sibling crates in
//! `crates/`.

use std::sync::Arc;

use manang_editor::{Editor, EditorPlugin};
use manang_kernel::{CommandInfo, EventBus, Kernel, KernelError, Payload};
use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, State};

/// Build identity, surfaced to the shell for the about dialogue.
///
/// Returned as plain data rather than a greeting string so the frontend never
/// parses strings out of the IPC boundary.
#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub target: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Manang",
        version: env!("CARGO_PKG_VERSION"),
        target: "typst",
    }
}

/// Structured failure for the shell; it never parses error strings.
#[derive(Debug, Serialize)]
pub struct IpcError {
    pub message: String,
}

impl From<KernelError> for IpcError {
    fn from(error: KernelError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

/// Why the shell could not start.
#[derive(Debug, thiserror::Error)]
enum BootError {
    #[error(transparent)]
    Kernel(#[from] KernelError),
    #[error(transparent)]
    Typst(#[from] manang_typst::TypstError),
    #[error(transparent)]
    Manifest(#[from] manang_kernel::ManifestError),
}

/// Build the kernel with every first-party plugin registered.
///
/// Everything in Manang is a plugin; the shell only dispatches commands.
fn build_kernel(config_dir: &std::path::Path, events: &EventBus) -> Result<Kernel, BootError> {
    let kernel = Kernel::new();
    kernel.register(
        manang_editor::manifest()?,
        Arc::new(EditorPlugin::new(Arc::new(Editor::new()))),
    )?;
    kernel.register(
        manang_typst::manifest()?,
        Arc::new(manang_typst::plugin(
            manang_typst::CompilerConfig::default(),
        )?),
    )?;
    kernel.register(
        manang_project::manifest()?,
        Arc::new(manang_project::plugin(
            config_dir.to_path_buf(),
            events.clone(),
        )),
    )?;
    Ok(kernel)
}

/// Commands for the palette.
#[tauri::command]
fn kernel_commands(kernel: State<'_, Arc<Kernel>>) -> Vec<CommandInfo> {
    kernel.commands()
}

/// Run a plugin command off the async runtime. Plugin calls are synchronous
/// and may be CPU-bound, so they never occupy a Tokio worker.
async fn dispatch(
    kernel: &Arc<Kernel>,
    command: String,
    args: Option<Value>,
) -> Result<Payload, IpcError> {
    let kernel = Arc::clone(kernel);
    let input = args.map_or(Payload::Empty, Payload::Json);
    tauri::async_runtime::spawn_blocking(move || kernel.invoke(&command, input))
        .await
        .map_err(|e| IpcError {
            message: format!("command task failed: {e}"),
        })?
        .map_err(IpcError::from)
}

/// Run a command whose result is structured data.
#[tauri::command]
async fn kernel_invoke(
    kernel: State<'_, Arc<Kernel>>,
    command: String,
    args: Option<Value>,
) -> Result<Value, IpcError> {
    match dispatch(&kernel, command.clone(), args).await? {
        Payload::Json(value) => Ok(value),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(IpcError {
            message: format!("`{command}` returns binary; use kernel_invoke_binary"),
        }),
    }
}

/// Run a command whose result is a binary artefact (PDF, SVG). The bytes cross
/// IPC raw, never as base64 or JSON.
#[tauri::command]
async fn kernel_invoke_binary(
    kernel: State<'_, Arc<Kernel>>,
    command: String,
    args: Option<Value>,
) -> Result<tauri::ipc::Response, IpcError> {
    match dispatch(&kernel, command.clone(), args).await? {
        Payload::Binary(bytes) => Ok(tauri::ipc::Response::new(bytes.to_vec())),
        _ => Err(IpcError {
            message: format!("`{command}` does not return binary"),
        }),
    }
}

/// Forward kernel events (file changes, ...) to the shell as `kernel://event`.
fn forward_events(app: tauri::AppHandle, events: &EventBus) -> std::io::Result<()> {
    let subscription = events.subscribe("*");
    std::thread::Builder::new()
        .name("manang-events".to_owned())
        .spawn(move || {
            while let Some(event) = subscription.recv() {
                let payload = match event.payload {
                    Payload::Json(value) => value,
                    _ => Value::Null,
                };
                // A closed window is not an error worth stopping the pump for.
                let _ = app.emit(
                    "kernel://event",
                    serde_json::json!({ "topic": event.topic, "payload": payload }),
                );
            }
        })
        .map(|_| ())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;
            let config_dir = app.path().app_config_dir()?;
            let events = EventBus::new();
            let kernel = build_kernel(&config_dir, &events)?;
            forward_events(app.handle().clone(), &events)?;
            app.manage(Arc::new(kernel));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            kernel_commands,
            kernel_invoke,
            kernel_invoke_binary
        ])
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::{AppInfo, EventBus, app_info, build_kernel};
    use manang_kernel::Payload;
    use serde_json::json;

    #[test]
    fn app_info_reports_build_identity() {
        let info: AppInfo = app_info();

        assert_eq!(info.name, "Manang");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.target, "typst");
    }

    #[test]
    fn app_info_serialises_as_structured_data() {
        // The shell must never parse strings out of the IPC boundary.
        let json = serde_json::to_value(app_info()).expect("app_info serialises");

        assert_eq!(json["name"], "Manang");
        assert!(json["version"].is_string());
        assert!(json["target"].is_string());
    }

    #[test]
    fn kernel_boots_with_the_editor_plugin() {
        let kernel = build_kernel(&std::env::temp_dir(), &EventBus::new())
            .expect("first-party plugins register");

        assert!(kernel.commands().iter().any(|c| c.id == "editor.new"));
        let out = kernel
            .invoke("editor.new", Payload::Empty)
            .expect("editor.new runs");
        assert_eq!(out, Payload::Json(json!({ "id": 1 })));
    }
}
