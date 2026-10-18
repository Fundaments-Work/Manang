//! `manifest.toml`: a plugin's identity, permissions and contributions.

use std::collections::HashSet;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::capability::CapabilitySet;
use crate::error::ManifestError;

/// Where a plugin runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// First-party, trusted, compiled into the binary.
    Native,
    /// Third-party, sandboxed in Wasmtime. The safe default.
    #[default]
    Wasm,
}

/// Lifecycle points a plugin can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hook {
    OnCompilePre,
    OnCompilePost,
    OnLint,
    RegisterAgentTool,
}

/// How a hook's subscribers are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    /// Each subscriber's output feeds the next, in registration order.
    Pipeline,
    /// Every subscriber sees the same input; all results are collected.
    FanOut,
}

impl Hook {
    pub fn mode(self) -> HookMode {
        match self {
            Self::OnCompilePre | Self::OnCompilePost => HookMode::Pipeline,
            Self::OnLint | Self::RegisterAgentTool => HookMode::FanOut,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMeta {
    /// Unique id, lowercase kebab-case. Also the command namespace.
    pub name: String,
    pub version: Version,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tier: Tier,
    /// Path to the `.wasm` module, relative to the plugin directory.
    #[serde(default)]
    pub entry: Option<String>,
    /// Where the plugin's section and settings belong in a contextual
    /// Settings page (see [`SETTINGS_CONTEXTS`]). Empty means `["app"]`:
    /// shown only when Settings is opened for the whole application.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_contexts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub filesystem: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub buffer: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    /// `<plugin-name>.<command>`.
    pub id: String,
    pub title: String,
}

/// Which settings layers may hold a declared setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingScope {
    /// Only the user settings file may set it.
    #[default]
    User,
    /// The user file sets the default and a project may override it.
    Project,
}

/// The value type of a declared setting.
///
/// Same JSON/TOML shape as `manang_settings::SettingType`: an internally
/// tagged `type` field (`bool`, `int`, `float`, `enum`, `string`, `path`,
/// `list`) plus that type's parameters (`min`/`max`, `values`, `item`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SettingKind {
    Bool,
    Int {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Float {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Enum {
        values: Vec<String>,
    },
    String,
    Path,
    List {
        item: Box<SettingKind>,
    },
}

/// A setting a plugin declares under `[[contributes.settings]]`.
///
/// The host registers it with the settings plugin (owner = plugin name) and
/// pushes effective values to `<plugin>.configure` (ADR 0007).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SettingSpec {
    /// `<plugin-name>.<kebab-case>`, e.g. `latex.engine`.
    pub id: String,
    #[serde(flatten)]
    pub kind: SettingKind,
    /// Required; validated against `kind`. Optional here only so a missing
    /// default yields a precise [`ManifestError::BadSetting`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub scope: SettingScope,
    #[serde(default)]
    pub requires_restart: bool,
    /// Short human label for the Settings UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    #[serde(default)]
    pub hooks: Vec<Hook>,
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub plugin: PluginMeta,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub contributes: Contributes,
}

/// The places a settings group can belong to. Settings opened from a place
/// show only the groups declaring it; opened for the whole application
/// (`app`) they show everything. `engine:<name>` names a document engine,
/// such as `engine:typst`.
pub const SETTINGS_CONTEXTS: &[&str] = &["app", "project", "document", "figure", "diagram", "pdf"];

/// Most contexts one settings group may declare.
pub const MAX_SETTINGS_CONTEXTS: usize = 8;

/// The context of a group that declares none.
pub const DEFAULT_SETTINGS_CONTEXT: &str = "app";

/// True for one of [`SETTINGS_CONTEXTS`] or `engine:<kebab-case name>`.
pub fn valid_settings_context(context: &str) -> bool {
    SETTINGS_CONTEXTS.contains(&context) || context.strip_prefix("engine:").is_some_and(valid_name)
}

/// Why a list of settings contexts is invalid, if it is: too many entries,
/// an unknown value or a repeated one.
pub fn check_settings_contexts(contexts: &[String]) -> Result<(), String> {
    if contexts.len() > MAX_SETTINGS_CONTEXTS {
        return Err(format!(
            "at most {MAX_SETTINGS_CONTEXTS} settings contexts are allowed"
        ));
    }
    let mut seen = HashSet::new();
    for context in contexts {
        if !valid_settings_context(context) {
            return Err(format!(
                "unknown settings context `{context}`; expected one of {} or `engine:<name>`",
                SETTINGS_CONTEXTS.join(", ")
            ));
        }
        if !seen.insert(context.as_str()) {
            return Err(format!("settings context `{context}` is listed twice"));
        }
    }
    Ok(())
}

/// Largest manifest the parser will look at.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Most commands (or permission entries) one manifest may declare.
pub const MAX_ENTRIES: usize = 256;

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.ends_with('-')
}

/// One or more dot-separated lowercase kebab-case segments.
fn valid_local_id(id: &str) -> bool {
    id.split('.').all(valid_name)
}

/// Why `value` is not a valid value of `kind`, if it is not.
fn check_value(kind: &SettingKind, value: &serde_json::Value) -> Result<(), String> {
    use serde_json::Value;
    match (kind, value) {
        (SettingKind::Bool, Value::Bool(_)) | (SettingKind::String, Value::String(_)) => Ok(()),
        (SettingKind::Int { min, max }, Value::Number(n)) => {
            let i = n.as_i64().ok_or_else(|| format!("{n} is not an integer"))?;
            if min.is_some_and(|m| i < m) || max.is_some_and(|m| i > m) {
                return Err(format!("{i} is out of range"));
            }
            Ok(())
        }
        (SettingKind::Float { min, max }, Value::Number(n)) => {
            let f = n.as_f64().ok_or_else(|| format!("{n} is not a number"))?;
            if min.is_some_and(|m| f < m) || max.is_some_and(|m| f > m) {
                return Err(format!("{f} is out of range"));
            }
            Ok(())
        }
        (SettingKind::Enum { values }, Value::String(s)) => {
            if values.contains(s) {
                Ok(())
            } else {
                Err(format!("`{s}` is not one of {}", values.join(", ")))
            }
        }
        (SettingKind::Path, Value::String(s)) if !s.is_empty() => Ok(()),
        (SettingKind::List { item }, Value::Array(items)) => {
            items.iter().try_for_each(|v| check_value(item, v))
        }
        (kind, other) => Err(format!("{other} does not match type {kind:?}")),
    }
}

/// Check one setting declaration's type parameters and default.
fn check_setting(setting: &SettingSpec) -> Result<(), String> {
    if setting.description.trim().is_empty() {
        return Err("a description is required".to_owned());
    }
    match &setting.kind {
        SettingKind::Int {
            min: Some(a),
            max: Some(b),
        } if a > b => return Err(format!("minimum {a} is above maximum {b}")),
        SettingKind::Float { min, max } => {
            if min.is_some_and(|m| !m.is_finite()) || max.is_some_and(|m| !m.is_finite()) {
                return Err("range bounds must be finite".to_owned());
            }
            if let (Some(a), Some(b)) = (min, max)
                && a > b
            {
                return Err(format!("minimum {a} is above maximum {b}"));
            }
        }
        SettingKind::Enum { values } if values.is_empty() => {
            return Err("an enum needs at least one value".to_owned());
        }
        SettingKind::List { item } if matches!(**item, SettingKind::List { .. }) => {
            return Err("lists cannot be nested".to_owned());
        }
        _ => {}
    }
    let default = setting
        .default
        .as_ref()
        .ok_or_else(|| "a default is required".to_owned())?;
    check_value(&setting.kind, default).map_err(|e| format!("the default is invalid: {e}"))
}

/// A relative, forward-slash path with no `..`, root or drive components.
fn safe_entry(entry: &str) -> bool {
    !entry.is_empty()
        && !entry.starts_with('/')
        && !entry.contains(['\\', '\0', ':'])
        && entry
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

impl Manifest {
    /// Parse and fully validate a manifest.
    pub fn from_toml(source: &str) -> Result<Self, ManifestError> {
        if source.len() > MAX_MANIFEST_BYTES {
            return Err(ManifestError::TooLarge(source.len(), MAX_MANIFEST_BYTES));
        }
        let manifest: Self = toml::from_str(source)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Check names, command namespacing and permission syntax.
    pub fn validate(&self) -> Result<(), ManifestError> {
        let name = &self.plugin.name;
        if !valid_name(name) {
            return Err(ManifestError::BadName(name.clone()));
        }
        if self.contributes.commands.len() > MAX_ENTRIES {
            return Err(ManifestError::TooMany(MAX_ENTRIES, "commands"));
        }
        let p = &self.permissions;
        if p.filesystem.len() + p.network.len() + p.buffer.len() > MAX_ENTRIES {
            return Err(ManifestError::TooMany(MAX_ENTRIES, "permissions"));
        }
        if let Some(entry) = &self.plugin.entry
            && !safe_entry(entry)
        {
            return Err(ManifestError::BadEntry(entry.clone()));
        }
        let prefix = format!("{name}.");
        let mut seen = HashSet::new();
        for command in &self.contributes.commands {
            let local = command.id.strip_prefix(&prefix).unwrap_or("");
            if local.is_empty() {
                return Err(ManifestError::UnnamespacedCommand {
                    plugin: name.clone(),
                    command: command.id.clone(),
                });
            }
            if !seen.insert(command.id.as_str()) {
                return Err(ManifestError::DuplicateCommand(command.id.clone()));
            }
        }
        if self.contributes.settings.len() > MAX_ENTRIES {
            return Err(ManifestError::TooMany(MAX_ENTRIES, "settings"));
        }
        let mut seen = HashSet::new();
        for setting in &self.contributes.settings {
            let local = setting.id.strip_prefix(&prefix).unwrap_or("");
            if !valid_local_id(local) {
                return Err(ManifestError::UnnamespacedSetting {
                    plugin: name.clone(),
                    setting: setting.id.clone(),
                });
            }
            if !seen.insert(setting.id.as_str()) {
                return Err(ManifestError::DuplicateSetting(setting.id.clone()));
            }
            check_setting(setting).map_err(|message| ManifestError::BadSetting {
                id: setting.id.clone(),
                message,
            })?;
        }
        check_settings_contexts(&self.plugin.settings_contexts)
            .map_err(ManifestError::BadSettingsContexts)?;
        CapabilitySet::from_permissions(&self.permissions)?;
        Ok(())
    }

    /// The contexts the plugin's Settings section belongs to: its declared
    /// `settings_contexts`, or `["app"]` when it declares none.
    pub fn settings_contexts(&self) -> Vec<String> {
        if self.plugin.settings_contexts.is_empty() {
            vec![DEFAULT_SETTINGS_CONTEXT.to_owned()]
        } else {
            self.plugin.settings_contexts.clone()
        }
    }

    /// The command through which a plugin that declares settings receives
    /// their effective values: `<plugin>.configure`.
    pub fn configure_command(&self) -> String {
        format!("{}.configure", self.plugin.name)
    }

    /// The capabilities this manifest grants.
    pub fn capabilities(&self) -> Result<CapabilitySet, ManifestError> {
        Ok(CapabilitySet::from_permissions(&self.permissions)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARP: &str = r#"
[plugin]
name = "marp-slides"
version = "1.0.0"
description = "Marp slide generation plugin for Markdown"

[permissions]
filesystem = ["read:templates"]
network = []
buffer = ["read:active", "write:active"]

[contributes]
hooks = ["on_compile_pre"]

[[contributes.commands]]
id = "marp-slides.export"
title = "Export slides"
"#;

    #[test]
    fn parses_the_documented_manifest() {
        let m = Manifest::from_toml(MARP).expect("valid manifest");
        assert_eq!(m.plugin.name, "marp-slides");
        assert_eq!(
            m.plugin.tier,
            Tier::Wasm,
            "third-party default is sandboxed"
        );
        assert_eq!(m.contributes.hooks, vec![Hook::OnCompilePre]);
        assert_eq!(m.contributes.commands[0].id, "marp-slides.export");
    }

    #[test]
    fn minimal_manifest_has_no_permissions() {
        let m =
            Manifest::from_toml("[plugin]\nname = \"a\"\nversion = \"0.1.0\"\n").expect("valid");
        assert_eq!(m.permissions, Permissions::default());
    }

    #[test]
    fn rejects_bad_names() {
        for name in ["Marp", "1abc", "a_b", "a-", ""] {
            let src = format!("[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\n");
            assert!(
                matches!(Manifest::from_toml(&src), Err(ManifestError::BadName(_))),
                "{name}"
            );
        }
    }

    #[test]
    fn rejects_unnamespaced_and_duplicate_commands() {
        let head = "[plugin]\nname = \"a\"\nversion = \"1.0.0\"\n";
        let other = format!("{head}[[contributes.commands]]\nid = \"b.run\"\ntitle = \"x\"\n");
        assert!(matches!(
            Manifest::from_toml(&other),
            Err(ManifestError::UnnamespacedCommand { .. })
        ));
        let dup = format!(
            "{head}[[contributes.commands]]\nid = \"a.run\"\ntitle = \"x\"\n\
             [[contributes.commands]]\nid = \"a.run\"\ntitle = \"y\"\n"
        );
        assert!(matches!(
            Manifest::from_toml(&dup),
            Err(ManifestError::DuplicateCommand(_))
        ));
    }

    #[test]
    fn rejects_bad_permissions_unknown_keys_and_versions() {
        let head = "[plugin]\nname = \"a\"\nversion = \"1.0.0\"\n";
        assert!(matches!(
            Manifest::from_toml(&format!(
                "{head}[permissions]\nfilesystem = [\"read:../x\"]\n"
            )),
            Err(ManifestError::Capability(_))
        ));
        assert!(
            Manifest::from_toml(&format!("{head}[permissions]\nsyscalls = [\"all\"]\n")).is_err()
        );
        assert!(Manifest::from_toml("[plugin]\nname = \"a\"\nversion = \"one\"\n").is_err());
    }

    #[test]
    fn rejects_oversized_and_unsafe_entry() {
        let big = format!(
            "[plugin]\nname = \"a\"\nversion = \"1.0.0\"\n#{}\n",
            "x".repeat(MAX_MANIFEST_BYTES)
        );
        assert!(matches!(
            Manifest::from_toml(&big),
            Err(ManifestError::TooLarge(..))
        ));
        for entry in ["../x.wasm", "/abs.wasm", "a//b", "C:\\x", "a\\b", ""] {
            let src = format!("[plugin]\nname = \"a\"\nversion = \"1.0.0\"\nentry = {entry:?}\n");
            assert!(
                matches!(Manifest::from_toml(&src), Err(ManifestError::BadEntry(_))),
                "{entry}"
            );
        }
        let ok = "[plugin]\nname = \"a\"\nversion = \"1.0.0\"\nentry = \"bin/p.wasm\"\n";
        assert!(Manifest::from_toml(ok).is_ok());
    }

    const LATEX_HEAD: &str = "[plugin]\nname = \"latex\"\nversion = \"1.0.0\"\n";

    const LATEX_SETTINGS: &str = r#"
[[contributes.settings]]
id = "latex.engine"
type = "enum"
values = ["pdflatex", "xelatex"]
default = "pdflatex"
scope = "project"
title = "Engine"
description = "TeX engine."

[[contributes.settings]]
id = "latex.timeout-seconds"
type = "int"
min = 1
max = 600
default = 120
description = "Timeout."
requires-restart = true

[[contributes.settings]]
id = "latex.extra"
type = "list"
item = { type = "string" }
default = []
description = "Extra."

[[contributes.settings]]
id = "latex.ratio"
type = "float"
min = 0
max = 1.5
default = 1
description = "Ratio."
"#;

    #[test]
    fn parses_declared_settings() {
        let m = Manifest::from_toml(&format!("{LATEX_HEAD}{LATEX_SETTINGS}")).expect("valid");
        let s = &m.contributes.settings;
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].scope, SettingScope::Project);
        assert_eq!(s[0].title.as_deref(), Some("Engine"));
        assert_eq!(s[0].default, Some(serde_json::json!("pdflatex")));
        assert_eq!(
            s[1].kind,
            SettingKind::Int {
                min: Some(1),
                max: Some(600)
            }
        );
        assert!(s[1].requires_restart);
        assert_eq!(s[1].default, Some(serde_json::json!(120)));
        assert_eq!(s[2].scope, SettingScope::User);
        assert_eq!(
            s[2].kind,
            SettingKind::List {
                item: Box::new(SettingKind::String)
            }
        );
        assert_eq!(
            s[3].kind,
            SettingKind::Float {
                min: Some(0.0),
                max: Some(1.5)
            }
        );
        assert_eq!(m.configure_command(), "latex.configure");
    }

    #[test]
    fn rejects_bad_settings() {
        let one = |body: &str| format!("{LATEX_HEAD}[[contributes.settings]]\n{body}");
        let cases = [
            (
                "id = \"other.x\"\ntype = \"bool\"\ndefault = true\ndescription = \"d\"\n",
                "unnamespaced",
            ),
            (
                "id = \"latex\"\ntype = \"bool\"\ndefault = true\ndescription = \"d\"\n",
                "bare plugin name",
            ),
            (
                "id = \"latex.Bad\"\ntype = \"bool\"\ndefault = true\ndescription = \"d\"\n",
                "bad local id",
            ),
            (
                "id = \"latex.x\"\ntype = \"bool\"\ndescription = \"d\"\n",
                "no default",
            ),
            (
                "id = \"latex.x\"\ntype = \"bool\"\ndefault = 1\ndescription = \"d\"\n",
                "wrong default type",
            ),
            (
                "id = \"latex.x\"\ntype = \"enum\"\nvalues = [\"a\"]\ndefault = \"b\"\ndescription = \"d\"\n",
                "default outside enum",
            ),
            (
                "id = \"latex.x\"\ntype = \"int\"\nmin = 5\nmax = 1\ndefault = 3\ndescription = \"d\"\n",
                "inverted range",
            ),
            (
                "id = \"latex.x\"\ntype = \"int\"\nmax = 1\ndefault = 3\ndescription = \"d\"\n",
                "default above max",
            ),
            (
                "id = \"latex.x\"\ntype = \"bool\"\ndefault = true\n",
                "no description",
            ),
            (
                "id = \"latex.x\"\ntype = \"colour\"\ndefault = true\ndescription = \"d\"\n",
                "unknown type",
            ),
        ];
        for (body, why) in cases {
            assert!(Manifest::from_toml(&one(body)).is_err(), "{why}");
        }
        let decl = "id = \"latex.x\"\ntype = \"bool\"\ndefault = true\ndescription = \"d\"\n";
        let dup = one(&format!("{decl}[[contributes.settings]]\n{decl}"));
        assert!(matches!(
            Manifest::from_toml(&dup),
            Err(ManifestError::DuplicateSetting(_))
        ));
        let mut m = Manifest::from_toml(&one(decl)).expect("valid");
        let template = m.contributes.settings[0].clone();
        m.contributes.settings = (0..=MAX_ENTRIES)
            .map(|i| SettingSpec {
                id: format!("latex.s{i}"),
                ..template.clone()
            })
            .collect();
        assert!(matches!(
            m.validate(),
            Err(ManifestError::TooMany(_, "settings"))
        ));
    }

    #[test]
    fn hook_modes() {
        assert_eq!(Hook::OnCompilePre.mode(), HookMode::Pipeline);
        assert_eq!(Hook::OnLint.mode(), HookMode::FanOut);
    }

    #[test]
    fn settings_contexts_default_to_app() {
        let m = Manifest::from_toml(LATEX_HEAD).expect("valid");
        assert!(m.plugin.settings_contexts.is_empty());
        assert_eq!(m.settings_contexts(), vec!["app".to_owned()]);
    }

    #[test]
    fn parses_settings_contexts() {
        let src = "[plugin]\nname = \"latex\"\nversion = \"1.0.0\"\n\
                   settings_contexts = [\"document\", \"engine:latex\"]\n";
        let m = Manifest::from_toml(src).expect("valid");
        assert_eq!(m.settings_contexts(), vec!["document", "engine:latex"]);
    }

    #[test]
    fn rejects_bad_settings_contexts() {
        let with = |list: &str| {
            format!("[plugin]\nname = \"a\"\nversion = \"1.0.0\"\nsettings_contexts = {list}\n")
        };
        for list in [
            "[\"editor\"]",
            "[\"engine:\"]",
            "[\"engine:Typst\"]",
            "[\"Document\"]",
            "[\"\"]",
            "[\"document\", \"document\"]",
            "[\"app\", \"project\", \"document\", \"figure\", \"diagram\", \"pdf\", \"engine:a\", \"engine:b\", \"engine:c\"]",
        ] {
            assert!(
                matches!(
                    Manifest::from_toml(&with(list)),
                    Err(ManifestError::BadSettingsContexts(_))
                ),
                "{list}"
            );
        }
        for good in SETTINGS_CONTEXTS {
            assert!(valid_settings_context(good), "{good}");
        }
        assert!(valid_settings_context("engine:typst"));
        assert!(Manifest::from_toml(&with("[]")).is_ok());
    }
}
