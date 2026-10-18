//! Install, approve, update, enable, disable, uninstall: the plugin manager's
//! lifecycle, on temporary directories with tiny real components.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::manifest::Permissions;
use manang_kernel::{Context, Kernel, Manifest, Payload, Plugin, PluginError};
use manang_plugins::{
    ManagerConfig, ManagerError, ManagerEvent, PluginManager, PluginStatus, PolicyDenied,
    PolicyGate, PolicyRequest,
};
use manang_wasm_host::Limits;
use parking_lot::Mutex;
use serde_json::json;

mod common;
use common::{archive, component, folder, manifest_with};

type Events = Arc<Mutex<Vec<ManagerEvent>>>;

struct Env {
    tmp: tempfile::TempDir,
    kernel: Arc<Kernel>,
    manager: Arc<PluginManager>,
    events: Events,
}

impl Env {
    fn root(&self) -> &Path {
        self.tmp.path()
    }
    fn plugins(&self) -> PathBuf {
        self.root().join("plugins")
    }
    fn trash(&self) -> PathBuf {
        self.root().join("trash")
    }
    fn src(&self) -> PathBuf {
        let dir = self.root().join("src");
        std::fs::create_dir_all(&dir).expect("src dir");
        dir
    }
    fn answer(&self, name: &str) -> Option<Payload> {
        self.kernel
            .invoke(&format!("{name}.run"), Payload::Empty)
            .ok()
    }
    fn status(&self, name: &str) -> PluginStatus {
        self.manager.state_of(name).expect("state").status
    }
}

fn config(root: &Path, events: &Events) -> ManagerConfig {
    let mut config = ManagerConfig::new(root.join("plugins"));
    config.registry_path = Some(root.join("config").join("plugins.toml"));
    config.trash_dir = Some(root.join("trash"));
    config.limits = Limits {
        deadline: Duration::from_secs(20),
        ..Limits::default()
    };
    let sink = Arc::clone(events);
    config.listener = Some(Arc::new(move |e| sink.lock().push(e.clone())));
    config
}

fn start_in(tmp: tempfile::TempDir) -> Env {
    let events: Events = Arc::default();
    let kernel = Arc::new(Kernel::new());
    let manager = Arc::new(
        PluginManager::new(Arc::clone(&kernel), config(tmp.path(), &events)).expect("manager"),
    );
    Env {
        tmp,
        kernel,
        manager,
        events,
    }
}

fn env() -> Env {
    start_in(tempfile::tempdir().expect("tempdir"))
}

/// Start the app again over the same directories.
fn restart(old: Env) -> Env {
    let Env { tmp, .. } = old;
    start_in(tmp)
}

fn perms(network: &[&str]) -> Permissions {
    Permissions {
        network: network.iter().map(|s| (*s).to_owned()).collect(),
        ..Permissions::default()
    }
}

/// A source folder for `name` asking for network access to `hosts`.
fn source(env: &Env, name: &str, version: &str, hosts: &[&str], answer: char) -> PathBuf {
    folder(
        &env.src(),
        name,
        &manifest_with(name, version, hosts),
        &component(answer, 1),
    )
}

fn install_and_approve(env: &Env, name: &str, hosts: &[&str]) {
    let src = source(env, name, "0.1.0", hosts, '1');
    env.manager.install(&src).expect("install");
    env.manager.approve(name, &perms(hosts)).expect("approve");
}

fn leftovers(env: &Env) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(env.plugins())
        .expect("read plugins")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn install_from_folder_waits_for_approval_then_loads() {
    let env = env();
    let src = source(&env, "alpha", "0.1.0", &["example.org"], '1');
    let state = env.manager.install(&src).expect("install");

    assert_eq!(state.status, PluginStatus::NeedsApproval);
    assert_eq!(state.permissions, perms(&["example.org"]));
    assert_eq!(state.new_permissions, perms(&["example.org"]));
    assert!(state.running_version.is_none());
    assert!(
        !env.kernel.has_command("alpha.run"),
        "never loaded unapproved"
    );
    assert_eq!(leftovers(&env), ["alpha"], "no staging directory is left");
    assert_eq!(state.source.as_deref(), Some("alpha"));

    // Asking for more than the plugin requested is refused.
    let err = env
        .manager
        .approve("alpha", &perms(&["evil.example"]))
        .expect_err("not requested");
    assert!(matches!(err, ManagerError::PermissionMismatch(_)), "{err}");
    assert!(!env.kernel.has_command("alpha.run"));

    let state = env
        .manager
        .approve("alpha", &perms(&["example.org"]))
        .expect("approve");
    assert_eq!(state.status, PluginStatus::Loaded);
    assert_eq!(state.granted, perms(&["example.org"]));
    assert_eq!(state.running_version.as_deref(), Some("0.1.0"));
    assert_eq!(env.answer("alpha"), Some(Payload::Json(json!(1))));
    assert!(state.installed_at.is_some());
    assert!(matches!(
        env.manager.approve("alpha", &perms(&[])),
        Err(ManagerError::NothingToApprove(_))
    ));
}

#[test]
fn a_plugin_runs_with_exactly_what_was_granted() {
    let env = env();
    let src = source(&env, "alpha", "0.1.0", &["a.example", "b.example"], '1');
    env.manager.install(&src).expect("install");
    let state = env
        .manager
        .approve("alpha", &perms(&["a.example"]))
        .expect("subset");
    assert_eq!(state.granted, perms(&["a.example"]));
    let manifest = env.kernel.manifest("alpha").expect("registered");
    assert_eq!(manifest.permissions, perms(&["a.example"]));
}

#[test]
fn install_from_archive_with_root_files_or_one_folder() {
    let env = env();
    let wasm = component('3', 1);
    let manifest = manifest_with("beta", "0.1.0", &[]);
    let flat = archive(
        &env.src().join("beta.manangplugin"),
        &[
            ("manifest.toml", manifest.as_bytes()),
            ("plugin.wasm", &wasm),
            ("README.md", b"# Beta"),
            ("LICENSE", b"MIT"),
            ("assets/icon.svg", b"<svg/>"),
        ],
    );
    let state = env.manager.install(&flat).expect("flat archive");
    assert_eq!(state.name, "beta");
    assert_eq!(state.status, PluginStatus::NeedsApproval);
    assert!(env.plugins().join("beta/assets/icon.svg").is_file());
    assert!(env.plugins().join("beta/README.md").is_file());
    env.manager.approve("beta", &perms(&[])).expect("approve");
    assert_eq!(env.answer("beta"), Some(Payload::Json(json!(3))));

    let wrapped = archive(
        &env.src().join("gamma.manangplugin"),
        &[
            (
                "gamma/manifest.toml",
                manifest_with("gamma", "0.1.0", &[]).as_bytes(),
            ),
            ("gamma/plugin.wasm", &wasm),
        ],
    );
    let state = env.manager.install(&wrapped).expect("wrapped archive");
    assert_eq!(state.name, "gamma");
}

#[test]
fn hostile_or_broken_archives_are_refused_and_leave_nothing_behind() {
    let env = env();
    let wasm = component('1', 1);
    let good = manifest_with("evil", "0.1.0", &[]);
    let src = env.src();
    let big = vec![0u8; 9 * 1024 * 1024];
    let many: Vec<(String, Vec<u8>)> = (0..300)
        .map(|i| (format!("assets/f{i}.txt"), b"x".to_vec()))
        .collect();
    let mut many_entries: Vec<(&str, &[u8])> =
        vec![("manifest.toml", good.as_bytes()), ("plugin.wasm", &wasm)];
    many_entries.extend(many.iter().map(|(n, b)| (n.as_str(), b.as_slice())));

    let cases: Vec<(&str, PathBuf, &str)> = vec![
        (
            "traversal",
            archive(
                &src.join("traversal.manangplugin"),
                &[
                    ("manifest.toml", good.as_bytes()),
                    ("plugin.wasm", &wasm),
                    ("../../escape.txt", b"x"),
                ],
            ),
            "not a safe relative path",
        ),
        (
            "absolute",
            archive(
                &src.join("absolute.manangplugin"),
                &[("manifest.toml", good.as_bytes()), ("/etc/passwd", b"x")],
            ),
            "not a safe relative path",
        ),
        (
            "backslash",
            archive(
                &src.join("backslash.manangplugin"),
                &[("manifest.toml", good.as_bytes()), ("assets\\..\\x", b"x")],
            ),
            "not a safe relative path",
        ),
        (
            "duplicate",
            archive(
                &src.join("duplicate.manangplugin"),
                &[
                    ("manifest.toml", good.as_bytes()),
                    ("plugin.wasm", &wasm),
                    ("README.md", b"a"),
                    ("readme.md", b"b"),
                ],
            ),
            "more than once",
        ),
        (
            "oversize",
            archive(
                &src.join("oversize.manangplugin"),
                &[
                    ("manifest.toml", good.as_bytes()),
                    ("plugin.wasm", &wasm),
                    ("assets/big.bin", &big),
                ],
            ),
            "larger than the limit",
        ),
        (
            "many",
            archive(&src.join("many.manangplugin"), &many_entries),
            "more than 256 files",
        ),
        (
            "stray file",
            archive(
                &src.join("stray.manangplugin"),
                &[
                    ("manifest.toml", good.as_bytes()),
                    ("plugin.wasm", &wasm),
                    ("install.sh", b"rm -rf"),
                ],
            ),
            "not allowed",
        ),
        (
            "no manifest",
            archive(
                &src.join("nomanifest.manangplugin"),
                &[("plugin.wasm", &wasm)],
            ),
            "no `manifest.toml`",
        ),
        (
            "bad manifest",
            archive(
                &src.join("badmanifest.manangplugin"),
                &[("manifest.toml", b"[plugin\nname"), ("plugin.wasm", &wasm)],
            ),
            "not valid",
        ),
        (
            "not wasm",
            archive(
                &src.join("notwasm.manangplugin"),
                &[("manifest.toml", good.as_bytes()), ("plugin.wasm", b"nope")],
            ),
            "not valid",
        ),
        (
            "name mismatch with folder",
            archive(
                &src.join("mismatch.manangplugin"),
                &[
                    ("other/manifest.toml", good.as_bytes()),
                    ("other/plugin.wasm", &wasm),
                ],
            ),
            "must be named after the plugin",
        ),
        (
            "not a zip",
            {
                let p = src.join("garbage.manangplugin");
                std::fs::write(&p, b"this is not a zip").expect("write");
                p
            },
            "not a valid plugin archive",
        ),
    ];
    for (label, path, expect) in cases {
        let err = env
            .manager
            .install(&path)
            .expect_err(&format!("{label} must be refused"));
        assert!(
            err.to_string().contains(expect),
            "{label}: `{err}` should mention `{expect}`"
        );
    }
    assert!(
        leftovers(&env).is_empty(),
        "left behind: {:?}",
        leftovers(&env)
    );
    assert!(!env.root().join("escape.txt").exists());
    assert!(env.manager.list().is_empty());
}

#[cfg(unix)]
#[test]
fn symlinks_are_refused_in_archives_and_folders() {
    let env = env();
    let wasm = component('1', 1);
    let manifest = manifest_with("linky", "0.1.0", &[]);

    // An archive holding a symlink entry.
    let path = env.src().join("linky.manangplugin");
    {
        use std::io::Write;
        let file = std::fs::File::create(&path).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("manifest.toml", options).expect("start");
        zip.write_all(manifest.as_bytes()).expect("write");
        zip.start_file("plugin.wasm", options).expect("start");
        zip.write_all(&wasm).expect("write");
        zip.add_symlink("README.md", "/etc/passwd", options)
            .expect("symlink");
        zip.finish().expect("finish");
    }
    let err = env.manager.install(&path).expect_err("symlink entry");
    assert!(err.to_string().contains("symbolic link"), "{err}");

    // A folder whose wasm is a link out.
    let dir = env.src().join("linky");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("manifest.toml"), &manifest).expect("manifest");
    let outside = env.src().join("outside.wasm");
    std::fs::write(&outside, &wasm).expect("outside");
    std::os::unix::fs::symlink(&outside, dir.join("plugin.wasm")).expect("link");
    let err = env.manager.install(&dir).expect_err("symlinked wasm");
    assert!(err.to_string().contains("symbolic link"), "{err}");
    assert!(leftovers(&env).is_empty());
}

#[test]
fn a_folder_must_be_named_after_its_plugin() {
    let env = env();
    let dir = folder(
        &env.src(),
        "wrong-name",
        &manifest_with("right-name", "0.1.0", &[]),
        &component('1', 1),
    );
    let err = env.manager.install(&dir).expect_err("mismatch");
    assert!(
        err.to_string().contains("must be named after the plugin"),
        "{err}"
    );
    assert!(leftovers(&env).is_empty());
}

#[test]
fn inspect_reports_without_installing() {
    let env = env();
    let src = source(&env, "alpha", "1.2.0", &["example.org"], '1');
    let report = env.manager.inspect(&src).expect("inspect");
    assert_eq!(report.name, "alpha");
    assert_eq!(report.version, "1.2.0");
    assert_eq!(report.kind, "folder");
    assert_eq!(report.permissions, perms(&["example.org"]));
    assert_eq!(report.new_permissions, perms(&["example.org"]));
    assert_eq!(report.commands[0].id, "alpha.run");
    assert!(report.installed_version.is_none());
    assert!(leftovers(&env).is_empty(), "inspect leaves no trace");
    assert!(env.manager.list().is_empty());
}

#[test]
fn an_update_asking_for_more_is_held_and_the_old_version_keeps_running() {
    let env = env();
    install_and_approve(&env, "alpha", &["a.example"]);
    assert_eq!(env.answer("alpha"), Some(Payload::Json(json!(1))));

    // Same version or older: refused.
    let same = source(&env, "alpha", "0.1.0", &["a.example"], '2');
    assert!(matches!(
        env.manager.install(&same),
        Err(ManagerError::AlreadyInstalled { .. })
    ));

    // A newer version asking for a new host waits.
    std::fs::remove_dir_all(&same).expect("rm src");
    let v2 = source(&env, "alpha", "0.2.0", &["a.example", "b.example"], '2');
    let report = env.manager.inspect(&v2).expect("inspect update");
    assert_eq!(report.installed_version.as_deref(), Some("0.1.0"));
    assert_eq!(report.new_permissions, perms(&["b.example"]));
    let state = env.manager.install(&v2).expect("install update");
    assert_eq!(state.status, PluginStatus::NeedsApproval);
    assert!(state.update_pending);
    assert_eq!(state.version, "0.2.0");
    assert_eq!(state.running_version.as_deref(), Some("0.1.0"));
    assert_eq!(state.new_permissions, perms(&["b.example"]));
    assert_eq!(
        env.answer("alpha"),
        Some(Payload::Json(json!(1))),
        "old keeps running"
    );

    // The hold survives a restart; the old version is what loads.
    let env = restart(env);
    env.manager.scan().expect("scan");
    assert_eq!(env.answer("alpha"), Some(Payload::Json(json!(1))));
    let state = env.manager.state_of("alpha").expect("state");
    assert_eq!(state.status, PluginStatus::NeedsApproval);
    assert!(state.update_pending);

    let state = env
        .manager
        .approve("alpha", &perms(&["a.example", "b.example"]))
        .expect("approve update");
    assert_eq!(state.status, PluginStatus::Loaded);
    assert_eq!(state.running_version.as_deref(), Some("0.2.0"));
    assert!(!state.update_pending);
    assert_eq!(env.answer("alpha"), Some(Payload::Json(json!(2))));
    assert!(
        std::fs::read_dir(env.trash())
            .expect("the old version is kept in the trash")
            .count()
            > 0
    );
}

#[test]
fn an_update_within_the_approved_permissions_applies_at_once() {
    let env = env();
    install_and_approve(&env, "alpha", &["a.example"]);
    let v2 = source(&env, "alpha", "0.2.0", &["a.example"], '2');
    let state = env.manager.install(&v2).expect("install update");
    assert_eq!(state.status, PluginStatus::Loaded);
    assert_eq!(state.running_version.as_deref(), Some("0.2.0"));
    assert_eq!(env.answer("alpha"), Some(Payload::Json(json!(2))));
}

#[test]
fn disabling_persists_across_restarts_and_enabling_loads_again() {
    let env = env();
    install_and_approve(&env, "alpha", &["example.org"]);
    let state = env.manager.disable("alpha").expect("disable");
    assert_eq!(state.status, PluginStatus::Disabled);
    assert!(!state.enabled);
    assert_eq!(state.disabled_by, Some("user"));
    assert!(env.answer("alpha").is_none(), "unloaded");
    assert!(
        env.plugins().join("alpha/plugin.wasm").is_file(),
        "files stay"
    );

    let env = restart(env);
    let report = env.manager.scan().expect("scan");
    assert!(report.loaded.is_empty());
    assert_eq!(env.status("alpha"), PluginStatus::Disabled);
    assert!(env.answer("alpha").is_none());

    let state = env.manager.enable("alpha").expect("enable");
    assert_eq!(state.status, PluginStatus::Loaded);
    assert_eq!(env.answer("alpha"), Some(Payload::Json(json!(1))));

    // An approved plugin loads on the next start without asking again.
    let env = restart(env);
    let report = env.manager.scan().expect("scan");
    assert_eq!(report.loaded.len(), 1);
    assert_eq!(env.status("alpha"), PluginStatus::Loaded);
    let text = std::fs::read_to_string(env.root().join("config/plugins.toml")).expect("registry");
    assert!(
        text.contains("[plugins.alpha]") && text.contains("example.org"),
        "{text}"
    );
}

#[test]
fn uninstall_unloads_and_moves_the_files_to_the_trash() {
    let env = env();
    install_and_approve(&env, "alpha", &[]);
    assert!(env.answer("alpha").is_some());
    let report = env.manager.uninstall("alpha").expect("uninstall");
    assert!(env.answer("alpha").is_none());
    assert!(!env.plugins().join("alpha").exists());
    let trashed = PathBuf::from(report.trashed_to.expect("trashed"));
    assert!(trashed.join("manifest.toml").is_file(), "kept, not deleted");
    assert!(trashed.starts_with(env.trash()));
    assert!(matches!(
        env.manager.state_of("alpha"),
        Err(ManagerError::NotInstalled(_))
    ));
    assert!(matches!(
        env.manager.uninstall("alpha"),
        Err(ManagerError::NotInstalled(_))
    ));
    // And it is forgotten: installing it again starts from scratch.
    let again = source(&env, "alpha", "0.1.0", &[], '1');
    assert_eq!(
        env.manager.install(&again).expect("reinstall").status,
        PluginStatus::NeedsApproval
    );
}

struct Stub;

impl Plugin for Stub {
    fn invoke(&self, _: &Context<'_>, _: &str, _: Payload) -> Result<Payload, PluginError> {
        Ok(Payload::Json(json!("stub")))
    }
}

fn register_native(kernel: &Kernel, name: &str) {
    let manifest = Manifest::from_toml(&format!(
        "[plugin]\nname = \"{name}\"\nversion = \"0.1.0\"\ntier = \"native\"\n\
         description = \"Built-in {name}\"\n\
         [[contributes.commands]]\nid = \"{name}.run\"\ntitle = \"Run\"\n"
    ))
    .expect("manifest");
    kernel.register(manifest, Arc::new(Stub)).expect("register");
}

#[test]
fn core_plugins_cannot_be_disabled_or_removed_but_others_can_be_switched_off() {
    let env = env();
    register_native(&env.kernel, "editor");
    register_native(&env.kernel, "git");

    let list = env.manager.list();
    let editor = list.iter().find(|s| s.name == "editor").expect("editor");
    let git = list.iter().find(|s| s.name == "git").expect("git");
    assert!(editor.core && !git.core);
    assert_eq!(editor.status, PluginStatus::Loaded);

    assert!(matches!(
        env.manager.disable("editor"),
        Err(ManagerError::Core(_))
    ));
    assert!(matches!(
        env.manager.uninstall("editor"),
        Err(ManagerError::Core(_))
    ));
    assert!(matches!(
        env.manager.uninstall("git"),
        Err(ManagerError::BuiltIn(_))
    ));
    assert!(env.kernel.has_command("editor.run"));

    let state = env.manager.disable("git").expect("git may be disabled");
    assert_eq!(state.status, PluginStatus::Disabled);
    assert!(!env.kernel.has_command("git.run"));
    // It is still listed, so it can be switched back on.
    assert_eq!(env.status("git"), PluginStatus::Disabled);
    let state = env.manager.enable("git").expect("enable");
    assert_eq!(state.status, PluginStatus::Loaded);
    assert!(env.kernel.has_command("git.run"));

    // A persisted choice is applied to the built-ins at startup.
    env.manager.disable("git").expect("disable");
    let env = restart(env);
    register_native(&env.kernel, "git");
    let changed = env.manager.reconcile_builtins();
    assert_eq!(changed, ["git"]);
    assert!(!env.kernel.has_command("git.run"));
}

fn project_with(env: &Env, disabled: &str) -> PathBuf {
    let project = env.root().join("project");
    std::fs::create_dir_all(project.join(".manang")).expect("mkdir");
    std::fs::write(project.join(".manang/plugins.toml"), disabled).expect("write");
    project
}

#[test]
fn a_project_can_only_switch_plugins_off() {
    let env = env();
    install_and_approve(&env, "alpha", &[]);
    install_and_approve(&env, "beta", &[]);

    let project = project_with(&env, "disabled = [\"alpha\"]\n");
    let changed = env.manager.set_project(Some(&project)).expect("project");
    assert_eq!(changed, ["alpha"]);
    let alpha = env.manager.state_of("alpha").expect("alpha");
    assert_eq!(alpha.status, PluginStatus::Disabled);
    assert_eq!(alpha.disabled_by, Some("project"));
    assert!(env.answer("alpha").is_none());
    assert!(env.answer("beta").is_some());
    // The user's own record is untouched.
    let text = std::fs::read_to_string(env.root().join("config/plugins.toml")).expect("registry");
    assert!(!text.contains("enabled = false"), "{text}");

    // Closing the project brings it back.
    env.manager.set_project(None).expect("clear");
    assert!(env.answer("alpha").is_some());

    // A project cannot enable anything: the key does not exist.
    let bad = project_with(&env, "enabled = [\"alpha\"]\n");
    assert!(matches!(
        env.manager.set_project(Some(&bad)),
        Err(ManagerError::Registry(_))
    ));

    // And it cannot undo the user's own choice.
    env.manager.disable("beta").expect("user disables");
    let empty = project_with(&env, "disabled = []\n");
    env.manager.set_project(Some(&empty)).expect("project");
    assert_eq!(env.status("beta"), PluginStatus::Disabled);
    assert_eq!(
        env.manager.state_of("beta").expect("beta").disabled_by,
        Some("user")
    );
}

#[test]
fn failures_keep_their_text_and_clear_when_fixed() {
    let env = env();
    // Dropped in by hand, like a plugin under development.
    let dir = env.plugins().join("broken");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("manifest.toml"),
        manifest_with("broken", "0.1.0", &[]),
    )
    .expect("m");
    std::fs::write(dir.join("plugin.wasm"), b"definitely not wasm").expect("w");
    // And one with no manifest at all.
    std::fs::create_dir_all(env.plugins().join("empty")).expect("mkdir");

    let report = env.manager.scan().expect("scan");
    assert_eq!(report.failed.len(), 2);
    let broken = env.manager.state_of("broken").expect("broken");
    assert_eq!(broken.status, PluginStatus::Failed);
    let error = broken.error.expect("error text");
    assert!(!error.is_empty(), "the reason is kept");
    let empty = env.manager.state_of("empty").expect("empty");
    assert_eq!(empty.status, PluginStatus::Failed);
    assert!(empty.error.expect("error").contains("manifest.toml"));

    let failed: Vec<_> = env
        .events
        .lock()
        .iter()
        .filter_map(|e| match e {
            ManagerEvent::Failed { name, error } => Some((name.clone(), error.clone())),
            _ => None,
        })
        .collect();
    assert!(
        failed.iter().any(|(n, e)| n == "broken" && !e.is_empty()),
        "{failed:?}"
    );
    assert!(failed.iter().any(|(n, _)| n == "empty"));

    // Fix it: the failure goes away.
    std::fs::write(dir.join("plugin.wasm"), component('1', 1)).expect("fix");
    let state = env.manager.reload_plugin("broken").expect("retry");
    assert_eq!(state.status, PluginStatus::Loaded);
    assert!(state.error.is_none());
}

#[test]
fn events_carry_names_and_errors() {
    let env = env();
    let src = source(&env, "alpha", "0.1.0", &["example.org"], '1');
    env.manager.install(&src).expect("install");
    env.manager
        .approve("alpha", &perms(&["example.org"]))
        .expect("approve");
    env.manager.disable("alpha").expect("disable");
    env.manager.scan().expect("scan");

    let events = env.events.lock().clone();
    let topics: Vec<(&str, Option<&str>)> =
        events.iter().map(|e| (e.topic(), e.plugin())).collect();
    assert!(
        topics.contains(&("plugins.needs-approval", Some("alpha"))),
        "{topics:?}"
    );
    assert!(topics.contains(&("plugins.loaded", Some("alpha"))));
    assert!(topics.contains(&("plugins.unloaded", Some("alpha"))));
    assert!(topics.contains(&("plugins.state-changed", Some("alpha"))));
    assert!(topics.contains(&("plugins.scanned", None)));
    let loaded = events
        .iter()
        .find(|e| e.topic() == "plugins.loaded")
        .expect("loaded");
    assert_eq!(loaded.payload(), json!({ "name": "alpha" }));
    let scanned = events
        .iter()
        .rfind(|e| e.topic() == "plugins.scanned")
        .expect("scanned");
    assert_eq!(scanned.payload(), json!({ "loaded": 0, "failed": 0 }));
    let failed = ManagerEvent::Failed {
        name: "x".into(),
        error: "boom".into(),
    };
    assert_eq!(failed.payload(), json!({ "name": "x", "error": "boom" }));
}

struct Blocklist(&'static str);

impl PolicyGate for Blocklist {
    fn check(&self, request: &PolicyRequest<'_>) -> Result<(), PolicyDenied> {
        if request.name == self.0 {
            Err(PolicyDenied {
                reason: format!("{} is on the organisation's blocklist", request.name),
            })
        } else {
            Ok(())
        }
    }
}

#[test]
fn the_policy_gate_can_veto_install_and_enable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let events: Events = Arc::default();
    let kernel = Arc::new(Kernel::new());
    let mut cfg = config(tmp.path(), &events);
    cfg.policy = Arc::new(Blocklist("banned"));
    let manager = PluginManager::new(Arc::clone(&kernel), cfg).expect("manager");
    let src = tmp.path().join("src");
    let banned = folder(
        &src,
        "banned",
        &manifest_with("banned", "0.1.0", &[]),
        &component('1', 1),
    );
    let err = manager.install(&banned).expect_err("blocked");
    assert!(matches!(err, ManagerError::Denied { .. }), "{err}");
    assert!(err.to_string().contains("blocklist"));
    assert!(!tmp.path().join("plugins/banned").exists());

    // A plugin dropped in by hand is refused at load, with the reason shown.
    let dropped = tmp.path().join("plugins/banned");
    std::fs::create_dir_all(&dropped).expect("mkdir");
    std::fs::write(
        dropped.join("manifest.toml"),
        manifest_with("banned", "0.1.0", &[]),
    )
    .expect("m");
    std::fs::write(dropped.join("plugin.wasm"), component('1', 1)).expect("w");
    let report = manager.scan().expect("scan");
    assert_eq!(report.failed.len(), 1);
    let state = manager.state_of("banned").expect("state");
    assert_eq!(state.status, PluginStatus::Failed);
    assert!(state.error.expect("error").contains("blocklist"));
}

#[test]
fn names_and_paths_are_validated() {
    let env = env();
    for bad in ["../etc", "a/b", "A", "", "-x", "x y", ".hidden"] {
        assert!(
            matches!(env.manager.enable(bad), Err(ManagerError::InvalidName(_))),
            "{bad:?}"
        );
        assert!(matches!(
            env.manager.uninstall(bad),
            Err(ManagerError::InvalidName(_))
        ));
        assert!(matches!(
            env.manager.approve(bad, &perms(&[])),
            Err(ManagerError::InvalidName(_))
        ));
    }
    assert!(matches!(
        env.manager.install(Path::new("/definitely/not/here")),
        Err(ManagerError::InvalidPath(_))
    ));
    let file = env.src().join("plugin.zip");
    std::fs::write(&file, b"x").expect("write");
    assert!(matches!(
        env.manager.install(&file),
        Err(ManagerError::InvalidPath(_))
    ));
}

#[test]
fn a_damaged_registry_is_set_aside_not_fatal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join("config")).expect("mkdir");
    std::fs::write(tmp.path().join("config/plugins.toml"), "[plugins.a\nbroken").expect("write");
    let env = start_in(tmp);
    assert_eq!(env.manager.warnings().len(), 1);
    assert!(env.root().join("config/plugins.toml.corrupt").is_file());
    assert!(env.manager.list().is_empty());
}

#[test]
fn the_list_shape_is_stable() {
    let env = env();
    register_native(&env.kernel, "editor");
    install_and_approve(&env, "alpha", &["example.org"]);
    let waiting = source(&env, "waiting", "0.3.0", &["api.example"], '1');
    env.manager.install(&waiting).expect("install");
    let dir = env.plugins().join("broken");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("manifest.toml"),
        manifest_with("broken", "0.1.0", &[]),
    )
    .expect("m");
    std::fs::write(dir.join("plugin.wasm"), b"nope").expect("w");
    env.manager.sync("broken");

    let mut value = serde_json::to_value(env.manager.list_with_warnings()).expect("json");
    // Redact what differs between runs.
    if let Some(plugins) = value["plugins"].as_array_mut() {
        for p in plugins {
            if p.get("dir").is_some() {
                p["dir"] = json!("<dir>");
            }
            if p.get("installed_at").is_some() {
                p["installed_at"] = json!(0);
            }
            if let Some(e) = p.get("error").and_then(|e| e.as_str()) {
                let trimmed = e.split(" (").next().unwrap_or(e).to_owned();
                p["error"] = json!(if trimmed.len() > 40 {
                    "<error>".to_owned()
                } else {
                    trimmed
                });
            }
        }
    }
    insta::assert_snapshot!(serde_json::to_string_pretty(&value).expect("pretty"));
}
