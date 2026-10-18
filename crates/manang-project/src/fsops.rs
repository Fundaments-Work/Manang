//! Confined file operations: read, atomic write, rename, delete-to-trash.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ProjectError;
use crate::paths::{ProjectRoot, STATE_DIR, lexical_relative, to_slash};

/// Largest file `read_text`/`read_bytes` will load.
pub const MAX_READ_BYTES: u64 = 64 * 1024 * 1024;
/// Largest payload `write_text` will write.
pub const MAX_WRITE_BYTES: usize = 64 * 1024 * 1024;

fn display(raw: &str) -> String {
    raw.chars().take(120).collect()
}

fn read_limited(root: &ProjectRoot, raw: &str) -> Result<Vec<u8>, ProjectError> {
    let path = root.resolve(raw)?;
    let file = fs::File::open(&path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ProjectError::NotFound(display(raw)),
        _ => ProjectError::io(format!("opening {}", display(raw)), e),
    })?;
    let meta = file
        .metadata()
        .map_err(|e| ProjectError::io(format!("inspecting {}", display(raw)), e))?;
    if !meta.is_file() {
        return Err(ProjectError::NotAFile(display(raw)));
    }
    if meta.len() > MAX_READ_BYTES {
        return Err(ProjectError::TooLarge {
            path: display(raw),
            size: meta.len(),
            limit: MAX_READ_BYTES,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    // `take` guards against the file growing between the stat and the read.
    file.take(MAX_READ_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(format!("reading {}", display(raw)), e))?;
    if bytes.len() as u64 > MAX_READ_BYTES {
        return Err(ProjectError::TooLarge {
            path: display(raw),
            size: bytes.len() as u64,
            limit: MAX_READ_BYTES,
        });
    }
    Ok(bytes)
}

/// Size, modification time and kind of a project entry, without reading it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Stat {
    /// Bytes for a file; 0 for a directory.
    pub size: u64,
    /// Milliseconds since the Unix epoch, when the platform reports it.
    pub modified: Option<u64>,
    /// `file`, `dir` or `other` (a socket, a device...).
    pub kind: &'static str,
}

/// Stat an entry inside the project. A symlink is followed when its target
/// is inside the project, as for reads.
pub fn stat(root: &ProjectRoot, raw: &str) -> Result<Stat, ProjectError> {
    let path = root.resolve(raw)?;
    let meta = fs::metadata(&path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ProjectError::NotFound(display(raw)),
        _ => ProjectError::io(format!("inspecting {}", display(raw)), e),
    })?;
    let kind = if meta.is_dir() {
        "dir"
    } else if meta.is_file() {
        "file"
    } else {
        "other"
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_millis()).ok());
    Ok(Stat {
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified,
        kind,
    })
}

/// Read a UTF-8 text file inside the project.
pub fn read_text(root: &ProjectRoot, raw: &str) -> Result<String, ProjectError> {
    let bytes = read_limited(root, raw)?;
    String::from_utf8(bytes).map_err(|_| ProjectError::NotUtf8(display(raw)))
}

/// Read a file's raw bytes (images, PDFs).
pub fn read_bytes(root: &ProjectRoot, raw: &str) -> Result<Vec<u8>, ProjectError> {
    read_limited(root, raw)
}

/// Write `bytes` to `target` atomically: a temp file in the same directory is
/// fully written and synced, then renamed over the target, so readers see
/// either the old or the new content, never a torn file. Existing permissions
/// are preserved.
pub fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let shown = target.display().to_string();
    let parent = target
        .parent()
        .ok_or_else(|| ProjectError::NotADirectory(shown.clone()))?;
    let wrap = |what: &str, e: io::Error| ProjectError::io(format!("{what} {shown}"), e);

    let mut tmp = tempfile::Builder::new()
        .prefix(".manang-tmp-")
        .tempfile_in(parent)
        .map_err(|e| wrap("creating temp file for", e))?;
    tmp.write_all(bytes).map_err(|e| wrap("writing", e))?;
    tmp.as_file().sync_all().map_err(|e| wrap("syncing", e))?;
    if let Ok(meta) = fs::metadata(target) {
        // Best effort: keep the executable bit etc. of the file we replace.
        let _ = fs::set_permissions(tmp.path(), meta.permissions());
    }
    tmp.persist(target)
        .map_err(|e| wrap("replacing", e.error))?;
    sync_dir(parent);
    Ok(())
}

/// Make the rename durable. Best effort and a no-op where unsupported.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Atomically write UTF-8 text to a project file, creating parent
/// directories. Returns the number of bytes written.
pub fn write_text(root: &ProjectRoot, raw: &str, content: &str) -> Result<usize, ProjectError> {
    if content.len() > MAX_WRITE_BYTES {
        return Err(ProjectError::TooLarge {
            path: display(raw),
            size: content.len() as u64,
            limit: MAX_WRITE_BYTES as u64,
        });
    }
    let path = root.resolve_for_write(raw)?;
    if path.is_dir() {
        return Err(ProjectError::NotAFile(display(raw)));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            ProjectError::io(format!("creating directories for {}", display(raw)), e)
        })?;
    }
    atomic_write(&path, content.as_bytes())?;
    Ok(content.len())
}

/// Rename or move an entry within the project. Refuses to overwrite.
pub fn rename(root: &ProjectRoot, from: &str, to: &str) -> Result<(), ProjectError> {
    let src = root.resolve_for_move(from)?;
    let dst = root.resolve_for_move(to)?;
    if src.symlink_metadata().is_err() {
        return Err(ProjectError::NotFound(display(from)));
    }
    if dst.symlink_metadata().is_ok() {
        return Err(ProjectError::Exists(display(to)));
    }
    if dst.starts_with(&src) {
        return Err(ProjectError::InvalidPath {
            path: display(to),
            reason: "cannot move a directory into itself",
        });
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            ProjectError::io(format!("creating directories for {}", display(to)), e)
        })?;
    }
    fs::rename(&src, &dst)
        .map_err(|e| ProjectError::io(format!("renaming {} to {}", display(from), display(to)), e))
}

static TRASH_SEQ: AtomicU64 = AtomicU64::new(0);

/// Move an entry to the project's recoverable trash,
/// `.manang/trash/<id>/<original path>`, and return the trashed path
/// (project-relative, `/`-separated).
///
/// The trash lives inside the project so the move is an atomic same-volume
/// rename that never copies data and never touches the user's OS trash.
/// Restoring is moving the file back; `.manang/` is git-ignored by the
/// starters.
pub fn trash(root: &ProjectRoot, raw: &str) -> Result<String, ProjectError> {
    let src = root.resolve_for_move(raw)?;
    if src.symlink_metadata().is_err() {
        return Err(ProjectError::NotFound(display(raw)));
    }
    let rel = lexical_relative(raw)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let base = root.path().join(STATE_DIR).join("trash");
    fs::create_dir_all(&base)
        .map_err(|e| ProjectError::io("creating the trash directory".to_owned(), e))?;
    let (slot, id) = loop {
        let id = format!("{millis}-{}", TRASH_SEQ.fetch_add(1, Ordering::Relaxed));
        let slot = base.join(&id);
        match fs::create_dir(&slot) {
            Ok(()) => break (slot, id),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(ProjectError::io("creating a trash slot".to_owned(), e)),
        }
    };
    let dst = slot.join(&rel);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ProjectError::io("preparing the trash slot".to_owned(), e))?;
    }
    fs::rename(&src, &dst)
        .map_err(|e| ProjectError::io(format!("moving {} to the trash", display(raw)), e))?;
    let shown = to_slash(&rel).unwrap_or_default();
    Ok(format!("{STATE_DIR}/trash/{id}/{shown}"))
}
