#![no_main]

//! `pathsec::check_lexical` and `PathPolicy::check_rel` on arbitrary strings.
//! An accepted path is relative, never contains `..`, `\`, an empty component
//! or a drive prefix, never touches a protected name, and never resolves
//! outside the root when joined to it.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use manang_live_plugin::pathsec::{DENY_LIST, PathPolicy, check_lexical};

/// A policy over the fuzz crate's own directory (read-only: `check_rel` does
/// no file system access beyond loading the root `.gitignore`).
fn policy() -> &'static PathPolicy {
    static POLICY: OnceLock<PathPolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        PathPolicy::new(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("policy over the fuzz directory")
            .with_case_insensitive(true)
    })
}

/// Lexically resolve `root/rel` (no file system) and report whether it stays
/// inside `root`.
fn stays_inside(root: &Path, rel: &str) -> bool {
    let mut out = PathBuf::from(root);
    let depth = root.components().count();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    out.starts_with(root) && out.components().count() > depth
}

fuzz_target!(|data: &[u8]| {
    let Ok(rel) = std::str::from_utf8(data) else {
        return;
    };
    let lexical = check_lexical(rel);
    let policy = policy();
    if policy.check_rel(rel).is_ok() {
        assert!(lexical.is_ok(), "check_rel accepted what check_lexical refused");
    }
    if lexical.is_err() {
        return;
    }
    assert!(!rel.is_empty());
    assert!(!rel.starts_with('/'));
    assert!(!rel.contains('\\') && !rel.contains('\0'));
    assert!(!Path::new(rel).has_root() && !Path::new(rel).is_absolute());
    assert!(!rel.contains(':'), "colon (drive prefix or stream) accepted: {rel:?}");
    for part in rel.split('/') {
        assert!(!part.is_empty() && part != "." && part != "..", "{rel:?}");
        assert!(!part.starts_with('.'), "dotfile accepted: {rel:?}");
        assert!(!part.eq_ignore_ascii_case("manang.toml"), "{rel:?}");
        assert!(!part.ends_with(['.', ' ']), "{rel:?}");
        assert!(
            !DENY_LIST.iter().any(|d| part.eq_ignore_ascii_case(d)),
            "deny-listed directory accepted: {rel:?}"
        );
        assert!(
            !part.chars().any(|c| c.is_control() || ":*?\"<>|".contains(c)),
            "{rel:?}"
        );
    }
    assert!(stays_inside(policy.root().path(), rel), "{rel:?} escapes the root");
});
