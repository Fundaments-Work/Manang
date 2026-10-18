//! Loading Manang's own machine-written TOML files (`projects.toml`,
//! `recent-projects.toml`) under the format policy of `docs/formats.md`:
//!
//! * a damaged file is set aside as `<name>.corrupt` (never overwritten) and the
//!   list starts empty with a warning, instead of being silently replaced by
//!   the next write;
//! * a file from a newer Manang is read as far as it makes sense and never
//!   written back;
//! * an older version is migrated forward after a one-time `<name>.bak-v<N>`.

use std::fs;
use std::io;
use std::path::Path;

use manang_formats::{
    Compat, Step, compat, migrate_chain, set_aside_corrupt, toml_version, write_migrated,
};
use serde::de::DeserializeOwned;

/// The result of loading one file.
pub(crate) struct Loaded<T> {
    pub value: T,
    /// Set when the file was written by a newer Manang: do not write it.
    pub newer: Option<u32>,
    pub warnings: Vec<String>,
}

fn empty<T: Default>(warnings: Vec<String>) -> Loaded<T> {
    Loaded {
        value: T::default(),
        newer: None,
        warnings,
    }
}

/// Load `file`. `what` names it in messages. `steps` migrate older versions
/// (`steps[i]`: version `i + 1` to `i + 2`) and `current` is the version this
/// build writes; the `version` key is the top-level integer `version`.
pub(crate) fn load_toml<T: DeserializeOwned + Default>(
    file: &Path,
    what: &str,
    current: u32,
    steps: &[Step<toml::Table>],
) -> Loaded<T> {
    let text = match fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return empty(Vec::new()),
        Err(e) => {
            return empty(vec![format!(
                "{what} could not be read ({e}); starting empty and leaving the file alone"
            )]);
        }
    };
    let mut warnings = Vec::new();
    let aside = |why: String, warnings: &mut Vec<String>| {
        match set_aside_corrupt(file) {
            Ok(to) => warnings.push(format!(
                "{what} {why}; it was kept as {} and a fresh one is in use",
                to.display()
            )),
            Err(e) => warnings.push(format!("{what} {why}, and it could not be set aside: {e}")),
        }
        empty::<T>(std::mem::take(warnings))
    };
    let table: toml::Table = match toml::from_str(&text) {
        Ok(t) => t,
        Err(e) => {
            return aside(
                format!("is not valid TOML ({})", e.message()),
                &mut warnings,
            );
        }
    };
    let version = match toml_version(&table, "version") {
        Ok(v) => v,
        Err(e) => return aside(format!("has a bad version ({e})"), &mut warnings),
    };
    match compat(version, current) {
        Compat::Current => match T::deserialize(toml::Value::Table(table)) {
            Ok(value) => Loaded {
                value,
                newer: None,
                warnings,
            },
            Err(e) => aside(format!("does not match its format ({e})"), &mut warnings),
        },
        Compat::Newer(found) => {
            // Read what we can; never write it back.
            let value = T::deserialize(toml::Value::Table(table)).unwrap_or_default();
            warnings.push(format!(
                "{what} was written by a newer Manang (format version {found}) and is read-only here; changes to it are not saved until you update"
            ));
            Loaded {
                value,
                newer: Some(found),
                warnings,
            }
        }
        Compat::Older(from) => {
            let migrated = migrate_chain(what, table, from, current, steps).and_then(|t| {
                let value = T::deserialize(toml::Value::Table(t.clone())).map_err(|e| {
                    manang_formats::FormatError::Migration {
                        what: what.to_owned(),
                        from,
                        to: current,
                        message: e.to_string(),
                    }
                })?;
                Ok((t, value))
            });
            match migrated {
                Ok((mut t, value)) => {
                    t.insert(
                        "version".to_owned(),
                        toml::Value::Integer(i64::from(current)),
                    );
                    let text = toml::to_string_pretty(&t).unwrap_or_default();
                    match write_migrated(file, from, &text) {
                        Ok(Some(backup)) => warnings.push(format!(
                            "{what} was updated from format version {from}; the old file is kept as {}",
                            backup.display()
                        )),
                        Ok(None) => {}
                        Err(e) => warnings.push(format!("{what} could not be updated on disk: {e}")),
                    }
                    Loaded {
                        value,
                        newer: None,
                        warnings,
                    }
                }
                Err(e) => aside(format!("could not be updated ({e})"), &mut warnings),
            }
        }
    }
}
