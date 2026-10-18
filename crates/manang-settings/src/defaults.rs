//! Settings the shell itself owns, pre-registered under the owner `core`.

use serde_json::{Value, json};

use crate::registry::{Scope, SettingDef, SettingType};

fn choice(values: &[&str]) -> SettingType {
    SettingType::Enum {
        values: values.iter().map(|v| (*v).to_owned()).collect(),
    }
}

fn int(min: i64, max: i64) -> SettingType {
    SettingType::Int {
        min: Some(min),
        max: Some(max),
    }
}

fn float(min: f64, max: f64) -> SettingType {
    SettingType::Float {
        min: Some(min),
        max: Some(max),
    }
}

/// The contexts each core category belongs to: where a contextual Settings
/// page shows it. Opened for the whole application (`app`) the page shows
/// every group; opened from a document it shows the `document` groups, and
/// so on. A category missing here belongs to `app` only.
pub const CATEGORY_CONTEXTS: &[(&str, &[&str])] = &[
    ("appearance", &["app"]),
    ("editor", &["document", "diagram"]),
    ("preview", &["document", "pdf"]),
    ("compile", &["document"]),
    ("files", &["app", "project"]),
];

/// The contexts of a core category (`["app"]` when it declares none).
pub fn category_contexts(category: &str) -> &'static [&'static str] {
    CATEGORY_CONTEXTS
        .iter()
        .find(|(name, _)| *name == category)
        .map_or(&["app"], |(_, contexts)| contexts)
}

fn def(id: &str, ty: SettingType, default: Value, scope: Scope, description: &str) -> SettingDef {
    let category = id.split('.').next().unwrap_or_default();
    SettingDef::new(id, ty, default, scope, false, description)
        .with_contexts(category_contexts(category))
}

/// The core definitions, in no particular order.
pub fn definitions() -> Vec<SettingDef> {
    use Scope::{Project, User};
    vec![
        def(
            "appearance.theme",
            choice(&["dark", "light", "system"]),
            json!("dark"),
            User,
            "Colour theme. `system` follows the operating system.",
        ),
        def(
            "appearance.custom-theme",
            SettingType::String,
            json!(""),
            User,
            "Id of a user theme from the themes folder to use instead of the built-in one. Empty for none.",
        ),
        def(
            "appearance.ui-scale",
            float(0.75, 2.0),
            json!(1.0),
            User,
            "Scale factor for the whole interface.",
        ),
        def(
            "appearance.reduced-motion",
            choice(&["system", "on", "off"]),
            json!("system"),
            User,
            "Replace transitions with instant changes. `system` follows the operating system.",
        ),
        def(
            "editor.font-size",
            int(8, 48),
            json!(14),
            Project,
            "Editor font size in pixels.",
        ),
        def(
            "editor.line-height",
            float(1.0, 3.0),
            json!(1.6),
            Project,
            "Editor line height as a multiple of the font size.",
        ),
        def(
            "editor.tab-size",
            int(1, 16),
            json!(2),
            Project,
            "Number of columns a tab occupies.",
        ),
        def(
            "editor.word-wrap",
            SettingType::Bool,
            json!(true),
            Project,
            "Wrap long lines at the editor width.",
        ),
        def(
            "editor.line-numbers",
            SettingType::Bool,
            json!(true),
            Project,
            "Show line numbers in the gutter.",
        ),
        def(
            "editor.vim-mode",
            SettingType::Bool,
            json!(false),
            User,
            "Use Vim-style modal editing. Off keeps the editor modeless.",
        ),
        def(
            "preview.auto-compile",
            SettingType::Bool,
            json!(true),
            Project,
            "Recompile the preview automatically while typing.",
        ),
        def(
            "preview.debounce-ms",
            int(0, 5000),
            json!(300),
            Project,
            "Milliseconds to wait after the last keystroke before recompiling.",
        ),
        def(
            "preview.double-click-source",
            SettingType::Bool,
            json!(true),
            User,
            "Double-click in the PDF goes to the source. Alt+double-click then selects a word.",
        ),
        def(
            "preview.dark-pages",
            SettingType::Bool,
            json!(false),
            User,
            "Show PDF pages inverted for reading in the dark. Export is unaffected.",
        ),
        def(
            "compile.engine",
            choice(&["typst", "latex", "markdown"]),
            json!("typst"),
            Project,
            "Default compile engine for new projects.",
        ),
        def(
            "files.autosave",
            choice(&["off", "after-delay", "on-focus-loss"]),
            json!("after-delay"),
            User,
            "When modified documents are saved automatically.",
        ),
        def(
            "files.trash-on-delete",
            SettingType::Bool,
            json!(true),
            User,
            "Move deleted files to the trash instead of removing them permanently.",
        ),
    ]
}
