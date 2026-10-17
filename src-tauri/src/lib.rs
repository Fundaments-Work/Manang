//! Manang desktop shell.
//!
//! AGENTS.md: all compute-heavy work lives in the Rust core, and the frontend
//! is a thin presentation layer. This crate owns only the IPC surface and the
//! application lifecycle; the document engine arrives as sibling crates in
//! `crates/`.

use serde::Serialize;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::{AppInfo, app_info};

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
}
