//! Path confinement: every path a command touches must stay inside the root.
//!
//! Defence in depth, applied to every request:
//!
//! 1. **Lexical check** ([`lexical_relative`]): the caller's string must be a
//!    plain relative path. Absolute paths, drive prefixes, `..`, backslashes
//!    and NUL are refused outright; `.` and repeated `/` are normalised away.
//! 2. **Symlink check** ([`ProjectRoot::resolve`]): the deepest existing
//!    ancestor of the target is canonicalised and must still be inside the
//!    canonical root, so a symlink that points out of the project is refused
//!    even if the path text looks innocent.
//! 3. **Protected names**: `.git` can never be written (hooks are code
//!    execution), and the root itself can never be renamed or deleted.
//!
//! Remaining limitation: this is check-then-use. A local attacker able to
//! swap a directory for a symlink between the check and the operation can win
//! the race. Closing that fully needs `openat`-style capability handles
//! (`cap-std`); it is out of scope for a single-user desktop app, where the
//! attacker would already own the files.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::ProjectError;

/// Upper bound on the byte length of a project-relative path.
pub const MAX_PATH_BYTES: usize = 4096;

/// Directory holding Manang's own ephemeral state (cache, trash).
pub const STATE_DIR: &str = ".manang";

fn invalid(path: &str, reason: &'static str) -> ProjectError {
    ProjectError::InvalidPath {
        path: truncate(path),
        reason,
    }
}

fn truncate(s: &str) -> String {
    const MAX: usize = 120;
    if s.len() <= MAX {
        return s.to_owned();
    }
    let mut end = MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// Validate and normalise a caller-supplied relative path. The result is
/// purely lexical and may be empty (meaning the root).
pub fn lexical_relative(raw: &str) -> Result<PathBuf, ProjectError> {
    if raw.len() > MAX_PATH_BYTES {
        return Err(invalid(raw, "path is too long"));
    }
    if raw.contains('\0') {
        return Err(invalid(raw, "path contains a NUL byte"));
    }
    if raw.contains('\\') {
        return Err(invalid(raw, "use `/` as the path separator"));
    }
    let mut out = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err(ProjectError::Escapes(truncate(raw))),
            Component::RootDir | Component::Prefix(_) => {
                return Err(invalid(raw, "path must be relative to the project"));
            }
        }
    }
    Ok(out)
}

/// Render a relative path with `/` separators, as exposed over IPC.
/// `None` when a component is not valid UTF-8.
pub fn to_slash(rel: &Path) -> Option<String> {
    let mut out = String::new();
    for (i, c) in rel.components().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(c.as_os_str().to_str()?);
    }
    Some(out)
}

fn has_component(rel: &Path, name: &str) -> bool {
    rel.components()
        .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(name))
}

/// A canonicalised project directory against which paths are resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    root: PathBuf,
}

impl ProjectRoot {
    /// Canonicalise `dir`, which must exist and be a directory.
    pub fn open(dir: &Path) -> Result<Self, ProjectError> {
        let shown = dir.display().to_string();
        let root = dir.canonicalize().map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => ProjectError::NotFound(shown.clone()),
            _ => ProjectError::io(format!("opening {shown}"), e),
        })?;
        if !root.is_dir() {
            return Err(ProjectError::NotADirectory(shown));
        }
        Ok(Self { root })
    }

    /// The canonical root directory.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Resolve `raw` to an absolute path inside the root, following symlinks
    /// (including a symlink as the final component). The target need not
    /// exist. Fails if the result would leave the root.
    pub fn resolve(&self, raw: &str) -> Result<PathBuf, ProjectError> {
        let rel = lexical_relative(raw)?;
        let joined = self.root.join(&rel);
        self.confine(raw, &joined)
    }

    /// Like [`resolve`](Self::resolve) but a symlink in the final component is
    /// *not* followed: the result names the link itself. Used by rename and
    /// delete, which act on the entry rather than its target. Fails for the
    /// root itself.
    pub fn resolve_entry(&self, raw: &str) -> Result<PathBuf, ProjectError> {
        let rel = lexical_relative(raw)?;
        let Some(name) = rel.file_name() else {
            return Err(ProjectError::Protected("the project root".to_owned()));
        };
        let parent = rel.parent().unwrap_or(Path::new(""));
        let parent = self.confine(raw, &self.root.join(parent))?;
        Ok(parent.join(name))
    }

    /// Resolve for a mutation: as [`resolve`](Self::resolve), additionally
    /// refusing the root and anything inside `.git`.
    pub fn resolve_for_write(&self, raw: &str) -> Result<PathBuf, ProjectError> {
        let rel = lexical_relative(raw)?;
        if rel.as_os_str().is_empty() {
            return Err(ProjectError::Protected("the project root".to_owned()));
        }
        let path = self.resolve(raw)?;
        self.guard_git(raw, &path)?;
        Ok(path)
    }

    /// Resolve an entry for rename/delete: acts on a symlink itself, refuses
    /// the root, `.git`, and Manang's own `.manang` state directory.
    pub fn resolve_for_move(&self, raw: &str) -> Result<PathBuf, ProjectError> {
        let rel = lexical_relative(raw)?;
        if rel
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() == STATE_DIR)
        {
            return Err(ProjectError::Protected(truncate(raw)));
        }
        let path = self.resolve_entry(raw)?;
        self.guard_git(raw, &path)?;
        Ok(path)
    }

    fn guard_git(&self, raw: &str, absolute: &Path) -> Result<(), ProjectError> {
        match absolute.strip_prefix(&self.root) {
            Ok(rel) if has_component(rel, ".git") => Err(ProjectError::Protected(truncate(raw))),
            _ => Ok(()),
        }
    }

    /// Project-relative form of an absolute path under the root.
    pub fn relative(&self, absolute: &Path) -> Option<PathBuf> {
        absolute
            .strip_prefix(&self.root)
            .ok()
            .map(Path::to_path_buf)
    }

    /// Canonicalise the deepest existing ancestor of `joined`, check it is
    /// inside the root, and re-attach the not-yet-existing tail.
    fn confine(&self, raw: &str, joined: &Path) -> Result<PathBuf, ProjectError> {
        let mut probe = joined.to_path_buf();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        loop {
            match probe.symlink_metadata() {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    match probe.file_name().map(ToOwned::to_owned) {
                        Some(name) => tail.push(name),
                        None => return Err(invalid(raw, "no existing ancestor")),
                    }
                    if !probe.pop() {
                        return Err(invalid(raw, "no existing ancestor"));
                    }
                }
                // e.g. a path component is a file ("NotADirectory") or
                // permission denied.
                Err(e) => return Err(ProjectError::io(format!("checking {}", truncate(raw)), e)),
            }
        }
        let mut canonical = probe.canonicalize().map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                invalid(raw, "dangling or looping symlink")
            } else {
                ProjectError::io(format!("resolving {}", truncate(raw)), e)
            }
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(ProjectError::Escapes(truncate(raw)));
        }
        for name in tail.into_iter().rev() {
            canonical.push(name);
        }
        Ok(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_normalises_and_rejects() {
        assert_eq!(lexical_relative("").ok(), Some(PathBuf::new()));
        assert_eq!(lexical_relative(".").ok(), Some(PathBuf::new()));
        assert_eq!(
            lexical_relative("./a//b/./c.txt").ok(),
            Some(PathBuf::from("a/b/c.txt"))
        );
        assert!(matches!(
            lexical_relative("../x"),
            Err(ProjectError::Escapes(_))
        ));
        assert!(matches!(
            lexical_relative("a/../../x"),
            Err(ProjectError::Escapes(_))
        ));
        assert!(lexical_relative("/etc/passwd").is_err());
        assert!(lexical_relative("a\\b").is_err());
        assert!(lexical_relative("a\0b").is_err());
        assert!(lexical_relative(&"a/".repeat(MAX_PATH_BYTES)).is_err());
    }

    #[test]
    fn unicode_names_are_fine() {
        assert_eq!(
            lexical_relative("図/é/日本語.typ").ok(),
            Some(PathBuf::from("図/é/日本語.typ"))
        );
    }

    #[test]
    fn to_slash_joins_with_forward_slashes() {
        assert_eq!(to_slash(Path::new("a/b/c")), Some("a/b/c".to_owned()));
        assert_eq!(to_slash(Path::new("")), Some(String::new()));
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "é".repeat(200);
        assert!(truncate(&s).ends_with("..."));
    }
}
