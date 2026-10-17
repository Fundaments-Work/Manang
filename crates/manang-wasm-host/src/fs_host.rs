//! Scoped, escape-proof file access for plugins.
//!
//! Every operation resolves its path in two steps:
//!
//! 1. A purely lexical check ([`split_path`]) rejects absolute paths, `..`,
//!    empty segments, backslashes, drive prefixes and NUL.
//! 2. The path must lie under a granted scope. The scope is then opened as a
//!    [`cap_std::fs::Dir`] *beneath the project root* and the remainder of the
//!    path is opened *beneath that scope directory*. `cap-std` resolves each
//!    component without ever leaving the directory handle it was given, so a
//!    symlink (or a race that swaps one in) cannot point outside the scope.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use manang_kernel::{Access, CapabilitySet};

use crate::limits::Limits;

/// Most directory entries `list-dir` returns.
const MAX_DIR_ENTRIES: usize = 10_000;

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Lexically validate a plugin-supplied relative path into its components.
///
/// `"."` segments are dropped, so `"."` alone yields no components (the scope
/// root).
pub(crate) fn split_path(path: &str) -> Result<Vec<&str>, String> {
    if path.contains(['\\', '\0', ':']) {
        return Err(format!("path `{path}` contains a forbidden character"));
    }
    if path.starts_with('/') {
        return Err(format!("path `{path}` must be relative"));
    }
    let mut parts = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" if path.is_empty() => {}
            "" => return Err(format!("path `{path}` has an empty segment")),
            "." => {}
            ".." => return Err(format!("path `{path}` must not contain `..`")),
            other => parts.push(other),
        }
    }
    Ok(parts)
}

/// A project directory plus the grants that apply to one call.
pub(crate) struct FsHost<'a> {
    pub root: &'a Path,
    pub capabilities: &'a CapabilitySet,
    pub limits: &'a Limits,
}

/// A scope directory handle and the path remaining beneath it.
struct Resolved {
    dir: Dir,
    rest: PathBuf,
}

impl FsHost<'_> {
    fn resolve(&self, path: &str, access: Access) -> Result<Resolved, String> {
        let parts = split_path(path)?;
        // Longest granted scope that is a component-wise prefix of the path.
        let mut best: Option<Vec<&str>> = None;
        for scope in self.capabilities.filesystem_scopes(access) {
            let scope_parts = split_path(scope)?;
            let covers =
                parts.len() >= scope_parts.len() && parts[..scope_parts.len()] == scope_parts[..];
            if covers && best.as_ref().is_none_or(|b| scope_parts.len() > b.len()) {
                best = Some(scope_parts);
            }
        }
        let Some(scope_parts) = best else {
            let verb = match access {
                Access::Read => "read",
                Access::Write => "write",
            };
            return Err(format!("no granted `{verb}` scope covers `{path}`"));
        };
        let root = Dir::open_ambient_dir(self.root, ambient_authority())
            .map_err(|e| format!("project directory is unavailable: {e}"))?;
        let scope_path: PathBuf = scope_parts.iter().collect();
        let dir = if scope_parts.is_empty() {
            root
        } else {
            if access == Access::Write {
                root.create_dir_all(&scope_path)
                    .map_err(|e| format!("cannot create the write scope: {e}"))?;
            }
            root.open_dir(&scope_path)
                .map_err(|e| format!("cannot open the scope directory: {e}"))?
        };
        let rest: PathBuf = parts[scope_parts.len()..].iter().collect();
        Ok(Resolved { dir, rest })
    }

    fn spend(&self, budget: &mut usize, bytes: usize) -> Result<(), String> {
        if bytes > *budget {
            return Err(format!(
                "call exceeds the {} byte file I/O budget",
                self.limits.max_fs_call_bytes
            ));
        }
        *budget -= bytes;
        Ok(())
    }

    pub fn read_file(&self, path: &str, budget: &mut usize) -> Result<Vec<u8>, String> {
        let r = self.resolve(path, Access::Read)?;
        if r.rest.as_os_str().is_empty() {
            return Err(format!("`{path}` is a directory"));
        }
        // Checked before open so a FIFO or device cannot block the call.
        let meta = r
            .dir
            .metadata(&r.rest)
            .map_err(|e| format!("cannot stat `{path}`: {e}"))?;
        if !meta.is_file() {
            return Err(format!("`{path}` is not a regular file"));
        }
        let max = self.limits.max_fs_file_bytes;
        if meta.len() > max as u64 {
            return Err(format!("`{path}` is larger than the {max} byte file limit"));
        }
        let file = r
            .dir
            .open(&r.rest)
            .map_err(|e| format!("cannot open `{path}`: {e}"))?;
        let mut out = Vec::new();
        file.take(max as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| format!("cannot read `{path}`: {e}"))?;
        if out.len() > max {
            return Err(format!("`{path}` is larger than the {max} byte file limit"));
        }
        self.spend(budget, out.len())?;
        Ok(out)
    }

    pub fn write_file(&self, path: &str, data: &[u8], budget: &mut usize) -> Result<(), String> {
        if data.len() > self.limits.max_fs_file_bytes {
            return Err(format!(
                "write of {} bytes exceeds the {} byte file limit",
                data.len(),
                self.limits.max_fs_file_bytes
            ));
        }
        self.spend(budget, data.len())?;
        let r = self.resolve(path, Access::Write)?;
        if r.rest.as_os_str().is_empty() {
            return Err(format!("`{path}` is a directory"));
        }
        if let Some(parent) = r.rest.parent().filter(|p| !p.as_os_str().is_empty()) {
            r.dir
                .create_dir_all(parent)
                .map_err(|e| format!("cannot create directories for `{path}`: {e}"))?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = r
            .dir
            .open_with(&r.rest, &options)
            .map_err(|e| format!("cannot open `{path}` for writing: {e}"))?;
        file.write_all(data)
            .map_err(|e| format!("cannot write `{path}`: {e}"))
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<Entry>, String> {
        let r = self.resolve(path, Access::Read)?;
        let rest = if r.rest.as_os_str().is_empty() {
            Path::new(".")
        } else {
            r.rest.as_path()
        };
        let read = r
            .dir
            .read_dir(rest)
            .map_err(|e| format!("cannot list `{path}`: {e}"))?;
        let mut out = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| format!("cannot list `{path}`: {e}"))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let meta = entry
                .metadata()
                .map_err(|e| format!("cannot stat `{name}`: {e}"))?;
            out.push(Entry {
                name,
                is_dir: meta.is_dir(),
                size: meta.len(),
            });
            if out.len() >= MAX_DIR_ENTRIES {
                return Err(format!("`{path}` has more than {MAX_DIR_ENTRIES} entries"));
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use manang_kernel::Manifest;

    use super::*;

    fn caps(permissions: &str) -> CapabilitySet {
        Manifest::from_toml(&format!(
            "[plugin]\nname = \"t\"\nversion = \"0.1.0\"\n[permissions]\n{permissions}"
        ))
        .expect("manifest")
        .capabilities()
        .expect("caps")
    }

    fn host<'a>(root: &'a Path, caps: &'a CapabilitySet, limits: &'a Limits) -> FsHost<'a> {
        FsHost {
            root,
            capabilities: caps,
            limits,
        }
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path();
        std::fs::create_dir_all(p.join("templates/sub")).expect("mkdir");
        std::fs::create_dir_all(p.join("secret")).expect("mkdir");
        std::fs::write(p.join("templates/a.typ"), b"hello").expect("write");
        std::fs::write(p.join("templates/sub/b.typ"), b"nested").expect("write");
        std::fs::write(p.join("secret/key.txt"), b"s3cret").expect("write");
        dir
    }

    #[test]
    fn lexical_validation_rejects_hostile_paths() {
        for bad in [
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a//b",
            "a/",
            "a\\b",
            "C:/x",
            "a\0b",
            "templates/../secret/key.txt",
        ] {
            assert!(split_path(bad).is_err(), "{bad:?}");
        }
        assert_eq!(split_path("a/./b").expect("ok"), vec!["a", "b"]);
        assert!(split_path(".").expect("ok").is_empty());
    }

    #[test]
    fn reads_inside_a_granted_scope_only() {
        let dir = project();
        let caps = caps("filesystem = [\"read:templates\"]\n");
        let limits = Limits::default();
        let fs = host(dir.path(), &caps, &limits);
        let mut budget = limits.max_fs_call_bytes;
        assert_eq!(
            fs.read_file("templates/a.typ", &mut budget).expect("ok"),
            b"hello"
        );
        assert_eq!(
            fs.read_file("templates/sub/b.typ", &mut budget)
                .expect("ok"),
            b"nested"
        );
        assert!(fs.read_file("secret/key.txt", &mut budget).is_err());
        assert!(
            fs.read_file("templates/../secret/key.txt", &mut budget)
                .is_err()
        );
        assert!(fs.read_file("/etc/passwd", &mut budget).is_err());
        // Read grant does not allow writing.
        assert!(
            fs.write_file("templates/new.typ", b"x", &mut budget)
                .is_err()
        );
        assert!(!dir.path().join("templates/new.typ").exists());
    }

    #[test]
    fn write_scope_creates_parents_and_replaces() {
        let dir = project();
        let caps = caps("filesystem = [\"write:out/gen\"]\n");
        let limits = Limits::default();
        let fs = host(dir.path(), &caps, &limits);
        let mut budget = limits.max_fs_call_bytes;
        fs.write_file("out/gen/x/y.txt", b"one", &mut budget)
            .expect("write");
        fs.write_file("out/gen/x/y.txt", b"two", &mut budget)
            .expect("rewrite");
        assert_eq!(
            std::fs::read(dir.path().join("out/gen/x/y.txt")).expect("read"),
            b"two"
        );
        // Write implies read inside the scope.
        assert_eq!(
            fs.read_file("out/gen/x/y.txt", &mut budget).expect("ok"),
            b"two"
        );
        // Siblings of the scope are out of bounds.
        assert!(fs.write_file("out/other.txt", b"x", &mut budget).is_err());
        assert!(fs.write_file("out/gen", b"x", &mut budget).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_the_scope() {
        use std::os::unix::fs::symlink;
        let dir = project();
        let p = dir.path();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("loot.txt"), b"loot").expect("write");
        // Absolute and relative file links, and a directory link, all pointing out.
        symlink(p.join("secret/key.txt"), p.join("templates/abs")).expect("symlink");
        symlink("../secret/key.txt", p.join("templates/rel")).expect("symlink");
        symlink(outside.path(), p.join("templates/dir")).expect("symlink");
        // A link inside the scope pointing inside the scope is fine.
        symlink("a.typ", p.join("templates/ok")).expect("symlink");
        // The scope directory itself is a link out of the project.
        symlink(outside.path(), p.join("linked-scope")).expect("symlink");

        let caps = caps("filesystem = [\"write:templates\", \"read:linked-scope\"]\n");
        let limits = Limits::default();
        let fs = host(p, &caps, &limits);
        let mut budget = limits.max_fs_call_bytes;
        for path in ["templates/abs", "templates/rel", "templates/dir/loot.txt"] {
            assert!(fs.read_file(path, &mut budget).is_err(), "read {path}");
        }
        assert!(
            fs.write_file("templates/dir/new.txt", b"x", &mut budget)
                .is_err()
        );
        assert!(!outside.path().join("new.txt").exists());
        assert!(
            fs.write_file("templates/abs", b"overwrite", &mut budget)
                .is_err()
        );
        assert_eq!(
            std::fs::read(p.join("secret/key.txt")).expect("read"),
            b"s3cret"
        );
        assert!(fs.read_file("linked-scope/loot.txt", &mut budget).is_err());
        assert!(fs.list_dir("linked-scope").is_err());
        assert_eq!(
            fs.read_file("templates/ok", &mut budget).expect("ok"),
            b"hello"
        );
    }

    #[test]
    fn size_and_budget_limits_apply() {
        let dir = project();
        std::fs::write(dir.path().join("templates/big.bin"), vec![0u8; 100]).expect("write");
        let caps = caps("filesystem = [\"write:templates\"]\n");
        let limits = Limits {
            max_fs_file_bytes: 64,
            max_fs_call_bytes: 100,
            ..Limits::default()
        };
        let fs = host(dir.path(), &caps, &limits);
        let mut budget = limits.max_fs_call_bytes;
        assert!(fs.read_file("templates/big.bin", &mut budget).is_err());
        assert!(
            fs.write_file("templates/w.bin", &[0u8; 65], &mut budget)
                .is_err()
        );
        fs.write_file("templates/w.bin", &[0u8; 60], &mut budget)
            .expect("fits");
        assert!(
            fs.write_file("templates/w2.bin", &[0u8; 60], &mut budget)
                .is_err()
        );
    }

    #[test]
    fn lists_sorted_entries() {
        let dir = project();
        let caps = caps("filesystem = [\"read:templates\"]\n");
        let limits = Limits::default();
        let fs = host(dir.path(), &caps, &limits);
        let names: Vec<_> = fs
            .list_dir("templates")
            .expect("list")
            .into_iter()
            .map(|e| (e.name, e.is_dir))
            .collect();
        assert_eq!(
            names,
            vec![("a.typ".to_owned(), false), ("sub".to_owned(), true)]
        );
        assert!(fs.list_dir("secret").is_err());
    }
}
