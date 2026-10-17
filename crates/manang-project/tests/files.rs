//! Behaviour of the file, tree and scaffold primitives on real temp dirs.

use std::fs;

use manang_project::fsops;
use manang_project::scaffold::{self, NewProject};
use manang_project::tree::{self, EntryKind, TreeQuery};
use manang_project::{Engine, ProjectConfig, ProjectError, ProjectRoot};

fn root() -> (tempfile::TempDir, ProjectRoot) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = ProjectRoot::open(tmp.path()).expect("root");
    (tmp, root)
}

#[test]
fn write_then_read_round_trips_and_creates_parents() {
    let (_t, root) = root();
    assert_eq!(
        fsops::write_text(&root, "a/b/c.typ", "héllo").expect("w"),
        6
    );
    assert_eq!(fsops::read_text(&root, "a/b/c.typ").expect("r"), "héllo");
    fsops::write_text(&root, "a/b/c.typ", "second").expect("overwrite");
    assert_eq!(fsops::read_text(&root, "a/b/c.typ").expect("r"), "second");
}

#[test]
fn atomic_write_leaves_no_temp_files_and_preserves_mode() {
    let (_t, root) = root();
    fsops::write_text(&root, "run.sh", "#!/bin/sh\n").expect("w");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = root.path().join("run.sh");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o750)).expect("chmod");
        fsops::write_text(&root, "run.sh", "#!/bin/sh\necho\n").expect("w2");
        let mode = fs::metadata(&p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o750);
    }
    let names: Vec<_> = fs::read_dir(root.path())
        .expect("dir")
        .map(|e| e.expect("e").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["run.sh"]);
}

#[test]
fn read_rejects_non_utf8_missing_and_directories() {
    let (_t, root) = root();
    fs::write(root.path().join("bin.dat"), [0xff, 0xfe, 0x00]).expect("w");
    fs::create_dir(root.path().join("d")).expect("d");
    assert!(matches!(
        fsops::read_text(&root, "bin.dat"),
        Err(ProjectError::NotUtf8(_))
    ));
    assert_eq!(
        fsops::read_bytes(&root, "bin.dat").expect("bytes"),
        [0xff, 0xfe, 0]
    );
    assert!(matches!(
        fsops::read_text(&root, "nope"),
        Err(ProjectError::NotFound(_))
    ));
    assert!(matches!(
        fsops::read_text(&root, "d"),
        Err(ProjectError::NotAFile(_))
    ));
    assert!(matches!(
        fsops::write_text(&root, "d", "x"),
        Err(ProjectError::NotAFile(_))
    ));
}

#[test]
fn rename_moves_refuses_overwrite_and_self_nesting() {
    let (_t, root) = root();
    fsops::write_text(&root, "a.txt", "A").expect("w");
    fsops::write_text(&root, "b.txt", "B").expect("w");
    fsops::write_text(&root, "dir/x.txt", "X").expect("w");
    fsops::rename(&root, "a.txt", "sub/renamed.txt").expect("rename");
    assert_eq!(fsops::read_text(&root, "sub/renamed.txt").expect("r"), "A");
    assert!(matches!(
        fsops::rename(&root, "b.txt", "sub/renamed.txt"),
        Err(ProjectError::Exists(_))
    ));
    assert!(matches!(
        fsops::rename(&root, "missing", "z"),
        Err(ProjectError::NotFound(_))
    ));
    assert!(fsops::rename(&root, "dir", "dir/inner").is_err());
    fsops::rename(&root, "dir", "moved").expect("dir rename");
    assert_eq!(fsops::read_text(&root, "moved/x.txt").expect("r"), "X");
}

#[test]
fn delete_goes_to_a_recoverable_trash() {
    let (_t, root) = root();
    fsops::write_text(&root, "figs/a.svg", "<svg/>").expect("w");
    let trashed = fsops::trash(&root, "figs").expect("trash");
    assert!(trashed.starts_with(".manang/trash/") && trashed.ends_with("/figs"));
    assert!(!root.path().join("figs").exists());
    assert_eq!(
        fs::read_to_string(root.path().join(&trashed).join("a.svg")).expect("recoverable"),
        "<svg/>"
    );
    // The same name can be trashed again without clobbering the first.
    fsops::write_text(&root, "figs/a.svg", "v2").expect("w");
    let second = fsops::trash(&root, "figs").expect("trash again");
    assert_ne!(trashed, second);
    assert!(matches!(
        fsops::trash(&root, "figs"),
        Err(ProjectError::NotFound(_))
    ));
}

fn names(page: &tree::TreePage) -> Vec<&str> {
    page.entries.iter().map(|e| e.path.as_str()).collect()
}

#[test]
fn tree_respects_gitignore_hides_internals_and_sorts_dirs_first() {
    let (_t, root) = root();
    for (p, c) in [
        (".gitignore", "build/\n*.aux\n"),
        ("main.typ", ""),
        ("Zed.txt", ""),
        ("build/out.pdf", ""),
        ("notes.aux", ""),
        ("figures/a.svg", ""),
        (".git/HEAD", ""),
        (".manang/cache/x", ""),
        ("sub/.git/HEAD", ""),
    ] {
        fsops::write_text(&root, p, c).ok();
    }
    // .git writes are refused above, so create the internals directly.
    for p in [".git/HEAD", "sub/.git/HEAD", ".manang/cache/x"] {
        let path = root.path().join(p);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, "").expect("write");
    }
    let page = tree::list(&root, &TreeQuery::default()).expect("list");
    assert_eq!(
        names(&page),
        vec!["figures", "sub", ".gitignore", "main.typ", "Zed.txt"]
    );
    assert_eq!(page.entries[0].kind, EntryKind::Dir);

    let all = tree::list(
        &root,
        &TreeQuery {
            recursive: true,
            ..TreeQuery::default()
        },
    )
    .expect("recursive");
    assert_eq!(
        names(&all),
        vec![
            ".gitignore",
            "figures",
            "figures/a.svg",
            "main.typ",
            "sub",
            "Zed.txt"
        ]
    );
    let sub = tree::list(
        &root,
        &TreeQuery {
            path: "figures".to_owned(),
            ..TreeQuery::default()
        },
    )
    .expect("sub");
    assert_eq!(names(&sub), vec!["figures/a.svg"]);
}

#[test]
fn tree_pages_and_validates_the_directory() {
    let (_t, root) = root();
    for i in 0..25 {
        fs::write(root.path().join(format!("f{i:02}.txt")), "").expect("w");
    }
    let q = |offset, limit| TreeQuery {
        offset,
        limit,
        ..TreeQuery::default()
    };
    let p1 = tree::list(&root, &q(0, 10)).expect("p1");
    assert_eq!((p1.entries.len(), p1.total, p1.has_more), (10, 25, true));
    let p3 = tree::list(&root, &q(20, 10)).expect("p3");
    assert_eq!((p3.entries.len(), p3.has_more), (5, false));
    assert_eq!(p3.entries[0].name, "f20.txt");
    let beyond = tree::list(&root, &q(999, 10)).expect("beyond");
    assert!(beyond.entries.is_empty() && !beyond.has_more);
    assert!(matches!(
        tree::list(
            &root,
            &TreeQuery {
                path: "f00.txt".to_owned(),
                ..TreeQuery::default()
            }
        ),
        Err(ProjectError::NotADirectory(_))
    ));
    assert!(matches!(
        tree::list(
            &root,
            &TreeQuery {
                path: "nope".to_owned(),
                ..TreeQuery::default()
            }
        ),
        Err(ProjectError::NotFound(_))
    ));
}

#[cfg(unix)]
#[test]
fn tree_does_not_descend_into_symlinked_directories() {
    let tmp = tempfile::tempdir().expect("tmp");
    let outside = tmp.path().join("outside");
    let project = tmp.path().join("project");
    fs::create_dir_all(&outside).expect("o");
    fs::create_dir_all(&project).expect("p");
    fs::write(outside.join("secret.txt"), "").expect("s");
    std::os::unix::fs::symlink(&outside, project.join("out")).expect("link");
    let root = ProjectRoot::open(&project).expect("root");
    let all = tree::list(
        &root,
        &TreeQuery {
            recursive: true,
            ..TreeQuery::default()
        },
    )
    .expect("list");
    assert_eq!(names(&all), vec!["out"]);
    assert_eq!(all.entries[0].kind, EntryKind::Symlink);
    assert!(
        tree::list(
            &root,
            &TreeQuery {
                path: "out".to_owned(),
                ..TreeQuery::default()
            }
        )
        .is_err()
    );
}

#[test]
fn scaffold_creates_a_valid_project_for_every_engine() {
    for engine in Engine::ALL {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("paper");
        let config = scaffold::create(
            &dir,
            &NewProject {
                name: "R&D #1 \"x\"",
                engine,
                template: Some("ieee"),
            },
        )
        .expect("create");
        let on_disk = fs::read_to_string(dir.join("manang.toml")).expect("toml");
        let parsed = ProjectConfig::parse(&on_disk).expect("valid");
        assert_eq!(parsed, config);
        assert!(dir.join(parsed.main_file()).is_file(), "{engine:?} main");
        assert!(dir.join("references.bib").is_file());
        assert!(
            fs::read_to_string(dir.join(".gitignore"))
                .expect("gi")
                .contains(".manang/")
        );
    }
}

#[test]
fn scaffold_refuses_non_empty_dirs_and_bad_names() {
    let tmp = tempfile::tempdir().expect("tmp");
    fs::write(tmp.path().join("keep.txt"), "").expect("w");
    let new = NewProject {
        name: "x",
        engine: Engine::Typst,
        template: None,
    };
    assert!(matches!(
        scaffold::create(tmp.path(), &new),
        Err(ProjectError::Exists(_))
    ));
    assert!(matches!(
        scaffold::create(
            &tmp.path().join("fresh"),
            &NewProject {
                name: " ",
                ..new.clone()
            }
        ),
        Err(ProjectError::Config(_))
    ));
    assert!(!tmp.path().join("fresh").exists());
    // An existing empty directory is fine.
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).expect("mkdir");
    assert!(scaffold::create(&empty, &new).is_ok());
}
