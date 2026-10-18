//! Reading and editing the hand-written TOML files.
//!
//! TOML is the format because these files are edited by hand: it has
//! comments, no trailing-comma traps and no quoting noise, unlike JSON.
//! Edits go through `toml_edit` so comments, ordering and spacing around
//! untouched keys survive, and every write is atomic (temp file + rename).
//!
//! A file that fails to parse is never overwritten: the user is mid-edit
//! and clobbering it would destroy their work.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde_json::{Map, Number, Value};
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::error::SettingsError;

/// The top-level key holding a file's format version.
pub const VERSION_KEY: &str = "version";
/// The format version of `settings.toml`, the project settings file and
/// `keybindings.toml` (`docs/formats.md`). No older version exists yet; a
/// future one adds steps to [`migrate`].
pub const FORMAT_VERSION: u32 = 1;

/// The `version` of a parsed document: `Ok(None)` when absent (a v1 file).
pub fn doc_version(doc: &DocumentMut) -> Result<Option<u32>, String> {
    match doc.get(VERSION_KEY) {
        None => Ok(None),
        Some(item) => item
            .as_integer()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|v| *v >= 1)
            .map(Some)
            .ok_or_else(|| format!("`{VERSION_KEY}` must be a positive whole number")),
    }
}

/// Bring `doc`, written as `from`, up to [`FORMAT_VERSION`]. There is nothing
/// older than v1, so this is the place future steps are chained.
fn migrate(doc: DocumentMut, from: u32) -> Result<DocumentMut, SettingsError> {
    let steps: [manang_formats::Step<DocumentMut>; 0] = [];
    Ok(manang_formats::migrate_chain(
        "settings file",
        doc,
        from,
        FORMAT_VERSION,
        &steps,
    )?)
}

/// Add `version = N` to a document that has none, so later releases know what
/// shape it is.
fn stamp_version(doc: &mut DocumentMut) {
    if doc.get(VERSION_KEY).is_none() {
        doc.as_table_mut()
            .insert(VERSION_KEY, toml_edit::value(i64::from(FORMAT_VERSION)));
    }
}

/// Largest settings file read.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Atomically replace `target` with `bytes`, creating parent directories.
pub fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), SettingsError> {
    let shown = target.display().to_string();
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            SettingsError::io(
                format!("locating the folder of {shown}"),
                io::Error::other("no parent folder"),
            )
        })?;
    let wrap = |what: &str, e: io::Error| SettingsError::io(format!("{what} {shown}"), e);
    fs::create_dir_all(parent).map_err(|e| wrap("creating the folder for", e))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".manang-tmp-")
        .tempfile_in(parent)
        .map_err(|e| wrap("creating a temp file for", e))?;
    tmp.write_all(bytes).map_err(|e| wrap("writing", e))?;
    tmp.as_file().sync_all().map_err(|e| wrap("syncing", e))?;
    tmp.persist(target)
        .map_err(|e| wrap("replacing", e.error))?;
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Read a file as text. `Ok(None)` when it does not exist.
pub fn read_text(path: &Path) -> Result<Option<String>, SettingsError> {
    let shown = path.display();
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(SettingsError::io(format!("inspecting {shown}"), e)),
    };
    if meta.len() > MAX_FILE_BYTES {
        return Err(SettingsError::io(
            format!("reading {shown}"),
            io::Error::other(format!("file is larger than {MAX_FILE_BYTES} bytes")),
        ));
    }
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SettingsError::io(format!("reading {shown}"), e)),
    }
}

fn item_to_json(item: &Item) -> Value {
    match item {
        Item::None => Value::Null,
        Item::Value(v) => value_to_json(v),
        Item::Table(t) => table_to_json(t),
        Item::ArrayOfTables(a) => Value::Array(a.iter().map(table_to_json).collect()),
    }
}

fn table_to_json(table: &Table) -> Value {
    Value::Object(
        table
            .iter()
            .map(|(k, v)| (k.to_owned(), item_to_json(v)))
            .collect::<Map<_, _>>(),
    )
}

fn value_to_json(value: &toml_edit::Value) -> Value {
    use toml_edit::Value as T;
    match value {
        T::String(s) => Value::String(s.value().clone()),
        T::Integer(i) => Value::Number((*i.value()).into()),
        T::Float(f) => Number::from_f64(*f.value()).map_or(Value::Null, Value::Number),
        T::Boolean(b) => Value::Bool(*b.value()),
        T::Datetime(d) => Value::String(d.value().to_string()),
        T::Array(a) => Value::Array(a.iter().map(value_to_json).collect()),
        T::InlineTable(t) => Value::Object(
            t.iter()
                .map(|(k, v)| (k.to_owned(), value_to_json(v)))
                .collect::<Map<_, _>>(),
        ),
    }
}

/// The whole document as JSON.
pub fn doc_to_json(doc: &DocumentMut) -> Value {
    table_to_json(doc.as_table())
}

/// Flatten nested tables into dotted ids: `{editor: {tab-size: 4}}` becomes
/// `editor.tab-size`. Arrays are leaves.
pub fn flatten(value: &Value) -> BTreeMap<String, Value> {
    fn walk(prefix: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
        if let Value::Object(map) = value {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                walk(&key, v, out);
            }
        } else {
            out.insert(prefix.to_owned(), value.clone());
        }
    }
    let mut out = BTreeMap::new();
    walk("", value, &mut out);
    out
}

/// One layer file as loaded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerFile {
    /// Dotted id to raw value, unvalidated.
    pub values: BTreeMap<String, Value>,
    /// Why the file could not be used, if it could not.
    pub error: Option<String>,
    /// Written by a newer Manang (this format version): read, never written.
    pub newer: Option<u32>,
}

/// Load a settings layer. Never fails: problems become [`LayerFile::error`]
/// and an empty layer, so a typo cannot lock the user out of the app.
pub fn read_layer(path: &Path) -> LayerFile {
    match read_text(path) {
        Ok(None) => LayerFile::default(),
        Ok(Some(text)) => match text.parse::<DocumentMut>() {
            Ok(doc) => {
                let mut values = flatten(&doc_to_json(&doc));
                // The version is the file's, not a setting.
                values.remove(VERSION_KEY);
                let (error, newer) = match doc_version(&doc) {
                    Err(message) => (Some(message), None),
                    Ok(version) => match manang_formats::compat(version, FORMAT_VERSION) {
                        manang_formats::Compat::Newer(found) => (None, Some(found)),
                        _ => (None, None),
                    },
                };
                LayerFile {
                    values,
                    error,
                    newer,
                }
            }
            Err(e) => LayerFile {
                values: BTreeMap::new(),
                error: Some(format!("not valid TOML: {e}")),
                newer: None,
            },
        },
        Err(e) => LayerFile {
            values: BTreeMap::new(),
            error: Some(e.to_string()),
            newer: None,
        },
    }
}

/// Convert a validated JSON value to TOML.
pub fn to_toml_value(path: &str, value: &Value) -> Result<toml_edit::Value, SettingsError> {
    let unstorable = |message: &str| SettingsError::Unstorable {
        path: path.to_owned(),
        message: message.to_owned(),
    };
    Ok(match value {
        Value::Null => return Err(unstorable("TOML has no null")),
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into()
            } else if let Some(f) = n.as_f64() {
                f.into()
            } else {
                return Err(unstorable("number is out of range"));
            }
        }
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => {
            let mut array = Array::new();
            for item in items {
                array.push(to_toml_value(path, item)?);
            }
            toml_edit::Value::Array(array)
        }
        Value::Object(_) => return Err(unstorable("tables are not supported here")),
    })
}

/// Load `path` fresh, apply `change`, and write it back atomically.
///
/// A missing file starts empty. An unparsable file aborts with
/// [`SettingsError::FileInvalid`] and is left untouched, and so is a file
/// written by a newer Manang (a [`SettingsError::Format`] `Newer` error): a
/// downgrade must not destroy what a later release stored. A file without a
/// `version` gets `version = 1` with the first change.
pub fn edit_file(
    path: &Path,
    change: impl FnOnce(&mut DocumentMut) -> Result<bool, SettingsError>,
) -> Result<(), SettingsError> {
    let text = read_text(path)?.unwrap_or_default();
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| SettingsError::FileInvalid {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
    let version = doc_version(&doc).map_err(|message| SettingsError::FileInvalid {
        path: path.display().to_string(),
        message,
    })?;
    let from = match manang_formats::compat(version, FORMAT_VERSION) {
        manang_formats::Compat::Newer(found) => {
            return Err(manang_formats::refuse_newer(
                &path.display().to_string(),
                found,
                FORMAT_VERSION,
            )
            .into());
        }
        manang_formats::Compat::Older(from) => Some(from),
        manang_formats::Compat::Current => None,
    };
    if let Some(from) = from {
        doc = migrate(doc, from)?;
        manang_formats::backup_once(path, from)?;
    }
    if change(&mut doc)? || from.is_some() {
        stamp_version(&mut doc);
        atomic_write(path, doc.to_string().as_bytes())?;
    }
    Ok(())
}

/// Set `id` (dotted) to `value`, creating `[tables]` as needed and keeping
/// the comments around an existing value.
pub fn set_value(doc: &mut DocumentMut, id: &str, value: &Value) -> Result<bool, SettingsError> {
    let segments: Vec<&str> = id.split('.').collect();
    let Some((last, parents)) = segments.split_last() else {
        return Ok(false);
    };
    let mut new = to_toml_value(id, value)?;
    let mut table = doc.as_table_mut();
    for (depth, seg) in parents.iter().enumerate() {
        let item = table.entry(seg).or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        });
        table = item
            .as_table_mut()
            .ok_or_else(|| SettingsError::Unstorable {
                path: id.to_owned(),
                message: format!(
                    "`{}` is already defined as a plain value or inline table; use a [table] section",
                    segments[..=depth].join(".")
                ),
            })?;
    }
    match table.get_mut(last) {
        Some(existing) => {
            if let Item::Value(old) = existing {
                if value_to_json(old) == *value {
                    return Ok(false);
                }
                *new.decor_mut() = old.decor().clone();
            }
            *existing = Item::Value(new);
        }
        None => {
            table.insert(last, Item::Value(new));
        }
    }
    Ok(true)
}

/// Remove `id`, pruning tables that become empty. Returns whether it existed.
pub fn remove_value(doc: &mut DocumentMut, id: &str) -> bool {
    fn remove(table: &mut Table, segs: &[&str]) -> bool {
        match segs {
            [] => false,
            [last] => table.remove(last).is_some(),
            [head, rest @ ..] => {
                let Some(child) = table.get_mut(head).and_then(Item::as_table_mut) else {
                    return false;
                };
                let removed = remove(child, rest);
                if removed && child.is_empty() {
                    table.remove(head);
                }
                removed
            }
        }
    }
    let segments: Vec<&str> = id.split('.').collect();
    remove(doc.as_table_mut(), &segments)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn doc(text: &str) -> DocumentMut {
        text.parse().expect("valid toml")
    }

    #[test]
    fn flatten_nests_into_dotted_ids() {
        let d = doc("[editor]\ntab-size = 4\n[a.b]\nc = [1, 2]\n");
        let flat = flatten(&doc_to_json(&d));
        assert_eq!(flat.get("editor.tab-size"), Some(&json!(4)));
        assert_eq!(flat.get("a.b.c"), Some(&json!([1, 2])));
    }

    #[test]
    fn set_preserves_comments_and_untouched_keys() {
        let mut d = doc(
            "# my settings\n[editor]\n# size\nfont-size = 12 # px\ntab-size = 4\n\n[files]\nautosave = \"off\"\n",
        );
        assert!(set_value(&mut d, "editor.font-size", &json!(16)).expect("set"));
        assert_eq!(
            d.to_string(),
            "# my settings\n[editor]\n# size\nfont-size = 16 # px\ntab-size = 4\n\n[files]\nautosave = \"off\"\n"
        );
    }

    #[test]
    fn set_creates_tables_and_is_a_noop_when_equal() {
        let mut d = doc("");
        assert!(set_value(&mut d, "preview.debounce-ms", &json!(250)).expect("set"));
        assert_eq!(d.to_string(), "[preview]\ndebounce-ms = 250\n");
        assert!(!set_value(&mut d, "preview.debounce-ms", &json!(250)).expect("set"));
    }

    #[test]
    fn set_refuses_to_descend_into_a_plain_value() {
        let mut d = doc("editor = 3\n");
        let err = set_value(&mut d, "editor.tab-size", &json!(2)).expect_err("blocked");
        assert!(matches!(err, SettingsError::Unstorable { .. }));
    }

    #[test]
    fn remove_prunes_empty_tables() {
        let mut d = doc("[editor]\ntab-size = 4\n[files]\nautosave = \"off\"\n");
        assert!(remove_value(&mut d, "editor.tab-size"));
        assert!(!remove_value(&mut d, "editor.tab-size"));
        assert_eq!(d.to_string(), "[files]\nautosave = \"off\"\n");
    }

    #[test]
    fn invalid_file_is_reported_and_never_overwritten() {
        let tmp = tempfile::tempdir().expect("tmp");
        let path = tmp.path().join("settings.toml");
        fs::write(&path, "editor = = broken").expect("write");
        let layer = read_layer(&path);
        assert!(layer.values.is_empty());
        assert!(layer.error.is_some());
        let err = edit_file(&path, |d| set_value(d, "editor.tab-size", &json!(2)));
        assert!(matches!(err, Err(SettingsError::FileInvalid { .. })));
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "editor = = broken"
        );
    }

    #[test]
    fn atomic_write_creates_parents_and_replaces() {
        let tmp = tempfile::tempdir().expect("tmp");
        let path = tmp.path().join("a/b/settings.toml");
        atomic_write(&path, b"x = 1\n").expect("write");
        atomic_write(&path, b"x = 2\n").expect("write");
        assert_eq!(fs::read_to_string(&path).expect("read"), "x = 2\n");
        let leftovers = fs::read_dir(path.parent().expect("parent"))
            .expect("dir")
            .count();
        assert_eq!(leftovers, 1);
    }
}
