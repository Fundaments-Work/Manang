//! The Typst `World`: project files on disk, overlaid with in-memory sources.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::files::{FileLoader, FileStore, FsRoot};
use typst_kit::fonts::{self, FontStore};
use typst_kit::packages::FsPackages;

use crate::error::TypstError;

/// Where fonts and packages come from.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// Scan the operating system's font directories in addition to the
    /// embedded fonts. Slower to start and machine dependent.
    pub system_fonts: bool,
    /// Extra directories searched recursively for fonts.
    pub font_dirs: Vec<PathBuf>,
    /// Package directories laid out as `<namespace>/<name>/<version>`, searched
    /// in order. The platform data and cache directories are appended after
    /// these. Packages are never downloaded.
    pub package_dirs: Vec<PathBuf>,
}

/// Build the shared font store: embedded fonts first so they always win ties.
pub(crate) fn font_store(env: &Environment) -> FontStore {
    let mut store = FontStore::new();
    store.extend(fonts::embedded());
    for dir in &env.font_dirs {
        store.extend(fonts::scan(dir));
    }
    if env.system_fonts {
        store.extend(fonts::system());
    }
    store
}

pub(crate) fn package_roots(env: &Environment) -> Vec<FsPackages> {
    env.package_dirs
        .iter()
        .cloned()
        .map(FsPackages::new)
        .chain(FsPackages::system_data())
        .chain(FsPackages::system_cache())
        .collect()
}

/// Serves project files, preferring unsaved in-memory text, and package files
/// from local package directories only.
pub(crate) struct Loader {
    pub(crate) root: FsRoot,
    pub(crate) overrides: HashMap<FileId, Arc<str>>,
    packages: Vec<FsPackages>,
}

impl Loader {
    pub(crate) fn new(root: PathBuf, packages: Vec<FsPackages>) -> Self {
        Self {
            root: FsRoot::new(root),
            overrides: HashMap::new(),
            packages,
        }
    }
}

impl FileLoader for Loader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => match self.overrides.get(&id) {
                Some(text) => Ok(Bytes::from_string(text.to_string())),
                None => self.root.load(id.vpath()),
            },
            VirtualRoot::Package(spec) => self
                .packages
                .iter()
                .find_map(|p| p.obtain(spec))
                .ok_or_else(|| {
                    FileError::Package(typst::diag::PackageError::NotFound(spec.clone()))
                })?
                .load(id.vpath()),
        }
    }
}

/// Intern the project file id for a user-supplied path, which may be absolute
/// (inside `root`) or relative to it.
pub(crate) fn project_file_id(root: &Path, path: &str) -> Result<FileId, TypstError> {
    let bad = |reason: String| TypstError::BadPath {
        path: path.to_owned(),
        reason,
    };
    let candidate = Path::new(path);
    let vpath = if candidate.is_absolute() {
        VirtualPath::virtualize(root, candidate)
            .map_err(|_| TypstError::OutsideRoot(path.to_owned()))?
    } else {
        VirtualPath::new(path).map_err(|e| bad(e.to_string()))?
    };
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

/// A borrowed view over the long-lived stores, valid for one compile.
pub(crate) struct ProjectWorld<'a> {
    pub(crate) library: &'a LazyHash<Library>,
    pub(crate) fonts: &'a FontStore,
    pub(crate) files: &'a FileStore<Loader>,
    pub(crate) main: FileId,
}

impl World for ProjectWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
        let shift = offset.map_or(0, |d| d.seconds() as i64);
        let secs = i64::try_from(now.as_secs()).ok()?.checked_add(shift)?;
        let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
        Datetime::from_ymd(i32::try_from(year).ok()?, month, day)
    }
}

pub(crate) fn default_library() -> LazyHash<Library> {
    LazyHash::new(<Library as LibraryExt>::default())
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
