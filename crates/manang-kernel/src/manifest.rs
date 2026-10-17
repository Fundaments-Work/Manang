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
    /// Third-party, sandboxed in Extism/Wasmtime. The safe default.
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    #[serde(default)]
    pub hooks: Vec<Hook>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub plugin: PluginMeta,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub contributes: Contributes,
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.ends_with('-')
}

impl Manifest {
    /// Parse and fully validate a manifest.
    pub fn from_toml(source: &str) -> Result<Self, ManifestError> {
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
        CapabilitySet::from_permissions(&self.permissions)?;
        Ok(())
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
    fn hook_modes() {
        assert_eq!(Hook::OnCompilePre.mode(), HookMode::Pipeline);
        assert_eq!(Hook::OnLint.mode(), HookMode::FanOut);
    }
}
