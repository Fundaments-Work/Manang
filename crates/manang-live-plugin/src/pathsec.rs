//! Path security: the boundary between a remote peer and a guest's disk.
//!
//! An invite holder now writes files on other people's machines, so every path
//! that arrives from the network (manifest entries, tree operations, blob
//! announcements) goes through a [`PathPolicy`] before it touches the file
//! system. Nothing here panics; every refusal is a typed [`PathError`].
//!
//! Layers, in order:
//!
//! 1. **Lexical** ([`PathPolicy::check_rel`], pure): relative, `/` separated,
//!    no empty, `.` or `..` components, no backslash, NUL or control
//!    characters, no Windows-hostile characters, no trailing dot or space,
//!    no Windows reserved device names (`CON`, `NUL`, `COM1`...), no dotfiles
//!    (this covers `.git/` and `.manang/`, which are also named explicitly),
//!    no `manang.toml`, no component from the fixed deny-list (`target`,
//!    `node_modules`, `__pycache__`), bounded length and depth.
//! 2. **Ignore filter** (same [`manang_project::watch::IgnoreFilter`] as the
//!    watcher): a path the watcher would never report is never written.
//! 3. **File system** ([`PathPolicy::check_write`]): a symlink as the final
//!    component is refused; the deepest existing ancestor is canonicalised
//!    and must stay inside the canonical root (and the resolved location is
//!    run through the lexical rules again, so a link `docs -> .git` does not
//!    smuggle a write into `.git`); on case-insensitive file systems a name
//!    that differs from an existing sibling only by case is refused.
//! 4. **Size caps** ([`PathPolicy::check_size`]).
//!
//! Remaining limitation (same as `manang-project`): check-then-use. A local
//! attacker who can swap a directory for a symlink between the check and the
//! write already owns the files.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use manang_live::manifest::MAX_PATH_BYTES;
use manang_live::project::{MAX_DEPTH, NodeKind, valid_name};
use manang_project::ProjectRoot;
use manang_project::watch::IgnoreFilter;
use thiserror::Error;

/// Default cap of one text file (it lives in a CRDT document in memory).
pub const MAX_TEXT_BYTES: u64 = 8 * 1024 * 1024;
/// Default cap of one blob (the wire cap of `manang-live`).
pub const MAX_BLOB_BYTES: u64 = manang_live::blob::MAX_BLOB_BYTES;

/// Directory names a remote peer may never create (build output, vendored
/// dependencies); matched case-insensitively at any depth.
pub const DENY_LIST: &[&str] = &["target", "node_modules", "__pycache__"];

/// Why a remote path or size was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathError {
    /// Empty path.
    #[error("the path is empty")]
    Empty,
    /// Longer than the protocol allows.
    #[error("the path is too long")]
    TooLong,
    /// More components than the protocol allows.
    #[error("the path is nested too deeply")]
    TooDeep,
    /// Absolute path or drive prefix.
    #[error("the path is absolute")]
    Absolute,
    /// `..` component.
    #[error("the path leaves the project")]
    Traversal,
    /// Backslash separator.
    #[error("the path uses a backslash")]
    Backslash,
    /// Empty or `.` component, doubled or trailing slash.
    #[error("the path has an empty component")]
    EmptyComponent,
    /// A component with a hostile character, trailing dot or space.
    #[error("a path component is not a valid file name")]
    InvalidName,
    /// A Windows reserved device name.
    #[error("`{0}` is a reserved name on Windows")]
    Reserved(String),
    /// A component starting with a dot.
    #[error("dotfiles are not shared in a live session")]
    Dotfile,
    /// `.git`, `.manang` or `manang.toml`.
    #[error("`{0}` is never written by a live session")]
    Protected(String),
    /// A fixed deny-list directory.
    #[error("`{0}` is excluded from live sessions")]
    Denied(String),
    /// Filtered by the watcher's ignore rules.
    #[error("the path is ignored by the project's ignore rules")]
    Ignored,
    /// A symlink as the final component.
    #[error("the path is a symbolic link")]
    Symlink,
    /// A symlinked directory resolves outside the project.
    #[error("the path escapes the project through a symbolic link")]
    Escapes,
    /// Differs from an existing sibling only by case.
    #[error("the name collides with `{0}` on a case-insensitive file system")]
    CaseCollision(String),
    /// Over the size cap.
    #[error("{size} bytes is over the limit of {limit}")]
    TooLarge {
        /// Announced size.
        size: u64,
        /// Cap that applies.
        limit: u64,
    },
    /// The file system could not be inspected.
    #[error("file system: {0}")]
    Io(String),
}

/// A path that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafePath {
    /// Project-relative, `/` separated.
    pub rel: String,
    /// Absolute location inside the canonical root.
    pub abs: PathBuf,
}

/// Rules applied to every remote path of one project.
#[derive(Debug)]
pub struct PathPolicy {
    root: ProjectRoot,
    ignore: IgnoreFilter,
    case_insensitive: bool,
    max_text: u64,
    max_blob: u64,
}

impl PathPolicy {
    /// Policy for the project at `dir` (canonicalised). Case-insensitive
    /// collision checks are on by default on Windows and macOS.
    pub fn new(dir: &Path) -> Result<Self, PathError> {
        let root = ProjectRoot::open(dir).map_err(|e| PathError::Io(e.to_string()))?;
        let ignore = IgnoreFilter::load(root.path());
        Ok(Self {
            root,
            ignore,
            case_insensitive: cfg!(any(windows, target_os = "macos")),
            max_text: MAX_TEXT_BYTES,
            max_blob: MAX_BLOB_BYTES,
        })
    }

    /// Force case-insensitive collision checks on or off.
    #[must_use]
    pub fn with_case_insensitive(mut self, on: bool) -> Self {
        self.case_insensitive = on;
        self
    }

    /// Override the size caps (text, blob).
    #[must_use]
    pub fn with_caps(mut self, max_text: u64, max_blob: u64) -> Self {
        self.max_text = max_text;
        self.max_blob = max_blob;
        self
    }

    /// Re-read the root `.gitignore` (call when the watcher reports it).
    pub fn reload_ignore(&mut self) {
        self.ignore = IgnoreFilter::load(self.root.path());
    }

    /// The canonical project root.
    #[must_use]
    pub fn root(&self) -> &ProjectRoot {
        &self.root
    }

    /// Whether collision checks are case-insensitive.
    #[must_use]
    pub fn case_insensitive(&self) -> bool {
        self.case_insensitive
    }

    /// Lexical rules only (layer 1) plus the ignore filter (layer 2); no file
    /// system access. Returns the path unchanged when it is acceptable.
    pub fn check_rel<'a>(&self, rel: &'a str) -> Result<&'a str, PathError> {
        check_lexical(rel)?;
        if self.ignore.is_ignored(Path::new(rel), false) {
            return Err(PathError::Ignored);
        }
        Ok(rel)
    }

    /// Whether a *local* path may be shared (same answer as
    /// [`check_rel`](Self::check_rel), as a boolean).
    #[must_use]
    pub fn is_shareable(&self, rel: &str) -> bool {
        self.check_rel(rel).is_ok()
    }

    /// Full check before creating, writing, renaming to or deleting `rel`.
    pub fn check_write(&self, rel: &str) -> Result<SafePath, PathError> {
        self.check_rel(rel)?;
        let abs = self.root.path().join(rel);
        match abs.symlink_metadata() {
            Ok(m) if m.file_type().is_symlink() => return Err(PathError::Symlink),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(PathError::Io(e.to_string())),
        }
        // Canonicalise the deepest existing ancestor; must stay inside.
        let resolved = self.root.resolve_for_write(rel).map_err(|e| match e {
            manang_project::ProjectError::Protected(_) => PathError::Protected(".git".to_owned()),
            _ => PathError::Escapes,
        })?;
        let Some(resolved_rel) = self.root.relative(&resolved) else {
            return Err(PathError::Escapes);
        };
        let resolved_str = resolved_rel
            .to_str()
            .ok_or(PathError::InvalidName)?
            .replace('\\', "/");
        // A link `docs -> .git` resolves inside the root but is still denied.
        check_lexical(&resolved_str)?;
        if self.case_insensitive {
            self.check_case(rel)?;
        }
        Ok(SafePath {
            rel: rel.to_owned(),
            abs,
        })
    }

    fn check_case(&self, rel: &str) -> Result<(), PathError> {
        let mut dir = self.root.path().to_path_buf();
        for part in rel.split('/') {
            let want = part.to_lowercase();
            match fs::read_dir(&dir) {
                Ok(entries) => {
                    let mut found_exact = false;
                    let mut other: Option<String> = None;
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name == part {
                            found_exact = true;
                        } else if name.to_lowercase() == want {
                            other = Some(name);
                        }
                    }
                    if let (false, Some(name)) = (found_exact, other) {
                        return Err(PathError::CaseCollision(name));
                    }
                }
                // The directory does not exist yet: nothing below can collide.
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(PathError::Io(e.to_string())),
            }
            dir.push(part);
        }
        Ok(())
    }

    /// Size cap for a node of `kind` (directories have no content).
    pub fn check_size(&self, kind: NodeKind, size: u64) -> Result<(), PathError> {
        let limit = match kind {
            NodeKind::Dir => 0,
            NodeKind::Text => self.max_text,
            NodeKind::Blob => self.max_blob,
        };
        if size > limit {
            return Err(PathError::TooLarge { size, limit });
        }
        Ok(())
    }

    /// Size cap for a text file.
    #[must_use]
    pub fn max_text(&self) -> u64 {
        self.max_text
    }
}

/// Windows device names: the stem (text before the first dot, trailing spaces
/// trimmed) is reserved whatever the extension.
fn is_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let up = stem.to_ascii_uppercase();
    matches!(
        up.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((up.starts_with("COM") || up.starts_with("LPT"))
        && matches!(up.get(3..), Some(d) if d.len() == 1 && d.bytes().all(|b| b.is_ascii_digit())))
        || up
            .strip_prefix("COM")
            .or_else(|| up.strip_prefix("LPT"))
            .is_some_and(|d| matches!(d, "\u{b9}" | "\u{b2}" | "\u{b3}"))
}

/// The pure rules (layer 1).
pub fn check_lexical(rel: &str) -> Result<(), PathError> {
    if rel.is_empty() {
        return Err(PathError::Empty);
    }
    if rel.len() > MAX_PATH_BYTES {
        return Err(PathError::TooLong);
    }
    if rel.contains('\\') {
        return Err(PathError::Backslash);
    }
    if rel.starts_with('/') || Path::new(rel).has_root() || has_prefix(rel) {
        return Err(PathError::Absolute);
    }
    let mut depth = 0usize;
    for part in rel.split('/') {
        depth += 1;
        if depth > MAX_DEPTH {
            return Err(PathError::TooDeep);
        }
        match part {
            "" | "." => return Err(PathError::EmptyComponent),
            ".." => return Err(PathError::Traversal),
            _ => {}
        }
        if !valid_name(part) {
            return Err(PathError::InvalidName);
        }
        if is_reserved(part) {
            return Err(PathError::Reserved(part.to_owned()));
        }
        if part.starts_with('.') {
            let low = part.to_ascii_lowercase();
            return Err(if low == ".git" || low == ".manang" {
                PathError::Protected(low)
            } else {
                PathError::Dotfile
            });
        }
        if part.eq_ignore_ascii_case("manang.toml") {
            return Err(PathError::Protected("manang.toml".to_owned()));
        }
        if let Some(d) = DENY_LIST.iter().find(|d| part.eq_ignore_ascii_case(d)) {
            return Err(PathError::Denied((*d).to_owned()));
        }
    }
    Ok(())
}

/// A Windows drive or UNC prefix, recognised on every platform.
fn has_prefix(rel: &str) -> bool {
    let b = rel.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        || matches!(
            Path::new(rel).components().next(),
            Some(Component::Prefix(_))
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(dir: &Path) -> PathPolicy {
        PathPolicy::new(dir)
            .expect("policy")
            .with_case_insensitive(true)
    }

    #[test]
    fn accepts_ordinary_paths() {
        for ok in ["main.typ", "chapters/intro.md", "figs/a b.svg", "ünï/ç.txt"] {
            assert_eq!(check_lexical(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn rejects_hostile_lexical_paths() {
        let cases: &[(&str, PathError)] = &[
            ("", PathError::Empty),
            ("../x", PathError::Traversal),
            ("a/../../x", PathError::Traversal),
            ("/etc/passwd", PathError::Absolute),
            ("C:/x", PathError::Absolute),
            ("a\\b", PathError::Backslash),
            ("a//b", PathError::EmptyComponent),
            ("a/", PathError::EmptyComponent),
            ("./a", PathError::EmptyComponent),
            ("a/b:c", PathError::InvalidName),
            ("a/b ", PathError::InvalidName),
            ("a/b.", PathError::InvalidName),
            ("a\0b", PathError::InvalidName),
            ("a/\u{7}b", PathError::InvalidName),
            ("con", PathError::Reserved("con".into())),
            ("a/NUL.txt", PathError::Reserved("NUL.txt".into())),
            ("com1.tar.gz", PathError::Reserved("com1.tar.gz".into())),
            ("LPT9", PathError::Reserved("LPT9".into())),
            ("COM\u{b9}", PathError::Reserved("COM\u{b9}".into())),
            (".env", PathError::Dotfile),
            ("a/.hidden/x", PathError::Dotfile),
            (".git/config", PathError::Protected(".git".into())),
            ("a/.GIT/hooks/x", PathError::Protected(".git".into())),
            (".manang/trash/x", PathError::Protected(".manang".into())),
            ("manang.toml", PathError::Protected("manang.toml".into())),
            (
                "sub/Manang.TOML",
                PathError::Protected("manang.toml".into()),
            ),
            ("target/x", PathError::Denied("target".into())),
            ("a/node_modules/x", PathError::Denied("node_modules".into())),
        ];
        for (path, want) in cases {
            assert_eq!(check_lexical(path).as_ref(), Err(want), "{path:?}");
        }
        assert_eq!(check_lexical(&"a/".repeat(65)), Err(PathError::TooDeep));
        assert_eq!(check_lexical(&"a".repeat(2000)), Err(PathError::TooLong));
    }

    #[test]
    fn ignore_filter_is_shared_with_the_watcher() {
        let tmp = tempfile::tempdir().expect("tmp");
        fs::write(tmp.path().join(".gitignore"), "build/\n*.aux\n").expect("gi");
        let p = policy(tmp.path());
        assert_eq!(p.check_rel("build/out.pdf"), Err(PathError::Ignored));
        assert_eq!(p.check_rel("x.aux"), Err(PathError::Ignored));
        assert_eq!(p.check_rel("a.typ"), Ok("a.typ"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_out_of_the_project() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().expect("tmp");
        let outside = tempfile::tempdir().expect("outside");
        let root = tmp.path().join("proj");
        fs::create_dir_all(root.join("real")).expect("dirs");
        symlink(outside.path(), root.join("escape")).expect("link");
        symlink(outside.path().join("f.txt"), root.join("flink.txt")).expect("link");
        symlink(root.join("real"), root.join("inside")).expect("link");
        fs::create_dir_all(root.join(".git")).expect("git");
        symlink(root.join(".git"), root.join("sneaky")).expect("link");
        let p = policy(&root);
        assert_eq!(p.check_write("escape/x.txt"), Err(PathError::Escapes));
        assert_eq!(p.check_write("escape/deep/x.txt"), Err(PathError::Escapes));
        assert_eq!(p.check_write("flink.txt"), Err(PathError::Symlink));
        assert_eq!(
            p.check_write("sneaky/hooks/pre-commit"),
            Err(PathError::Protected(".git".into()))
        );
        // A link that stays inside the root is fine for its children.
        assert!(p.check_write("inside/x.txt").is_ok());
    }

    #[test]
    fn case_collisions_are_refused_when_case_insensitive() {
        let tmp = tempfile::tempdir().expect("tmp");
        fs::create_dir_all(tmp.path().join("Docs")).expect("dir");
        fs::write(tmp.path().join("Docs/Readme.md"), "x").expect("f");
        let p = policy(tmp.path());
        assert_eq!(
            p.check_write("docs/new.md"),
            Err(PathError::CaseCollision("Docs".into()))
        );
        assert_eq!(
            p.check_write("Docs/README.md"),
            Err(PathError::CaseCollision("Readme.md".into()))
        );
        assert!(p.check_write("Docs/Readme.md").is_ok());
        assert!(p.check_write("Docs/other.md").is_ok());
        let sensitive = PathPolicy::new(tmp.path())
            .expect("p")
            .with_case_insensitive(false);
        assert!(sensitive.check_write("docs/new.md").is_ok());
    }

    #[test]
    fn size_caps_apply_per_kind() {
        let tmp = tempfile::tempdir().expect("tmp");
        let p = policy(tmp.path()).with_caps(10, 100);
        assert!(p.check_size(NodeKind::Text, 10).is_ok());
        assert_eq!(
            p.check_size(NodeKind::Text, 11),
            Err(PathError::TooLarge {
                size: 11,
                limit: 10
            })
        );
        assert!(p.check_size(NodeKind::Blob, 100).is_ok());
        assert!(p.check_size(NodeKind::Blob, 101).is_err());
        assert!(p.check_size(NodeKind::Dir, 1).is_err());
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        fn piece() -> impl Strategy<Value = String> {
            prop_oneof![
                Just("..".to_owned()),
                Just(".".to_owned()),
                Just("".to_owned()),
                Just("a".to_owned()),
                Just("b.txt".to_owned()),
                Just(".git".to_owned()),
                Just("C:".to_owned()),
                Just("escape".to_owned()),
                Just("inside".to_owned()),
                "[a-zA-Z0-9 ._:\\\\*?-]{0,8}",
            ]
        }

        proptest! {
            /// No accepted path resolves outside the project root, whatever
            /// the (hostile) input and whatever links the project contains.
            #[test]
            fn accepted_paths_stay_inside_the_root(
                parts in proptest::collection::vec(piece(), 1..6),
                sep in prop_oneof![Just("/"), Just("//"), Just("\\")],
                lead in prop_oneof![Just(""), Just("/")],
            ) {
                let tmp = tempfile::tempdir().expect("tmp");
                let outside = tempfile::tempdir().expect("outside");
                let root = tmp.path().join("proj");
                fs::create_dir_all(root.join("inside")).expect("dirs");
                #[cfg(unix)]
                {
                    let _ = std::os::unix::fs::symlink(outside.path(), root.join("escape"));
                }
                let p = policy(&root);
                let raw = format!("{lead}{}", parts.join(sep));
                if let Ok(safe) = p.check_write(&raw) {
                    let mut probe = safe.abs.clone();
                    while !probe.exists() && probe.pop() {}
                    let canon = probe.canonicalize().expect("canon");
                    prop_assert!(canon.starts_with(p.root().path()), "{raw:?} -> {canon:?}");
                    prop_assert!(raw.split('/').all(|c| c != ".."));
                    prop_assert!(!raw.starts_with('/'));
                }
                drop(outside);
            }

            #[test]
            fn lexical_check_never_panics(raw in "\\PC{0,64}") {
                let _ = check_lexical(&raw);
            }
        }
    }
}
