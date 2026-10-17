//! Manang desktop shell.
//!
//! AGENTS.md: all compute-heavy work lives in the Rust core, and the frontend
//! is a thin presentation layer. This crate owns only the IPC surface and the
//! application lifecycle; the document engine arrives as sibling crates in
//! `crates/`.

use std::sync::Arc;

use manang_editor::{Editor, EditorPlugin};
use manang_kernel::{CommandInfo, Kernel, KernelError, Payload};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

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

/// Build the kernel with every first-party plugin registered.
///
/// Everything in Manang is a plugin; the shell only dispatches commands.
fn build_kernel() -> Result<Kernel, KernelError> {
    let kernel = Kernel::new();
    let editor = Arc::new(Editor::new());
    kernel.register(
        manang_editor::manifest()?,
        Arc::new(EditorPlugin::new(editor)),
    )?;
    Ok(kernel)
}

/// Commands for the palette.
#[tauri::command]
fn kernel_commands(kernel: State<'_, Kernel>) -> Vec<CommandInfo> {
    kernel.commands()
}

/// Run a command whose result is structured data.
#[tauri::command]
fn kernel_invoke(
    kernel: State<'_, Kernel>,
    command: String,
    args: Option<Value>,
) -> Result<Value, IpcError> {
    let input = args.map_or(Payload::Empty, Payload::Json);
    match kernel.invoke(&command, input)? {
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
fn kernel_invoke_binary(
    kernel: State<'_, Kernel>,
    command: String,
    args: Option<Value>,
) -> Result<tauri::ipc::Response, IpcError> {
    let input = args.map_or(Payload::Empty, Payload::Json);
    match kernel.invoke(&command, input)? {
        Payload::Binary(bytes) => Ok(tauri::ipc::Response::new(bytes.to_vec())),
        _ => Err(IpcError {
            message: format!("`{command}` does not return binary"),
        }),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    let kernel = build_kernel().map_err(|e| tauri::Error::Anyhow(e.into()))?;
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(kernel)
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
    use super::{AppInfo, app_info, build_kernel};
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
        let kernel = build_kernel().expect("first-party plugins register");

        assert!(kernel.commands().iter().any(|c| c.id == "editor.new"));
        let out = kernel
            .invoke("editor.new", Payload::Empty)
            .expect("editor.new runs");
        assert_eq!(out, Payload::Json(json!({ "id": 1 })));
    }
}
