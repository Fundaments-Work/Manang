//! Shared helpers for Manang's versioned on-disk formats.
//!
//! Every file Manang writes follows one policy (see `docs/formats.md`):
//!
//! * a file carries an integer `version` (or `schema`) key; a file without one
//!   predates the key and is version 1;
//! * an **older** version is migrated forward in memory, and the old file is
//!   copied once to `<name>.bak-v<N>` before the migrated text replaces it;
//! * a **newer** version than this build understands is read-only: it is never
//!   rewritten, so a downgrade cannot destroy data written by a later release;
//! * a file that cannot be parsed is renamed to `<name>.corrupt` (never
//!   overwritten, never deleted) and the caller starts from defaults with a
//!   warning, instead of failing to start.
//!
//! The crate is a leaf: it knows nothing about any particular format. The
//! crates that own a format pass in the current version and their migration
//! steps. It uses `std::fs` and `tempfile`, so it is for the desktop core, not
//! for crates that must compile to `wasm32-unknown-unknown`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Everything a format helper can fail with.
#[derive(Debug, Error)]
pub enum FormatError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    /// The text is not TOML at all.
    #[error("not valid TOML: {0}")]
    Syntax(String),
    /// The version key is present but not a positive integer.
    #[error("`{key}` must be a positive whole number")]
    BadVersion { key: String },
    /// A file written by a newer Manang: kept read-only.
    #[error(
        "{what} was written by a newer Manang (format version {found}; this version understands up to {supported}). It is kept read-only so nothing is lost; update Manang to change it"
    )]
    Newer {
        what: String,
        found: u32,
        supported: u32,
    },
    /// No step is registered for part of the chain, or a step refused.
    #[error("cannot migrate {what} from version {from} to {to}: {message}")]
    Migration {
        what: String,
        from: u32,
        to: u32,
        message: String,
    },
}

impl FormatError {
    fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// How a file's version relates to the one this build writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compat {
    /// Same version: nothing to do.
    Current,
    /// Older: migrate forward (after a backup).
    Older(u32),
    /// Newer: read-only.
    Newer(u32),
}

/// Version assumed for a file that has no version key: the shape that shipped
/// before the key existed.
pub const UNVERSIONED: u32 = 1;

/// Compare a found version (`None` = no key = [`UNVERSIONED`]) with `current`.
pub fn compat(found: Option<u32>, current: u32) -> Compat {
    let found = found.unwrap_or(UNVERSIONED);
    match found.cmp(&current) {
        std::cmp::Ordering::Equal => Compat::Current,
        std::cmp::Ordering::Less => Compat::Older(found),
        std::cmp::Ordering::Greater => Compat::Newer(found),
    }
}

fn as_version(n: u64, key: &str) -> Result<u32, FormatError> {
    u32::try_from(n)
        .ok()
        .filter(|v| *v >= 1)
        .ok_or_else(|| FormatError::BadVersion {
            key: key.to_owned(),
        })
}

/// The integer `key` of a JSON object: `Ok(None)` when absent or not an object.
pub fn json_version(value: &serde_json::Value, key: &str) -> Result<Option<u32>, FormatError> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| FormatError::BadVersion {
                key: key.to_owned(),
            })
            .and_then(|n| as_version(n, key))
            .map(Some),
    }
}

/// The integer `key` at the top level of a TOML table: `Ok(None)` when absent.
pub fn toml_version(table: &toml::Table, key: &str) -> Result<Option<u32>, FormatError> {
    match table.get(key) {
        None => Ok(None),
        Some(toml::Value::Integer(n)) => u64::try_from(*n)
            .map_err(|_| FormatError::BadVersion {
                key: key.to_owned(),
            })
            .and_then(|n| as_version(n, key))
            .map(Some),
        Some(_) => Err(FormatError::BadVersion {
            key: key.to_owned(),
        }),
    }
}

/// The top-level integer `key` of TOML `text`, for a crate that parses with a
/// different `toml` major version than this one.
pub fn toml_text_version(text: &str, key: &str) -> Result<Option<u32>, FormatError> {
    let table: toml::Table =
        toml::from_str(text).map_err(|e| FormatError::Syntax(e.message().to_owned()))?;
    toml_version(&table, key)
}

/// One migration step: version `n` to `n + 1`, on the format's parsed value.
pub type Step<V> = fn(V) -> Result<V, String>;

/// Run the steps that take `value` from version `from` to `to`. `steps[i]`
/// migrates version `i + 1` to `i + 2`. Forward only: `from > to` is an error.
pub fn migrate_chain<V>(
    what: &str,
    mut value: V,
    from: u32,
    to: u32,
    steps: &[Step<V>],
) -> Result<V, FormatError> {
    let fail = |message: String| FormatError::Migration {
        what: what.to_owned(),
        from,
        to,
        message,
    };
    if from > to {
        return Err(fail("migrations only go forward".to_owned()));
    }
    for version in from..to {
        let index = usize::try_from(version.saturating_sub(1)).unwrap_or(usize::MAX);
        let step = steps
            .get(index)
            .ok_or_else(|| fail(format!("no step from version {version}")))?;
        value = step(value).map_err(|m| fail(format!("from version {version}: {m}")))?;
    }
    Ok(value)
}

/// The sibling `<name><suffix>` of `path`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// The backup path for a file about to be migrated away from `version`.
pub fn backup_path(path: &Path, version: u32) -> PathBuf {
    sibling(path, &format!(".bak-v{version}"))
}

/// Copy `path` to `<name>.bak-v<version>` unless that backup already exists
/// (the first backup is the real original; later runs must not replace it).
/// Returns the backup path when one was made.
pub fn backup_once(path: &Path, version: u32) -> Result<Option<PathBuf>, FormatError> {
    let dest = backup_path(path, version);
    if dest.exists() {
        return Ok(None);
    }
    fs::copy(path, &dest)
        .map_err(|e| FormatError::io(format!("backing up {}", path.display()), e))?;
    Ok(Some(dest))
}

/// Rename an unreadable file to `<name>.corrupt` (or `.corrupt.1`, `.corrupt.2`
/// ... when one is already there), so the next write cannot destroy it.
pub fn set_aside_corrupt(path: &Path) -> Result<PathBuf, FormatError> {
    let mut dest = sibling(path, ".corrupt");
    let mut n = 0u32;
    while dest.exists() && n < 1000 {
        n += 1;
        dest = sibling(path, &format!(".corrupt.{n}"));
    }
    fs::rename(path, &dest)
        .map_err(|e| FormatError::io(format!("setting aside {}", path.display()), e))?;
    Ok(dest)
}

/// Write `bytes` to `target` atomically: a synced temp file in the same
/// directory, renamed over the target. Creates the parent directory.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), FormatError> {
    let shown = target.display().to_string();
    let parent = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    fs::create_dir_all(parent)
        .map_err(|e| FormatError::io(format!("creating the folder of {shown}"), e))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".manang-tmp-")
        .tempfile_in(parent)
        .map_err(|e| FormatError::io(format!("creating a temp file for {shown}"), e))?;
    tmp.write_all(bytes)
        .map_err(|e| FormatError::io(format!("writing {shown}"), e))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| FormatError::io(format!("syncing {shown}"), e))?;
    tmp.persist(target)
        .map_err(|e| FormatError::io(format!("replacing {shown}"), e.error))?;
    Ok(())
}

/// Write migrated `text` over `path`, keeping the old file once as
/// `<name>.bak-v<from>` first. Returns the backup path when one was made.
pub fn write_migrated(path: &Path, from: u32, text: &str) -> Result<Option<PathBuf>, FormatError> {
    let backup = if path.exists() {
        backup_once(path, from)?
    } else {
        None
    };
    write_atomic(path, text.as_bytes())?;
    Ok(backup)
}

/// The error to report when something tries to write a read-only (newer) file.
pub fn refuse_newer(what: &str, found: u32, supported: u32) -> FormatError {
    FormatError::Newer {
        what: what.to_owned(),
        found,
        supported,
    }
}

/// What loading a versioned file found, for the caller to tell the user.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Notes {
    /// Migrated forward from this version; the file's old copy is at `backup`.
    pub migrated_from: Option<u32>,
    pub backup: Option<PathBuf>,
    /// Written by a newer Manang: do not write it back.
    pub read_only_newer: Option<u32>,
    /// The file was unreadable and was set aside here.
    pub set_aside: Option<PathBuf>,
}

impl Notes {
    /// One sentence per thing the user should know, for a warning list.
    pub fn warnings(&self, what: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(v) = self.migrated_from {
            let kept = self
                .backup
                .as_ref()
                .map(|b| format!("; the old file is kept as {}", b.display()))
                .unwrap_or_default();
            out.push(format!("{what} was updated from format version {v}{kept}"));
        }
        if let Some(v) = self.read_only_newer {
            out.push(format!(
                "{what} was written by a newer Manang (format version {v}) and is read-only"
            ));
        }
        if let Some(p) = &self.set_aside {
            out.push(format!(
                "{what} could not be read and was kept as {}; defaults are in use",
                p.display()
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat_treats_a_missing_version_as_one() {
        assert_eq!(compat(None, 1), Compat::Current);
        assert_eq!(compat(None, 3), Compat::Older(1));
        assert_eq!(compat(Some(2), 2), Compat::Current);
        assert_eq!(compat(Some(999), 2), Compat::Newer(999));
    }

    #[test]
    fn versions_are_read_from_json_and_toml() {
        let v = serde_json::json!({ "version": 2, "x": 1 });
        assert_eq!(json_version(&v, "version").ok(), Some(Some(2)));
        assert_eq!(json_version(&v, "schema").ok(), Some(None));
        assert!(json_version(&serde_json::json!({ "version": "two" }), "version").is_err());
        assert!(json_version(&serde_json::json!({ "version": 0 }), "version").is_err());
        let t: toml::Table = toml::from_str("version = 3\n").unwrap_or_default();
        assert_eq!(toml_version(&t, "version").ok(), Some(Some(3)));
        let bad: toml::Table = toml::from_str("version = \"3\"\n").unwrap_or_default();
        assert!(toml_version(&bad, "version").is_err());
    }

    #[test]
    fn the_chain_runs_each_step_once_and_only_forward() {
        let steps: [Step<Vec<u32>>; 2] = [
            |mut v| {
                v.push(2);
                Ok(v)
            },
            |mut v| {
                v.push(3);
                Ok(v)
            },
        ];
        assert_eq!(
            migrate_chain("test", vec![1], 1, 3, &steps).ok(),
            Some(vec![1, 2, 3])
        );
        assert_eq!(
            migrate_chain("test", vec![1, 2], 2, 3, &steps).ok(),
            Some(vec![1, 2, 3])
        );
        assert!(migrate_chain("test", vec![1], 3, 1, &steps).is_err());
        assert!(migrate_chain("test", vec![1], 1, 4, &steps).is_err());
        let failing: [Step<u32>; 1] = [|_| Err("nope".to_owned())];
        let err = migrate_chain("test", 1, 1, 2, &failing)
            .err()
            .map(|e| e.to_string());
        assert!(err.is_some_and(|m| m.contains("nope")));
    }

    #[test]
    fn a_backup_is_made_once_and_never_replaced() {
        let dir = tempfile::tempdir().expect("tmp");
        let file = dir.path().join("settings.toml");
        fs::write(&file, "old = 1\n").expect("write");
        let first = write_migrated(&file, 1, "version = 2\n").ok().flatten();
        assert_eq!(first, Some(dir.path().join("settings.toml.bak-v1")));
        // A second migration from the same version keeps the first backup.
        write_migrated(&file, 1, "version = 2\nmore = 1\n").expect("again");
        let kept = fs::read_to_string(dir.path().join("settings.toml.bak-v1"));
        assert_eq!(kept.ok().as_deref(), Some("old = 1\n"));
        let now = fs::read_to_string(&file);
        assert_eq!(now.ok().as_deref(), Some("version = 2\nmore = 1\n"));
    }

    #[test]
    fn corrupt_files_are_set_aside_without_overwriting_earlier_ones() {
        let dir = tempfile::tempdir().expect("tmp");
        let file = dir.path().join("plugins.toml");
        fs::write(&file, "a").expect("write");
        let one = set_aside_corrupt(&file).expect("aside");
        fs::write(&file, "b").expect("write");
        let two = set_aside_corrupt(&file).expect("aside");
        assert_ne!(one, two);
        assert!(!file.exists());
        assert_eq!(fs::read_to_string(one).ok().as_deref(), Some("a"));
        assert_eq!(fs::read_to_string(two).ok().as_deref(), Some("b"));
    }

    #[test]
    fn warnings_describe_each_outcome() {
        let notes = Notes {
            migrated_from: Some(1),
            backup: Some(PathBuf::from("/x/a.toml.bak-v1")),
            read_only_newer: Some(9),
            set_aside: Some(PathBuf::from("/x/a.toml.corrupt")),
        };
        insta::assert_debug_snapshot!(notes.warnings("settings.toml"));
    }
}
