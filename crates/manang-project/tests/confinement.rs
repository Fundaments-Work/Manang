//! No path, however hostile, may resolve or write outside the project root.

use std::fs;
use std::path::Path;

use manang_project::fsops;
use manang_project::{ProjectError, ProjectRoot};
use proptest::prelude::*;

/// A project containing symlinks that point outside, to `/`, to themselves and
/// back inside, next to a sibling "outside" directory.
struct Fixture {
    _tmp: tempfile::TempDir,
    outside: std::path::PathBuf,
    root: ProjectRoot,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let outside = tmp.path().join("outside");
    let project = tmp.path().join("project");
    fs::create_dir_all(&outside).expect("outside");
    fs::create_dir_all(project.join("inner/deep")).expect("project");
    fs::write(outside.join("secret.txt"), "secret").expect("secret");
    fs::write(project.join("a"), "a").expect("a");
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&outside, project.join("out")).expect("out");
        symlink("/", project.join("abs")).expect("abs");
        symlink("loop", project.join("loop")).expect("loop");
        symlink("../outside/secret.txt", project.join("link")).expect("link");
        symlink("..", project.join("up")).expect("up");
        symlink("missing-target", project.join("dangling")).expect("dangling");
        symlink("../..", project.join("inner/deep/back")).expect("back");
        symlink("inner", project.join("inside")).expect("inside");
    }
    let root = ProjectRoot::open(&project).expect("root");
    Fixture {
        _tmp: tmp,
        outside,
        root,
    }
}

fn segment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("..".to_owned()),
        Just(".".to_owned()),
        Just(String::new()),
        Just("a".to_owned()),
        Just("out".to_owned()),
        Just("abs".to_owned()),
        Just("loop".to_owned()),
        Just("link".to_owned()),
        Just("up".to_owned()),
        Just("dangling".to_owned()),
        Just("inner".to_owned()),
        Just("deep".to_owned()),
        Just("back".to_owned()),
        Just("inside".to_owned()),
        Just("secret.txt".to_owned()),
        Just("etc".to_owned()),
        Just(".git".to_owned()),
        Just("é日本\u{202e}語".to_owned()),
        Just("C:".to_owned()),
        Just("a\\b".to_owned()),
        Just("x".repeat(300)),
        "[a-zA-Z0-9._ -]{0,12}",
        "\\PC{0,8}",
    ]
}

fn raw_path() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just(""), Just("/"), Just("//"), Just("/etc/"), Just("./")],
        proptest::collection::vec(segment(), 0..8),
    )
        .prop_map(|(prefix, segments)| format!("{prefix}{}", segments.join("/")))
}

fn assert_inside(root: &ProjectRoot, result: Result<std::path::PathBuf, ProjectError>) {
    if let Ok(path) = result {
        assert!(
            path.starts_with(root.path()),
            "{} escaped {}",
            path.display(),
            root.path().display()
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn resolvers_never_return_a_path_outside_the_root(raw in raw_path()) {
        let f = fixture();
        assert_inside(&f.root, f.root.resolve(&raw));
        assert_inside(&f.root, f.root.resolve_entry(&raw));
        assert_inside(&f.root, f.root.resolve_for_write(&raw));
        assert_inside(&f.root, f.root.resolve_for_move(&raw));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn mutations_never_touch_the_outside(raw in raw_path(), other in raw_path()) {
        let f = fixture();
        let _ = fsops::write_text(&f.root, &raw, "pwned");
        let _ = fsops::rename(&f.root, &raw, &other);
        let _ = fsops::trash(&f.root, &raw);
        let _ = fsops::read_text(&f.root, &raw);
        let secret = fs::read_to_string(f.outside.join("secret.txt")).expect("secret");
        prop_assert_eq!(secret, "secret");
        let entries: Vec<_> = fs::read_dir(&f.outside).expect("read_dir").collect();
        prop_assert_eq!(entries.len(), 1);
    }
}

#[test]
fn known_escapes_are_refused() {
    let f = fixture();
    for raw in [
        "../outside/secret.txt",
        "/etc/passwd",
        "a/../../outside",
        "..",
    ] {
        assert!(f.root.resolve(raw).is_err(), "{raw}");
    }
    #[cfg(unix)]
    for raw in [
        "out",
        "out/secret.txt",
        "out/new.txt",
        "abs/etc/passwd",
        "link",
        "up/outside/secret.txt",
        "inner/deep/back/../outside",
        "loop",
        "dangling",
    ] {
        assert!(f.root.resolve(raw).is_err(), "{raw} should be refused");
        assert!(
            fsops::write_text(&f.root, raw, "x").is_err(),
            "{raw} write should be refused"
        );
    }
}

#[cfg(unix)]
#[test]
fn symlinks_that_stay_inside_work_and_links_are_moved_not_followed() {
    let f = fixture();
    assert!(f.root.resolve("inside/deep").is_ok());
    fsops::write_text(&f.root, "inside/note.txt", "hi").expect("write via inner link");
    assert_eq!(
        fs::read_to_string(f.root.path().join("inner/note.txt")).expect("read"),
        "hi"
    );
    // Deleting an escaping symlink removes the link, never the target.
    fsops::trash(&f.root, "out").expect("trash link");
    assert!(f.outside.join("secret.txt").exists());
    assert!(!Path::new(&f.root.path().join("out")).exists());
}

#[test]
fn git_dir_and_root_are_protected() {
    let f = fixture();
    fs::create_dir_all(f.root.path().join(".git")).expect(".git");
    assert!(matches!(
        fsops::write_text(&f.root, ".git/hooks/pre-commit", "#!/bin/sh"),
        Err(ProjectError::Protected(_))
    ));
    assert!(matches!(
        fsops::write_text(&f.root, ".GIT/config", ""),
        Err(ProjectError::Protected(_))
    ));
    assert!(matches!(
        fsops::trash(&f.root, ""),
        Err(ProjectError::Protected(_))
    ));
    assert!(matches!(
        fsops::trash(&f.root, ".manang"),
        Err(ProjectError::Protected(_))
    ));
    assert!(matches!(
        fsops::rename(&f.root, "a", ".git/a"),
        Err(ProjectError::Protected(_))
    ));
}
