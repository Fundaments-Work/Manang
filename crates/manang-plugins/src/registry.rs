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

/// The format version `plugins.toml` is written in (`docs/formats.md`). A
/// file from a newer Manang is read as far as it is understood and never
/// written back; a damaged one is set aside as `plugins.toml.corrupt`.
pub const REGISTRY_VERSION: u32 = 1;

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
    /// Absent in a v1 file written before the key existed.
    #[serde(default, rename = "version", skip_serializing)]
    _version: Option<u32>,
    #[serde(default)]
    plugins: BTreeMap<String, PluginRecord>,
}

/// What is written: the version first, then the decisions.
#[derive(Serialize)]
struct RegistryOut<'a> {
    version: u32,
    plugins: &'a BTreeMap<String, PluginRecord>,
}

/// The fields of a [`PluginRecord`], for reading a newer file leniently.
const RECORD_KEYS: &[&str] = &[
    "enabled",
    "granted",
    "reviewed",
    "approved_version",
    "pending_version",
    "installed_at",
    "source",
    "publisher",
];

/// Keep only the keys this build knows, so a newer file's additions do not
/// make the whole file unreadable.
fn strip_unknown(table: &mut toml::Table) {
    table.retain(|k, _| k == "version" || k == "plugins");
    if let Some(toml::Value::Table(plugins)) = table.get_mut("plugins") {
        for (_, record) in plugins.iter_mut() {
            if let toml::Value::Table(t) = record {
                t.retain(|k, _| RECORD_KEYS.contains(&k));
            }
        }
    }
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
    /// The file is from a newer Manang and is kept read-only.
    #[error(transparent)]
    Format(#[from] manang_formats::FormatError),
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
    /// Written by a newer Manang: read-only.
    newer: Option<u32>,
}

impl Registry {
    /// A registry that is never written (tests, tools).
    pub fn ephemeral() -> Self {
        Self::default()
    }

    /// Read `path`; a missing file is an empty registry.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let parse = |reason: String| RegistryError::Parse {
            path: path.to_path_buf(),
            reason,
        };
        let mut newer = None;
        let file = match read_text(path)? {
            None => RegistryFile::default(),
            Some(text) => {
                let version = manang_formats::toml_text_version(&text, "version")
                    .map_err(|e| parse(e.to_string()))?;
                let file: RegistryFile = match manang_formats::compat(version, REGISTRY_VERSION) {
                    manang_formats::Compat::Newer(found) => {
                        newer = Some(found);
                        let mut table: toml::Table =
                            toml::from_str(&text).map_err(|e| parse(e.to_string()))?;
                        strip_unknown(&mut table);
                        // Unreadable even leniently: keep what is on disk, start empty.
                        toml::Value::Table(table).try_into().unwrap_or_default()
                    }
                    // No older versions exist yet; migration steps go here.
                    _ => toml::from_str(&text).map_err(|e| parse(e.to_string()))?,
                };
                if let Some(bad) = file.plugins.keys().find(|n| !valid_plugin_name(n)) {
                    return Err(RegistryError::BadName(bad.clone(), path.to_path_buf()));
                }
                file
            }
        };
        Ok(Self {
            path: Some(path.to_path_buf()),
            file,
            newer,
        })
    }

    /// An empty registry that will be written to `path`.
    pub fn empty_at(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            file: RegistryFile::default(),
            newer: None,
        }
    }

    /// The format version of the file, when a newer Manang wrote it: the
    /// registry is then read-only.
    pub fn newer_version(&self) -> Option<u32> {
        self.newer
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
        if let Some(found) = self.newer {
            return Err(
                manang_formats::refuse_newer("plugins.toml", found, REGISTRY_VERSION).into(),
            );
        }
        let mut text = String::from(
            "# Manang plugin registry: your decisions about installed plugins.\n\
             # Edit with care; the Plugins view keeps it up to date.\n\n",
        );
        text.push_str(
            &toml::to_string_pretty(&RegistryOut {
                version: REGISTRY_VERSION,
                plugins: &self.file.plugins,
            })
            .map_err(|e| RegistryError::Parse {
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
    /// Format version; absent in a v1 file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
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
        let parse = |reason: String| RegistryError::Parse {
            path: path.clone(),
            reason,
        };
        let version = manang_formats::toml_text_version(&text, "version")
            .map_err(|e| parse(e.to_string()))?;
        let parsed: Self = if let manang_formats::Compat::Newer(_) =
            manang_formats::compat(version, REGISTRY_VERSION)
        {
            // A newer Manang may add keys; a project override is only ever
            // read, so read what is understood.
            let mut table: toml::Table = toml::from_str(&text).map_err(|e| parse(e.to_string()))?;
            table.retain(|k, _| k == "version" || k == "disabled");
            toml::Value::Table(table)
                .try_into()
                .map_err(|e: toml::de::Error| parse(e.to_string()))?
        } else {
            toml::from_str(&text).map_err(|e| parse(e.to_string()))?
        };
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
