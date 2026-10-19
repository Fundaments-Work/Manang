//! Reading a plugin out of a `.manangplugin` archive or a folder, safely.
//!
//! A `.manangplugin` is a **zip** of the plugin folder: `manifest.toml`,
//! `plugin.wasm`, and optionally `README*`, `LICENSE*` and an `assets/` tree,
//! either at the archive root or all under one top-level folder. Nothing else
//! is accepted. Reading is strict: entry names are sanitised (no `..`,
//! absolute paths, drive letters, backslashes, hidden components), symlinks and
//! duplicate names are refused, and entry count and sizes are capped by what
//! is actually read, not by what the headers claim. Nothing is executed.

use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use manang_kernel::manifest::MAX_MANIFEST_BYTES;
use thiserror::Error;

use crate::manager::{DEFAULT_ENTRY, MANIFEST_FILE, MAX_WASM_BYTES};

/// File extension of a plugin archive.
pub const ARCHIVE_EXTENSION: &str = "manangplugin";
/// Most files one archive (or one folder install) may hold.
pub const MAX_ARCHIVE_ENTRIES: usize = 256;
/// Largest README, LICENSE or asset file.
pub const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024;
/// Most bytes one plugin may unpack to: the wasm limit plus room for assets.
pub const MAX_UNPACKED_BYTES: u64 = MAX_WASM_BYTES + 8 * 1024 * 1024;
/// Deepest nesting under `assets/`.
const MAX_DEPTH: usize = 8;
/// Longest entry path.
const MAX_PATH_BYTES: usize = 200;

/// A plugin archive or folder cannot be installed.
#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("cannot read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a valid plugin archive: {0}")]
    Zip(String),
    #[error("the archive holds more than {0} files")]
    TooManyEntries(usize),
    #[error("`{name}` is larger than the limit of {limit} bytes")]
    TooLarge { name: String, limit: u64 },
    #[error("the plugin unpacks to more than {0} bytes")]
    TooLargeTotal(u64),
    #[error("entry name `{0}` is not a safe relative path")]
    UnsafePath(String),
    #[error("`{0}` is a symbolic link; links are not allowed in plugins")]
    Symlink(String),
    #[error("`{0}` appears more than once")]
    Duplicate(String),
    #[error(
        "`{0}` is not allowed in a plugin; only manifest.toml, plugin.wasm, README, LICENSE and assets/ are"
    )]
    Unexpected(String),
    #[error("the plugin has no `{0}`")]
    Missing(&'static str),
    #[error("`{0}` is not a regular file")]
    NotAFile(String),
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ArchiveError + '_ {
    move |source| ArchiveError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A relative, forward-slash path made only of plain components, or an error.
///
/// One trailing `/` (a directory entry) is accepted and dropped. Everything
/// that could leave the destination is refused: empty names, `.`/`..`
/// components, leading `/`, backslashes, drive colons, NUL and other control
/// characters, hidden (dot-prefixed) components, and over-long paths.
pub fn sanitize_entry(name: &str) -> Result<String, ArchiveError> {
    let bad = || ArchiveError::UnsafePath(name.chars().take(80).collect());
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    if trimmed.is_empty() || trimmed.len() > MAX_PATH_BYTES {
        return Err(bad());
    }
    if trimmed.contains(['\\', ':']) || trimmed.chars().any(char::is_control) {
        return Err(bad());
    }
    let mut depth = 0;
    for part in trimmed.split('/') {
        if part.is_empty() || part.starts_with('.') || part.trim() != part {
            return Err(bad());
        }
        depth += 1;
    }
    if depth > MAX_DEPTH {
        return Err(bad());
    }
    Ok(trimmed.to_owned())
}

/// Whether `path` (already sanitised, relative to the plugin root) is a file a
/// plugin may contain.
pub fn is_allowed(path: &str) -> bool {
    if path == MANIFEST_FILE || path == DEFAULT_ENTRY {
        return true;
    }
    if path.starts_with("assets/") {
        return true;
    }
    let upper = path.to_ascii_uppercase();
    !path.contains('/')
        && ["README", "LICENSE"].iter().any(|stem| {
            upper == *stem
                || upper
                    .strip_prefix(stem)
                    .is_some_and(|rest| rest.starts_with(['.', '-']))
        })
}

fn limit_for(path: &str) -> u64 {
    if path == MANIFEST_FILE {
        MAX_MANIFEST_BYTES as u64
    } else if path == DEFAULT_ENTRY {
        MAX_WASM_BYTES
    } else {
        MAX_ASSET_BYTES
    }
}

/// Copy `reader` into a new file at `dest`, at most `limit` bytes.
fn write_capped(
    reader: &mut dyn Read,
    dest: &Path,
    name: &str,
    limit: u64,
) -> Result<u64, ArchiveError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(io(parent))?;
    }
    // `create_new`: a second entry with the same name can never overwrite.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(io(dest))?;
    let mut limited = reader.take(limit + 1);
    let copied = std::io::copy(&mut limited, &mut file).map_err(io(dest))?;
    if copied > limit {
        return Err(ArchiveError::TooLarge {
            name: name.to_owned(),
            limit,
        });
    }
    file.flush().map_err(io(dest))?;
    Ok(copied)
}

/// If every file sits under one shared top-level folder, the paths without it
/// and the folder's name.
fn strip_common_root(names: &[String]) -> (Vec<String>, Option<String>) {
    let first = names
        .first()
        .and_then(|n| n.split_once('/'))
        .map(|(root, _)| root);
    let shared = first.is_some_and(|root| {
        names
            .iter()
            .all(|n| n.split_once('/').is_some_and(|(r, _)| r == root))
    });
    match (shared, first) {
        (true, Some(root)) => (
            names
                .iter()
                .map(|n| n[root.len() + 1..].to_owned())
                .collect(),
            Some(root.to_owned()),
        ),
        _ => (names.to_vec(), None),
    }
}

/// Unpack the archive at `archive` into the existing, empty directory `dest`.
/// Returns the name of the single top-level folder the files sat in, if any.
pub fn extract_archive(archive: &Path, dest: &Path) -> Result<Option<String>, ArchiveError> {
    let file = fs::File::open(archive).map_err(io(archive))?;
    let meta = file.metadata().map_err(io(archive))?;
    if !meta.is_file() {
        return Err(ArchiveError::NotAFile(archive.display().to_string()));
    }
    if meta.len() > MAX_UNPACKED_BYTES {
        return Err(ArchiveError::TooLargeTotal(MAX_UNPACKED_BYTES));
    }
    let mut zip = zip::ZipArchive::new(file).map_err(|e| ArchiveError::Zip(e.to_string()))?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(ArchiveError::TooManyEntries(MAX_ARCHIVE_ENTRIES));
    }

    // Pass 1: names, kinds and declared sizes, before anything is written.
    let mut raw_names = Vec::new();
    let mut indices = Vec::new();
    let mut declared = 0u64;
    for i in 0..zip.len() {
        let entry = zip
            .by_index_raw(i)
            .map_err(|e| ArchiveError::Zip(e.to_string()))?;
        // zip 9 reports a name that is not valid UTF-8 as an error: refuse such an archive.
        let name = entry
            .name()
            .map_err(|e| ArchiveError::Zip(e.to_string()))?
            .into_owned();
        let clean = sanitize_entry(&name)?;
        if entry.is_dir() {
            continue;
        }
        if let Some(mode) = entry.unix_mode() {
            match mode & 0o170000 {
                0o120000 => return Err(ArchiveError::Symlink(clean)),
                0 | 0o100000 => {}
                _ => return Err(ArchiveError::NotAFile(clean)),
            }
        }
        declared = declared.saturating_add(entry.size());
        if declared > MAX_UNPACKED_BYTES {
            return Err(ArchiveError::TooLargeTotal(MAX_UNPACKED_BYTES));
        }
        raw_names.push(clean);
        indices.push(i);
    }
    let (names, root) = strip_common_root(&raw_names);
    let mut seen = BTreeSet::new();
    for name in &names {
        let name = sanitize_entry(name)?;
        if !is_allowed(&name) {
            return Err(ArchiveError::Unexpected(name));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(ArchiveError::Duplicate(name));
        }
    }
    if !seen.contains(MANIFEST_FILE) {
        return Err(ArchiveError::Missing(MANIFEST_FILE));
    }

    // Pass 2: unpack, counting what is really read.
    let mut total = 0u64;
    for (name, index) in names.iter().zip(indices) {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| ArchiveError::Zip(e.to_string()))?;
        let limit = limit_for(name).min(MAX_UNPACKED_BYTES.saturating_sub(total));
        total += write_capped(&mut entry, &dest.join(name), name, limit)?;
    }
    Ok(root)
}

/// Copy the files a plugin may contain out of the folder `src` into the
/// existing, empty directory `dest`. Other files in `src` (sources, build
/// output) are left behind; links among the wanted ones are refused.
pub fn copy_folder(src: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let mut count = 0usize;
    let mut total = 0u64;
    copy_dir(src, dest, "", &mut count, &mut total)?;
    if !dest.join(MANIFEST_FILE).is_file() {
        return Err(ArchiveError::Missing(MANIFEST_FILE));
    }
    Ok(())
}

fn copy_dir(
    src: &Path,
    dest: &Path,
    prefix: &str,
    count: &mut usize,
    total: &mut u64,
) -> Result<(), ArchiveError> {
    let mut entries: Vec<_> = fs::read_dir(src)
        .map_err(io(src))?
        .collect::<Result<_, _>>()
        .map_err(io(src))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            file_name.clone()
        } else {
            format!("{prefix}/{file_name}")
        };
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(io(&path))?;
        let wanted_dir = meta.is_dir() && (rel == "assets" || rel.starts_with("assets/"));
        if meta.is_dir() {
            if wanted_dir {
                let rel = sanitize_entry(&rel)?;
                copy_dir(&path, dest, &rel, count, total)?;
            }
            continue;
        }
        let Ok(clean) = sanitize_entry(&rel) else {
            continue;
        };
        if !is_allowed(&clean) {
            continue;
        }
        if meta.file_type().is_symlink() {
            return Err(ArchiveError::Symlink(clean));
        }
        if !meta.is_file() {
            return Err(ArchiveError::NotAFile(clean));
        }
        *count += 1;
        if *count > MAX_ARCHIVE_ENTRIES {
            return Err(ArchiveError::TooManyEntries(MAX_ARCHIVE_ENTRIES));
        }
        let limit = limit_for(&clean).min(MAX_UNPACKED_BYTES.saturating_sub(*total));
        let mut file = fs::File::open(&path).map_err(io(&path))?;
        *total += write_capped(&mut file, &dest.join(&clean), &clean, limit)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn sanitiser_refuses_every_escape() {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a/./b",
            "a//b",
            "C:/x",
            "a\\b",
            "..",
            ".hidden",
            "a/.git/config",
            "nul\0byte",
            "a/b/c/d/e/f/g/h/i/j",
        ] {
            assert!(sanitize_entry(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(sanitize_entry("assets/a.png").expect("ok"), "assets/a.png");
        assert_eq!(sanitize_entry("assets/").expect("dir"), "assets");
    }

    #[test]
    fn allowed_files_are_the_documented_few() {
        for ok in [
            "manifest.toml",
            "plugin.wasm",
            "README.md",
            "README",
            "LICENSE",
            "LICENSE-MIT",
            "license.txt",
            "assets/icon.svg",
            "assets/a/b.json",
        ] {
            assert!(is_allowed(ok), "{ok}");
        }
        for no in [
            "run.sh",
            "src/lib.rs",
            "READMEX",
            "sub/manifest.toml",
            "plugin.sig",
        ] {
            assert!(!is_allowed(no), "{no}");
        }
    }

    proptest! {
        /// Whatever name an archive claims, an accepted path stays under the
        /// plugin directory: only plain components, nothing that resolves up.
        #[test]
        fn accepted_names_never_escape(name in ".{0,120}") {
            if let Ok(clean) = sanitize_entry(&name) {
                let path = Path::new(&clean);
                prop_assert!(path.is_relative());
                prop_assert!(path
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))));
                let root = Path::new("/plugins/p");
                prop_assert!(root.join(path).starts_with(root));
            }
        }

        #[test]
        fn traversal_shapes_are_always_refused(
            pre in "[a-z]{0,5}", post in "[a-z/]{0,8}", sep in prop::sample::select(vec!["/", "\\"])
        ) {
            let name = format!("{pre}{sep}..{sep}{post}");
            prop_assert!(sanitize_entry(&name).is_err());
        }
    }
}
