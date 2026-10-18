//! `<config>/manang/plugins.toml`: what the user decided about each plugin,
//! and the project's tighten-only override.
//!
//! Human-readable TOML, written atomically. The file never holds plugin
//! code, only decisions: enabled or not, the permissions the user approved,
//! where the plugin came from.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use manang_kernel::manifest::{MAX_MANIFEST_BYTES, Permissions};
use serde::{Deserialize, Serialize};
use thiserror::Error;

fn yes() -> bool {
    true
}

/// The user's decisions about one plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRecord {
    /// Off means unloaded but installed.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The exact permissions the user approved; the plugin gets no more.
    #[serde(default)]
    pub granted: Permissions,
    /// Everything the plugin asked for when the user last decided. A later
    /// version asking for something outside this is held for approval.
    #[serde(default)]
    pub reviewed: Permissions,
    /// The version the user approved. `None`: installed, never approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_version: Option<String>,
    /// A newer version waiting for approval (kept beside the running one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_version: Option<String>,
    /// Seconds since the Unix epoch.
    #[serde(default)]
    pub installed_at: u64,
    /// The folder or archive it was installed from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Publisher, once signatures can vouch for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
}

impl Default for PluginRecord {
    fn default() -> Self {
        Self {
            enabled: true,
            granted: Permissions::default(),
            reviewed: Permissions::default(),
            approved_version: None,
            pending_version: None,
            installed_at: 0,
            source: None,
            publisher: None,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    #[serde(default)]
    plugins: BTreeMap<String, PluginRecord>,
}

/// The registry could not be read or written.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("`{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{path}` is not a valid plugin registry: {reason}")]
    Parse { path: PathBuf, reason: String },
    #[error("`{path}` is larger than {limit} bytes")]
    TooLarge { path: PathBuf, limit: usize },
    #[error("plugin name `{0}` in `{1}` is not lowercase kebab-case")]
    BadName(String, PathBuf),
}

/// Plugin names are lowercase kebab-case starting with a letter.
pub fn valid_plugin_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.ends_with('-')
        && name.len() <= 64
}

/// Write `bytes` to `path` through a temporary file and a rename.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        // Best effort: the temp file is ours.
        let _ = fs::remove_file(&tmp);
    }
    result
}

const MAX_REGISTRY_BYTES: u64 = MAX_MANIFEST_BYTES as u64 * 16;

fn read_text(path: &Path) -> Result<Option<String>, RegistryError> {
    let io = |source| RegistryError::Io {
        path: path.to_path_buf(),
        source,
    };
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io(source)),
    };
    if meta.len() > MAX_REGISTRY_BYTES {
        return Err(RegistryError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_REGISTRY_BYTES as usize,
        });
    }
    fs::read_to_string(path).map(Some).map_err(io)
}

/// The user's plugin decisions, in memory and (when a path is set) on disk.
#[derive(Debug, Default)]
pub struct Registry {
    path: Option<PathBuf>,
    file: RegistryFile,
}

impl Registry {
    /// A registry that is never written (tests, tools).
    pub fn ephemeral() -> Self {
        Self::default()
    }

    /// Read `path`; a missing file is an empty registry.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let file = match read_text(path)? {
            None => RegistryFile::default(),
            Some(text) => {
                let file: RegistryFile =
                    toml::from_str(&text).map_err(|e| RegistryError::Parse {
                        path: path.to_path_buf(),
                        reason: e.to_string(),
                    })?;
                if let Some(bad) = file.plugins.keys().find(|n| !valid_plugin_name(n)) {
                    return Err(RegistryError::BadName(bad.clone(), path.to_path_buf()));
                }
                file
            }
        };
        Ok(Self {
            path: Some(path.to_path_buf()),
            file,
        })
    }

    /// An empty registry that will be written to `path`.
    pub fn empty_at(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            file: RegistryFile::default(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&PluginRecord> {
        self.file.plugins.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.file.plugins.keys()
    }

    /// The record for `name`, created with defaults if absent. Call
    /// [`Registry::save`] after changing it.
    pub fn entry(&mut self, name: &str) -> &mut PluginRecord {
        self.file.plugins.entry(name.to_owned()).or_default()
    }

    pub fn remove(&mut self, name: &str) -> Option<PluginRecord> {
        self.file.plugins.remove(name)
    }

    /// Write the file (no-op for an ephemeral registry).
    pub fn save(&self) -> Result<(), RegistryError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut text = String::from(
            "# Manang plugin registry: your decisions about installed plugins.\n\
             # Edit with care; the Plugins view keeps it up to date.\n\n",
        );
        text.push_str(
            &toml::to_string_pretty(&self.file).map_err(|e| RegistryError::Parse {
                path: path.clone(),
                reason: e.to_string(),
            })?,
        );
        atomic_write(path, text.as_bytes()).map_err(|source| RegistryError::Io {
            path: path.clone(),
            source,
        })
    }
}

/// `<project>/.manang/plugins.toml`. A project may only turn plugins off for
/// itself: there is deliberately no way to enable anything here, so opening a
/// project can never switch on what the user disabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectOverride {
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// Where a project keeps its override.
pub fn project_override_path(project: &Path) -> PathBuf {
    project.join(".manang").join("plugins.toml")
}

impl ProjectOverride {
    /// Read the project's override; a missing file means no override.
    pub fn load(project: &Path) -> Result<Self, RegistryError> {
        let path = project_override_path(project);
        let Some(text) = read_text(&path)? else {
            return Ok(Self::default());
        };
        let parsed: Self = toml::from_str(&text).map_err(|e| RegistryError::Parse {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        if let Some(bad) = parsed.disabled.iter().find(|n| !valid_plugin_name(n)) {
            return Err(RegistryError::BadName(bad.clone(), path));
        }
        Ok(parsed)
    }

    pub fn disables(&self, name: &str) -> bool {
        self.disabled.iter().any(|n| n == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults_to_enabled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plugins.toml");
        let mut reg = Registry::empty_at(&path);
        {
            let rec = reg.entry("alpha");
            rec.approved_version = Some("1.0.0".into());
            rec.granted.network.push("example.org".into());
        }
        reg.save().expect("save");
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("[plugins.alpha]"), "{text}");
        let again = Registry::load(&path).expect("load");
        assert_eq!(again.get("alpha"), reg.get("alpha"));
        assert!(again.get("alpha").is_some_and(|r| r.enabled));
    }

    #[test]
    fn rejects_unknown_keys_and_bad_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plugins.toml");
        fs::write(&path, "[plugins.a]\nsurprise = 1\n").expect("write");
        assert!(matches!(
            Registry::load(&path),
            Err(RegistryError::Parse { .. })
        ));
        fs::write(&path, "[plugins.\"../x\"]\n").expect("write");
        assert!(matches!(
            Registry::load(&path),
            Err(RegistryError::BadName(..))
        ));
    }

    #[test]
    fn a_project_override_can_only_disable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manang = dir.path().join(".manang");
        fs::create_dir_all(&manang).expect("mkdir");
        fs::write(manang.join("plugins.toml"), "disabled = [\"git\"]\n").expect("write");
        let o = ProjectOverride::load(dir.path()).expect("load");
        assert!(o.disables("git") && !o.disables("search"));
        fs::write(manang.join("plugins.toml"), "enabled = [\"git\"]\n").expect("write");
        assert!(ProjectOverride::load(dir.path()).is_err());
        fs::remove_file(manang.join("plugins.toml")).expect("rm");
        assert_eq!(
            ProjectOverride::load(dir.path()).expect("none"),
            ProjectOverride::default()
        );
    }
}
