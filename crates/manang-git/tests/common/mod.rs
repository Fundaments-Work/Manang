//! Shared helpers: isolated environment, temp repos, a kernel harness.
#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Once};

use manang_kernel::{EventBus, Kernel, Payload};
use serde_json::{Value, json};

static ISOLATE: Once = Once::new();

/// Point git at an empty home so the developer's own config never leaks in.
/// Runs once per test process, before any other thread reads the environment.
pub fn isolate() {
    ISOLATE.call_once(|| {
        let home = tempfile::tempdir().expect("home").keep();
        // SAFETY: called from the first line of every test before any
        // subprocess or gix call; test processes set these once and never
        // change them again.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("XDG_CONFIG_HOME", home.join("xdg"));
            std::env::set_var("GIT_CONFIG_GLOBAL", home.join("gitconfig"));
            std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        }
    });
}

/// Run plain `git` (test setup only) and return trimmed stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

pub struct Repo {
    pub tmp: tempfile::TempDir,
    pub dir: PathBuf,
}

impl Repo {
    /// A directory that is not a repository.
    pub fn plain() -> Self {
        isolate();
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("proj").canonicalize_or_create();
        Self { tmp, dir }
    }

    /// An initialised repository on `main` with a local identity, no commits.
    pub fn unborn() -> Self {
        let r = Self::plain();
        git(&r.dir, &["init", "-q", "-b", "main"]);
        r.identity();
        r
    }

    /// `unborn()` plus one commit adding `a.txt` ("one\n").
    pub fn with_commit() -> Self {
        let r = Self::unborn();
        r.write("a.txt", "one\n");
        git(&r.dir, &["add", "a.txt"]);
        git(&r.dir, &["commit", "-q", "-m", "first"]);
        r
    }

    pub fn identity(&self) {
        git(&self.dir, &["config", "user.name", "Test User"]);
        git(&self.dir, &["config", "user.email", "test@example.com"]);
        git(&self.dir, &["config", "commit.gpgsign", "false"]);
    }

    pub fn write(&self, rel: &str, content: &str) {
        self.write_bytes(rel, content.as_bytes());
    }

    pub fn write_bytes(&self, rel: &str, content: &[u8]) {
        let p = self.dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(p, content).expect("write");
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.dir.join(rel)).expect("read")
    }

    pub fn commit_all(&self, msg: &str) {
        git(&self.dir, &["add", "-A"]);
        git(&self.dir, &["commit", "-q", "-m", msg]);
    }
}

trait CanonOrCreate {
    fn canonicalize_or_create(self) -> PathBuf;
}

impl CanonOrCreate for PathBuf {
    fn canonicalize_or_create(self) -> PathBuf {
        fs::create_dir_all(&self).expect("mkdir");
        self.canonicalize().expect("canonicalize")
    }
}

pub struct Harness {
    pub kernel: Kernel,
    pub bus: EventBus,
}

pub fn harness() -> Harness {
    isolate();
    let bus = EventBus::new();
    let kernel = Kernel::new();
    kernel
        .register(
            manang_git::manifest().expect("manifest"),
            Arc::new(manang_git::plugin(Some(bus.clone()))),
        )
        .expect("register");
    Harness { kernel, bus }
}

impl Harness {
    /// Call a command with `root` filled in. `Err` carries the parsed
    /// `{code, message}` error object.
    pub fn call(&self, repo: &Repo, command: &str, mut args: Value) -> Result<Value, Value> {
        if let Some(obj) = args.as_object_mut() {
            obj.entry("root")
                .or_insert(json!(repo.dir.to_string_lossy()));
        }
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(Payload::Empty) => Ok(Value::Null),
            Ok(Payload::Binary(_)) => Ok(json!({ "binary": true })),
            Err(e) => {
                let text = e.to_string();
                Err(serde_json::from_str(&text).unwrap_or_else(|_| json!({ "message": text })))
            }
        }
    }

    pub fn ok(&self, repo: &Repo, command: &str, args: Value) -> Value {
        self.call(repo, command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    pub fn code(&self, repo: &Repo, command: &str, args: Value) -> String {
        match self.call(repo, command, args) {
            Ok(v) => panic!("{command} unexpectedly succeeded: {v}"),
            Err(e) => e["code"].as_str().unwrap_or("?").to_owned(),
        }
    }
}

/// One repository shared by property-test cases (creating one per case would
/// dominate the run time).
pub fn shared_repo() -> &'static Repo {
    static SHARED: std::sync::OnceLock<Repo> = std::sync::OnceLock::new();
    SHARED.get_or_init(Repo::with_commit)
}
