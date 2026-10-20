//! Project sessions: two (or three) whole plugin instances through a real
//! relay, no UI.
//!
//! Each `Peer` is a kernel with the editor and live plugins, a project folder
//! on disk and (where a test wants real file events) the project watcher
//! publishing `project.changed` on the plugin's event bus. A peer also plays
//! the web view's part of the edit protocol for the files it attaches: it
//! keeps a model of the view's text per file, pushes with the version it has
//! seen, retries when told it is stale and applies `live.remote` events in
//! order. The relay runs in-process on a random port and records nothing but
//! forwards everything.

use std::collections::BTreeMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_editor::{Editor, EditorPlugin};
use manang_kernel::{EventBus, Kernel, Payload, Subscription};
use manang_live::LoroEngine;
use manang_live::blob::{BlobHash, blob_hash};
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{Invite, OsEntropy, RoomSecret, seed_fingerprint};
use manang_live::project::{FileId, LoroTreeEngine, NodeKind, NodeMeta, ProjectDoc, TreeCrdt};
use manang_live::session::{Input, Output, Session, SessionConfig};
use manang_live::transport::{SyncTransport, TransportEvent};
use manang_live_plugin::{InviteLink, InviteScope, LivePlugin};
use manang_live_testkit::Handle;
use manang_project::watch::ProjectWatcher;
use manang_relay_client::RelayClient;
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::{Value, json};
use tokio::runtime::Runtime;

struct Relay {
    rt: Runtime,
    running: Option<Handle>,
    addr: SocketAddr,
}

impl Relay {
    fn start() -> Self {
        let rt = Runtime::new().expect("runtime");
        let running = rt
            .block_on(manang_live_testkit::spawn(
                "127.0.0.1:0".parse().expect("addr"),
            ))
            .expect("fixture binds");
        let addr = running.local_addr();
        Self {
            rt,
            running: Some(running),
            addr,
        }
    }

    fn url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    /// Take the relay down: every connection drops.
    fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            self.rt.block_on(running.stop());
        }
    }

    /// Bring it back on the same port.
    fn restart(&mut self) {
        let running = self
            .rt
            .block_on(manang_live_testkit::spawn(self.addr))
            .expect("relay binds again");
        self.running = Some(running);
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            self.rt.block_on(running.stop());
        }
    }
}

/// What the web view shows of one attached file.
#[derive(Default)]
struct View {
    version: u64,
    text: Vec<u16>,
    held: BTreeMap<u64, Value>,
}

impl View {
    fn apply(&mut self, change: &Value) {
        let start = change["start"].as_u64().expect("start") as usize;
        let delete = change["delete"].as_u64().expect("delete") as usize;
        let insert: Vec<u16> = change["insert"]
            .as_str()
            .expect("insert")
            .encode_utf16()
            .collect();
        self.text.splice(start..start + delete, insert);
    }

    fn string(&self) -> String {
        String::from_utf16(&self.text).expect("the view model stays valid UTF-16")
    }
}

struct Peer {
    kernel: Kernel,
    events: EventBus,
    sub: Subscription,
    /// The project folder (canonical).
    root: PathBuf,
    _dir: tempfile::TempDir,
    cfg: tempfile::TempDir,
    _watcher: Mutex<Option<ProjectWatcher>>,
    sid: Mutex<String>,
    log: Mutex<Vec<(String, Value)>>,
    views: Mutex<BTreeMap<String, View>>,
}

fn err_code(error: &str) -> String {
    serde_json::from_str::<Value>(error)
        .ok()
        .and_then(|v| v.get("code").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| error.to_owned())
}

fn wait_for(limit: Duration, what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// One node of a hand-made (possibly hostile) project.
struct RawEntry {
    parent: Option<usize>,
    name: &'static str,
    text: Option<&'static str>,
    blob: Option<Vec<u8>>,
}

fn raw_text(parent: Option<usize>, name: &'static str, text: &'static str) -> RawEntry {
    RawEntry {
        parent,
        name,
        text: Some(text),
        blob: None,
    }
}

fn raw_dir(name: &'static str) -> RawEntry {
    RawEntry {
        parent: None,
        name,
        text: None,
        blob: None,
    }
}

/// A host written by hand against the core crate: it shares whatever tree it
/// is given, with names a well-behaved peer can never create, and answers for
/// the files the guest asks for.
struct RawHost {
    session: Session<LoroEngine, OsEntropy>,
    transport: RelayClient,
    ropes: BTreeMap<FileId, Rope>,
    texts: BTreeMap<FileId, String>,
    blobs: BTreeMap<BlobHash, Vec<u8>>,
    started: Instant,
}

impl RawHost {
    fn new(relay: &Relay, entries: &[RawEntry]) -> (Self, String) {
        let peer = 4242;
        let mut engine = LoroTreeEngine::new(peer).expect("tree");
        let mut ids: Vec<FileId> = Vec::new();
        let mut texts = BTreeMap::new();
        let mut blobs = BTreeMap::new();
        for entry in entries {
            let parent = entry.parent.map(|i| ids[i]);
            let meta = match (entry.text, &entry.blob) {
                (Some(text), _) => NodeMeta::file(
                    entry.name,
                    NodeKind::Text,
                    text.len() as u64,
                    blob_hash(text.as_bytes()),
                    seed_fingerprint(text),
                ),
                (None, Some(bytes)) => NodeMeta::file(
                    entry.name,
                    NodeKind::Blob,
                    bytes.len() as u64,
                    blob_hash(bytes),
                    [0; 16],
                ),
                (None, None) => NodeMeta::dir(entry.name),
            };
            // The engine does not vet names: this is how a hostile peer
            // gets `..` or `.git` into the tree.
            let id = engine.create(parent, &meta).expect("create");
            if let Some(text) = entry.text {
                texts.insert(id, text.to_owned());
            }
            if let Some(bytes) = &entry.blob {
                blobs.insert(blob_hash(bytes), bytes.clone());
            }
            ids.push(id);
        }
        let snapshot = engine.export_snapshot().expect("snapshot");
        let project = ProjectDoc::from_snapshot(peer, &snapshot).expect("project");
        let secret = RoomSecret::generate(&mut OsEntropy).expect("secret");
        let keys = secret.derive_project().expect("keys");
        let url = format!("{}/room/{}", relay.url(), keys.room_id_hex());
        let fragment = Invite::for_project(secret, [0; 16]).to_fragment();
        let invite = InviteLink {
            scope: InviteScope::Project,
            relay: relay.url(),
            file: None,
            fragment,
        }
        .render()
        .expect("invite");
        let mut session = Session::new_project(
            project,
            keys,
            OsEntropy,
            SessionConfig {
                name: "Mallory".to_owned(),
                ..SessionConfig::default()
            },
        )
        .expect("session");
        let mut transport = RelayClient::new(url, relay.rt.handle().clone());
        let _ = session.flush_project();
        for output in session.start() {
            if matches!(output, Output::Connect) {
                transport.connect().expect("connect");
            }
        }
        (
            Self {
                session,
                transport,
                ropes: BTreeMap::new(),
                texts,
                blobs,
                started: Instant::now(),
            },
            invite,
        )
    }

    fn now(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).expect("ms")
    }

    fn perform(&mut self, outputs: Vec<Output>) {
        let mut queue: std::collections::VecDeque<Output> = outputs.into();
        while let Some(output) = queue.pop_front() {
            let now = self.now();
            match output {
                Output::Send(frame) => self.transport.send(frame).expect("send"),
                Output::Connect => self.transport.connect().expect("connect"),
                Output::DocWanted(id) => {
                    if let Some(text) = self.texts.get(&id)
                        && let Ok(doc) = LiveDoc::<LoroEngine>::seeded(4242, text)
                    {
                        queue.extend(self.session.provide_doc(now, id, doc, false));
                    }
                }
                Output::BlobRequested { hash, .. } => match self.blobs.get(&hash) {
                    Some(data) => queue.extend(self.session.offer_blob(now, hash, data.clone())),
                    None => self.session.decline_blob(hash),
                },
                _ => {}
            }
        }
    }

    /// Run the host for `ms`.
    fn run(&mut self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            while let Some(event) = self.transport.poll() {
                let now = self.now();
                let out = match &event {
                    TransportEvent::Up => self.session.handle_with(now, Input::Up, &mut self.ropes),
                    TransportEvent::Down => {
                        self.session.handle_with(now, Input::Down, &mut self.ropes)
                    }
                    TransportEvent::Frame(f) => {
                        self.session
                            .handle_with(now, Input::Frame(f), &mut self.ropes)
                    }
                };
                self.perform(out);
            }
            let now = self.now();
            let out = self.session.handle_with(now, Input::Tick, &mut self.ropes);
            self.perform(out);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

const LONG: Duration = Duration::from_secs(20);

impl Peer {
    /// A peer with an empty project folder. `loaded_docs` caps the documents
    /// kept in memory (the rest is parked in the sidecar).
    fn new(name: &str, relay: Option<&str>, loaded_docs: Option<usize>) -> Self {
        let events = EventBus::new();
        let editor = Arc::new(Editor::new());
        let cfg = tempfile::tempdir().expect("cfg");
        let dir = tempfile::tempdir().expect("project");
        let root = dir.path().canonicalize().expect("canonical root");
        let kernel = Kernel::new();
        kernel
            .register(
                manang_editor::manifest().expect("editor manifest"),
                Arc::new(EditorPlugin::new(Arc::clone(&editor))),
            )
            .expect("register editor");
        let mut plugin = LivePlugin::new(editor, events.clone(), cfg.path().to_path_buf());
        if let Some(n) = loaded_docs {
            plugin = plugin.with_loaded_docs(n);
        }
        kernel
            .register(
                manang_live_plugin::manifest().expect("live manifest"),
                Arc::new(plugin),
            )
            .expect("register live");
        let sub = events.subscribe_with_capacity("live.*", 100_000);
        let peer = Self {
            kernel,
            events,
            sub,
            root,
            _dir: dir,
            cfg,
            _watcher: Mutex::new(None),
            sid: Mutex::new(String::new()),
            log: Mutex::new(Vec::new()),
            views: Mutex::new(BTreeMap::new()),
        };
        let mut values = json!({ "live.display-name": name });
        if let Some(relay) = relay {
            values["live.relay-url"] = json!(relay);
        }
        peer.call("live.configure", json!({ "values": values }))
            .expect("configure");
        peer
    }

    /// Publish real `project.changed` events for this peer's folder.
    fn watch(&self) {
        let watcher =
            ProjectWatcher::start(&self.root, self.events.clone(), Duration::from_millis(80))
                .expect("watcher");
        *self._watcher.lock() = Some(watcher);
    }

    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(_) => Ok(Value::Null),
            Err(e) => Err(e.to_string()),
        }
    }

    fn write(&self, rel: &str, content: &str) {
        self.write_bytes(rel, content.as_bytes());
    }

    fn write_bytes(&self, rel: &str, content: &[u8]) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, content).expect("write");
    }

    fn read(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.root.join(rel)).ok()
    }

    fn read_bytes(&self, rel: &str) -> Option<Vec<u8>> {
        fs::read(self.root.join(rel)).ok()
    }

    fn start_project(&self) -> String {
        let out = self
            .call(
                "live.start",
                json!({ "scope": "project", "root": self.root }),
            )
            .expect("start");
        *self.sid.lock() = out["session"].as_str().expect("session").to_owned();
        assert_eq!(out["scope"], json!("project"));
        out["invite"].as_str().expect("invite").to_owned()
    }

    fn join_project(&self, invite: &str) -> Result<Value, String> {
        let out = self.call("live.join", json!({ "invite": invite, "root": self.root }))?;
        *self.sid.lock() = out["session"].as_str().expect("session").to_owned();
        Ok(out)
    }

    /// Move events from the bus into the log and the view models.
    fn drain(&self) {
        let sid = self.sid.lock().clone();
        for event in self.sub.drain() {
            let Payload::Json(body) = event.payload else {
                continue;
            };
            if body["session"] != json!(sid) {
                continue;
            }
            if event.topic == "live.remote" {
                let path = body["path"].as_str().unwrap_or("").to_owned();
                let mut views = self.views.lock();
                if let Some(view) = views.get_mut(&path) {
                    let version = body["version"].as_u64().expect("version");
                    view.held.insert(version, body.clone());
                    while let Some(next) = view.held.remove(&(view.version + 1)) {
                        for change in next["changes"].as_array().expect("changes") {
                            view.apply(change);
                        }
                        view.version += 1;
                    }
                }
            }
            self.log.lock().push((event.topic, body));
        }
    }

    fn events_of(&self, topic: &str) -> Vec<Value> {
        self.drain();
        self.log
            .lock()
            .iter()
            .filter(|(t, _)| t == topic)
            .map(|(_, v)| v.clone())
            .collect()
    }

    fn plan(&self) -> BTreeMap<String, String> {
        wait_for(LONG, "the join plan", || {
            !self.events_of("live.plan").is_empty()
        });
        let event = self.events_of("live.plan").pop().expect("plan");
        event["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().expect("path").to_owned(),
                    f["class"].as_str().expect("class").to_owned(),
                )
            })
            .collect()
    }

    /// Join with the choices and wait until every file is fetched.
    fn resolve(&self, choices: Value) -> Result<Value, String> {
        self.call("live.resolve", json!({ "choices": choices }))
    }

    fn status(&self) -> Value {
        self.call("live.status", json!({})).expect("status")
    }

    fn attach(&self, path: &str) -> String {
        let out = self
            .call("live.attach", json!({ "path": path }))
            .expect("attach");
        let text = out["text"].as_str().expect("text").to_owned();
        self.views.lock().insert(
            path.to_owned(),
            View {
                version: out["version"].as_u64().expect("version"),
                text: text.encode_utf16().collect(),
                held: BTreeMap::new(),
            },
        );
        text
    }

    fn view_text(&self, path: &str) -> String {
        self.drain();
        self.views
            .lock()
            .get(path)
            .map(View::string)
            .expect("attached view")
    }

    /// Type `text` at the end of an attached file; retries when stale.
    fn type_at_end(&self, path: &str, text: &str) {
        for _ in 0..200 {
            self.drain();
            let (version, at) = {
                let views = self.views.lock();
                let view = views.get(path).expect("attached view");
                (view.version, view.text.len())
            };
            let out = self
                .call(
                    "live.push",
                    json!({
                        "session": self.sid.lock().clone(),
                        "path": path,
                        "version": version,
                        "changes": [{ "start": at, "delete": 0, "insert": text }],
                    }),
                )
                .expect("push");
            let new_version = out["version"].as_u64().expect("version");
            if out["accepted"] == json!(true) {
                {
                    let mut views = self.views.lock();
                    let view = views.get_mut(path).expect("view");
                    view.apply(&json!({ "start": at, "delete": 0, "insert": text }));
                }
                // The accepted step is acknowledged by an ordered event.
                wait_for(LONG, "the push ack", || {
                    self.drain();
                    self.views.lock().get(path).expect("view").version >= new_version
                });
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("push never accepted");
    }

    fn leave(&self) {
        self.call("live.leave", json!({})).expect("leave");
    }

    /// Paths of the trash entries holding a file named like `name`.
    fn trash_has(&self, name: &str) -> Vec<String> {
        let mut found = Vec::new();
        let base = self.root.join(".manang/trash");
        fn walk(dir: &Path, name: &str, found: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, name, found);
                } else if path.file_name().is_some_and(|n| n == name) {
                    found.push(path.display().to_string());
                }
            }
        }
        walk(&base, name, &mut found);
        found
    }

    /// Parked document snapshots of the current rooms.
    fn sidecar_files(&self) -> usize {
        fn count(dir: &Path) -> usize {
            let Ok(entries) = fs::read_dir(dir) else {
                return 0;
            };
            entries
                .flatten()
                .map(|e| {
                    let path = e.path();
                    if path.is_dir() {
                        count(&path)
                    } else {
                        usize::from(path.extension().is_some_and(|x| x == "loro"))
                    }
                })
                .sum()
        }
        count(&self.cfg.path().join("manang-live"))
    }

    fn trash_text(&self, name: &str) -> Vec<String> {
        self.trash_has(name)
            .iter()
            .filter_map(|p| fs::read_to_string(p).ok())
            .collect()
    }
}

fn project_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("main.typ", "= Title\nhello é😀\n"),
        ("chapters/one.typ", "chapter one\n"),
        ("chapters/two.typ", "chapter two\n"),
        ("notes.md", "# notes\n"),
    ]
}

fn seed_host(host: &Peer) {
    for (path, text) in project_files() {
        host.write(path, text);
    }
    host.write_bytes("fig.png", &png_like(70_000));
}

fn png_like(len: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0, 0xff, 0xfe, 0];
    bytes.extend((0..len).map(|i| (i * 31 % 251) as u8));
    bytes
}

/// A host sharing the project folder and the relay it uses.
fn host_with_project() -> (Relay, Peer, String) {
    let relay = Relay::start();
    let host = Peer::new("Host", Some(&relay.url()), None);
    seed_host(&host);
    let invite = host.start_project();
    (relay, host, invite)
}

#[test]
fn a_guest_with_an_empty_folder_gets_every_file_and_nothing_is_written_before_resolve() {
    let (relay, host, invite) = host_with_project();
    let guest = Peer::new("Guest", Some(&relay.url()), None);
    guest.join_project(&invite).expect("join");
    let plan = guest.plan();
    assert_eq!(plan.len(), 5, "{plan:?}");
    assert!(plan.values().all(|c| c == "missing"), "{plan:?}");
    // Phase 1 changed nothing on disk.
    std::thread::sleep(Duration::from_millis(600));
    assert!(guest.read("main.typ").is_none());
    assert_eq!(guest.status()["phase"], json!("plan"));
    // Attaching before the plan is resolved is refused.
    assert!(
        guest
            .call("live.attach", json!({"path": "main.typ"}))
            .is_err()
    );

    let out = guest.resolve(json!([])).expect("resolve");
    assert_eq!(out["applied"], json!(5));
    assert_eq!(out["trashed"], json!([]));
    wait_for(LONG, "all files on the guest's disk", || {
        project_files()
            .iter()
            .all(|(p, t)| guest.read(p).as_deref() == Some(*t))
            && guest.read_bytes("fig.png") == host.read_bytes("fig.png")
    });
    assert_eq!(guest.read_bytes("fig.png").expect("fig").len(), 70_000 + 8);
    // Progress ran to the end and the file list is the host's.
    wait_for(LONG, "progress to finish", || {
        guest
            .events_of("live.progress")
            .last()
            .is_some_and(|p| p["done"] == p["total"])
    });
    let files = guest.call("live.files", json!({})).expect("files");
    let paths: Vec<&str> = files["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().expect("path"))
        .collect();
    assert_eq!(
        paths,
        vec![
            "chapters/one.typ",
            "chapters/two.typ",
            "fig.png",
            "main.typ",
            "notes.md"
        ]
    );
    assert_eq!(
        files["files"][2]["kind"],
        json!("binary"),
        "the image is a blob"
    );
    // Attached, the file hands the view its text.
    assert_eq!(guest.attach("main.typ"), "= Title\nhello é😀\n");
    // A second resolve has nothing to resolve.
    let again = guest.resolve(json!([])).expect_err("no plan");
    assert_eq!(err_code(&again), "no-plan");
    guest.leave();
    host.leave();
}

#[test]
fn the_plan_classifies_files_and_resolve_obeys_the_owner_decisions() {
    let (relay, host, invite) = host_with_project();
    let guest = Peer::new("Guest", Some(&relay.url()), None);
    // Identical, differing (two), missing (notes.md, fig.png) and guest-only.
    guest.write("chapters/one.typ", "chapter one\n");
    guest.write("main.typ", "my own title\n");
    guest.write("chapters/two.typ", "my chapter two\n");
    guest.write("private.txt", "mine only\n");
    guest.write("shared.txt", "mine, shared on purpose\n");
    guest.join_project(&invite).expect("join");
    let plan = guest.plan();
    let class = |p: &str| plan.get(p).map(String::as_str);
    assert_eq!(class("chapters/one.typ"), Some("identical"));
    assert_eq!(class("main.typ"), Some("differing"));
    assert_eq!(class("chapters/two.typ"), Some("differing"));
    assert_eq!(class("notes.md"), Some("missing"));
    assert_eq!(class("fig.png"), Some("missing"));
    assert_eq!(class("private.txt"), Some("guest-only"));
    assert_eq!(class("shared.txt"), Some("guest-only"));
    // Nothing on disk changed by planning.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(guest.read("main.typ").as_deref(), Some("my own title\n"));
    assert!(guest.read("notes.md").is_none());

    // A differing file with no answer is refused, and nothing happened.
    let refused = guest.resolve(json!([])).expect_err("unresolved");
    assert_eq!(err_code(&refused), "unresolved");
    // Two replacements without the all-files confirmation are refused too.
    let both = json!([
        { "path": "main.typ", "action": "take-host" },
        { "path": "chapters/two.typ", "action": "take-host" },
    ]);
    let refused = guest.resolve(both).expect_err("needs confirmation");
    assert_eq!(err_code(&refused), "needs-confirmation");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(guest.read("main.typ").as_deref(), Some("my own title\n"));
    assert_eq!(
        guest.read("chapters/two.typ").as_deref(),
        Some("my chapter two\n")
    );
    assert!(guest.read("notes.md").is_none(), "still nothing written");

    // The explicit all-files confirmation lets both be replaced.
    let out = guest
        .resolve(json!([
            { "path": "main.typ", "action": "take-host" },
            { "path": "chapters/two.typ", "action": "take-host" },
            { "path": "private.txt", "action": "keep-private" },
            { "path": "shared.txt", "action": "share" },
            { "path": "*", "action": "take-host" },
        ]))
        .expect("resolve");
    assert_eq!(out["trashed"], json!(["chapters/two.typ", "main.typ"]));
    // identical (1) + missing (2) + replaced (2) + shared (1)
    assert_eq!(out["applied"], json!(6));
    wait_for(LONG, "the host's versions on the guest's disk", || {
        guest.read("main.typ").as_deref() == Some("= Title\nhello é😀\n")
            && guest.read("chapters/two.typ").as_deref() == Some("chapter two\n")
            && guest.read("notes.md").as_deref() == Some("# notes\n")
            && guest.read_bytes("fig.png") == host.read_bytes("fig.png")
    });
    // The replaced copies were saved first.
    assert_eq!(guest.trash_text("main.typ"), vec!["my own title\n"]);
    assert_eq!(guest.trash_text("two.typ"), vec!["my chapter two\n"]);
    // The identical file was left alone; the private file stays private.
    assert_eq!(
        guest.read("chapters/one.typ").as_deref(),
        Some("chapter one\n")
    );
    assert_eq!(guest.read("private.txt").as_deref(), Some("mine only\n"));
    // The shared one reaches the host.
    wait_for(LONG, "the shared file on the host", || {
        host.read("shared.txt").as_deref() == Some("mine, shared on purpose\n")
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        host.read("private.txt").is_none(),
        "guest-only stays private"
    );
    // Editing the private file later does not quietly share it either.
    guest.watch();
    std::thread::sleep(Duration::from_millis(300));
    guest.write("private.txt", "still mine\n");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(host.read("private.txt").is_none());
    let files = guest.call("live.files", json!({})).expect("files");
    assert!(
        files["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|f| f["path"] != json!("private.txt"))
    );
    guest.leave();
    host.leave();
}

#[test]
fn a_single_explicit_replacement_needs_no_confirmation_and_keep_mine_stays_out() {
    let (relay, host, invite) = host_with_project();
    let guest = Peer::new("Guest", Some(&relay.url()), None);
    guest.write("main.typ", "my own title\n");
    guest.write("notes.md", "my notes\n");
    guest.join_project(&invite).expect("join");
    let plan = guest.plan();
    assert_eq!(plan["main.typ"], "differing");
    assert_eq!(plan["notes.md"], "differing");
    let out = guest
        .resolve(json!([
            { "path": "main.typ", "action": "take-host" },
            { "path": "notes.md", "action": "keep-mine" },
        ]))
        .expect("resolve");
    assert_eq!(out["trashed"], json!(["main.typ"]));
    wait_for(LONG, "the host's main.typ", || {
        guest.read("main.typ").as_deref() == Some("= Title\nhello é😀\n")
    });
    assert_eq!(guest.read("notes.md").as_deref(), Some("my notes\n"));
    // A kept file is out of the session: not listed, not attachable.
    let refused = guest
        .call("live.attach", json!({"path": "notes.md"}))
        .expect_err("excluded");
    assert_eq!(err_code(&refused), "excluded");
    // The host editing it later does not overwrite the guest's copy.
    host.attach("notes.md");
    host.type_at_end("notes.md", "host line\n");
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(guest.read("notes.md").as_deref(), Some("my notes\n"));
    guest.leave();
    host.leave();
}

/// A host and a joined guest with the whole project on both disks.
fn connected_pair() -> (Relay, Peer, Peer) {
    let (relay, host, invite) = host_with_project();
    let guest = Peer::new("Guest", Some(&relay.url()), None);
    guest.join_project(&invite).expect("join");
    guest.plan();
    guest.resolve(json!([])).expect("resolve");
    wait_for(LONG, "the guest to have every file", || {
        project_files()
            .iter()
            .all(|(p, t)| guest.read(p).as_deref() == Some(*t))
            && guest.read_bytes("fig.png") == host.read_bytes("fig.png")
    });
    (relay, host, guest)
}

fn settle(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

#[test]
fn edits_propagate_in_both_directions_for_open_and_closed_files() {
    let (_relay, host, guest) = connected_pair();
    host.watch();
    guest.watch();
    settle(400);

    // Both have main.typ open in a tab: the web view protocol, per file.
    assert_eq!(host.attach("main.typ"), "= Title\nhello é😀\n");
    assert_eq!(guest.attach("main.typ"), "= Title\nhello é😀\n");
    host.type_at_end("main.typ", "host line\n");
    wait_for(LONG, "the guest's view to follow the host", || {
        guest.view_text("main.typ") == "= Title\nhello é😀\nhost line\n"
    });
    guest.type_at_end("main.typ", "guest line é😀\n");
    let both = "= Title\nhello é😀\nhost line\nguest line é😀\n";
    wait_for(LONG, "the host's view to follow the guest", || {
        host.view_text("main.typ") == both && guest.view_text("main.typ") == both
    });
    // A file open in a tab is the web view's to write: the plugin leaves it.
    settle(1200);
    assert_eq!(
        guest.read("main.typ").as_deref(),
        Some("= Title\nhello é😀\n")
    );
    // Closing the tab hands the room's text to the disk.
    assert_eq!(
        guest
            .call("live.detach", json!({"path": "main.typ"}))
            .expect("detach"),
        json!({ "detached": true })
    );
    wait_for(LONG, "the closed tab to be written", || {
        guest.read("main.typ").as_deref() == Some(both)
    });

    // A file nobody has open on the host: the guest types in its tab and the
    // host's disk follows (debounced), without anyone attaching on the host.
    guest.attach("chapters/one.typ");
    guest.type_at_end("chapters/one.typ", "guest wrote this\n");
    wait_for(LONG, "the host's disk to follow", || {
        host.read("chapters/one.typ").as_deref() == Some("chapter one\nguest wrote this\n")
    });

    // An edit made on disk by an outside tool becomes a minimal text edit and
    // reaches the other disk; the echo does not bounce back.
    host.write("chapters/two.typ", "chapter two, edited on disk\n");
    wait_for(LONG, "the outside edit to reach the guest", || {
        guest.read("chapters/two.typ").as_deref() == Some("chapter two, edited on disk\n")
    });
    settle(1500);
    assert_eq!(
        host.read("chapters/two.typ").as_deref(),
        Some("chapter two, edited on disk\n")
    );
    assert_eq!(
        guest.read("chapters/two.typ").as_deref(),
        Some("chapter two, edited on disk\n")
    );
    guest.leave();
    host.leave();
}

#[test]
fn create_rename_and_delete_propagate_and_a_delete_is_recoverable() {
    let (_relay, host, guest) = connected_pair();
    host.watch();
    guest.watch();
    settle(400);

    // Create, in a new directory too.
    host.write("fresh.typ", "fresh\n");
    host.write("sub/deep.typ", "deep\n");
    wait_for(LONG, "the new files on the guest", || {
        guest.read("fresh.typ").as_deref() == Some("fresh\n")
            && guest.read("sub/deep.typ").as_deref() == Some("deep\n")
    });
    let tree = guest.events_of("live.tree");
    assert!(
        tree.iter()
            .any(|e| e["ops"].as_array().is_some_and(|ops| ops
                .iter()
                .any(|o| o["op"] == "create" && o["path"] == "fresh.typ"))),
        "{tree:?}"
    );

    // Rename.
    fs::rename(
        host.root.join("chapters/one.typ"),
        host.root.join("chapters/uno.typ"),
    )
    .expect("rename");
    wait_for(LONG, "the rename on the guest", || {
        guest.read("chapters/uno.typ").as_deref() == Some("chapter one\n")
            && guest.read("chapters/one.typ").is_none()
    });

    // Delete: the guest's copy goes to the trash, not away.
    fs::remove_file(host.root.join("notes.md")).expect("delete");
    wait_for(LONG, "the delete on the guest", || {
        guest.read("notes.md").is_none()
    });
    assert_eq!(guest.trash_text("notes.md"), vec!["# notes\n"]);
    let notices = guest.events_of("live.notice");
    assert!(
        notices
            .iter()
            .any(|n| n["kind"] == "deleted" && n["path"] == "notes.md"),
        "{notices:?}"
    );

    // Restore brings it back here and, as a new node, to the host.
    assert_eq!(
        guest
            .call("live.restore", json!({"path": "notes.md"}))
            .expect("restore"),
        json!({ "restored": true })
    );
    wait_for(LONG, "the restored file on both disks", || {
        guest.read("notes.md").as_deref() == Some("# notes\n")
            && host.read("notes.md").as_deref() == Some("# notes\n")
    });
    let again = guest
        .call("live.restore", json!({"path": "never-there.typ"}))
        .expect_err("not deleted");
    assert_eq!(err_code(&again), "not-deleted");
    guest.leave();
    host.leave();
}

#[test]
fn a_hostile_host_cannot_make_the_guest_write_outside_the_project_or_into_protected_places() {
    let relay = Relay::start();
    // Three top-level directories and the files a hostile tree puts in them.
    let entries = vec![
        raw_dir(".git"),                                    // 0
        raw_text(Some(0), "config", "[core] evil\n"),       // 1
        raw_dir(".manang"),                                 // 2
        raw_text(Some(2), "owned", "x\n"),                  // 3
        raw_text(None, "manang.toml", "[project]\nevil\n"), // 4
        raw_text(None, ".env", "TOKEN=1\n"),                // 5
        raw_dir("link"),                                    // 6 (the guest has a symlink here)
        raw_text(Some(6), "pwned.txt", "escaped\n"),        // 7
        raw_text(None, "evil.txt", "overwritten\n"),        // 8 (the guest has a symlink here)
        raw_text(None, "..", "traversal\n"),                // 9 names a parent directory
        raw_text(None, "a/b", "slash in a name\n"),         // 10
        raw_text(None, "c\\d", "backslash\n"),              // 11
        raw_dir("node_modules"),                            // 12
        raw_text(Some(12), "pkg.js", "x\n"),                // 13
        raw_text(None, "fine.txt", "an honest file\n"),     // 14
    ];
    let (mut raw, invite) = RawHost::new(&relay, &entries);
    let guest = Peer::new("Guest", Some(&relay.url()), None);
    // Somewhere outside the project, which the symlinks lead to.
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("secret.txt"), "do not touch\n").expect("secret");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), guest.root.join("link")).expect("dir link");
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            guest.root.join("evil.txt"),
        )
        .expect("file link");
    }
    guest.join_project(&invite).expect("join");
    // The host must answer while the guest waits for its plan.
    let end = Instant::now() + LONG;
    while guest.events_of("live.plan").is_empty() {
        assert!(Instant::now() < end, "no plan");
        raw.run(50);
    }
    let plan = guest.plan();
    // The refused paths are not even offered for taking.
    for path in [
        ".git",
        ".git/config",
        "manang.toml",
        ".env",
        ".manang/owned",
    ] {
        assert!(!plan.contains_key(path), "{path} is in the plan: {plan:?}");
    }
    assert_eq!(plan.get("fine.txt").map(String::as_str), Some("missing"));
    guest
        .resolve(json!([{ "path": "*", "action": "take-host" }]))
        .or_else(|_| guest.resolve(json!([])))
        .expect("resolve");
    // Let every transfer and every debounced write finish.
    let end = Instant::now() + LONG;
    while guest.read("fine.txt").is_none() {
        assert!(Instant::now() < end, "the honest file never arrived");
        raw.run(50);
    }
    raw.run(2_500);
    assert_eq!(guest.read("fine.txt").as_deref(), Some("an honest file\n"));

    // Nothing was created in protected places.
    assert!(!guest.root.join(".git").exists());
    assert!(!guest.root.join("manang.toml").exists());
    assert!(!guest.root.join(".env").exists());
    assert!(!guest.root.join(".manang/owned").exists());
    assert!(!guest.root.join("node_modules").exists());
    // The symlinks led nowhere: the outside is exactly as it was.
    let listing: Vec<String> = fs::read_dir(outside.path())
        .expect("outside")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(listing, vec!["secret.txt"], "something landed outside");
    assert_eq!(
        fs::read_to_string(outside.path().join("secret.txt")).expect("secret"),
        "do not touch\n"
    );
    // No file outside the project folder, whatever the names said.
    let parent = guest.root.parent().expect("parent");
    for entry in fs::read_dir(parent).expect("parent dir").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            entry.path() == guest.root || !name.contains("traversal"),
            "{name}"
        );
    }
    assert!(!parent.join("traversal").exists());
    // What the plugin refused is on record.
    let skipped = guest.status()["skipped"].to_string();
    assert!(skipped.contains(".git"), "{skipped}");
    assert!(
        skipped.contains("escapes the project through a symbolic link"),
        "the escape through the linked directory was attempted and refused: {skipped}"
    );
    assert!(skipped.contains("evil.txt"), "{skipped}");
    // The links themselves are untouched.
    assert!(
        fs::symlink_metadata(guest.root.join("link"))
            .expect("link")
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::symlink_metadata(guest.root.join("evil.txt"))
            .expect("evil")
            .file_type()
            .is_symlink()
    );
    guest.leave();
}

#[test]
fn a_blob_crosses_in_chunks_and_a_changed_image_replaces_the_old_one() {
    let (_relay, host, guest) = connected_pair();
    host.watch();
    guest.watch();
    settle(400);
    let first = host.read_bytes("fig.png").expect("fig");
    assert_eq!(
        guest.read_bytes("fig.png").as_deref(),
        Some(first.as_slice())
    );
    assert!(first.len() > 65_536, "several chunks");

    // A new version of the image on the host reaches the guest (the last
    // writer wins) and a new image created on the guest reaches the host.
    host.write_bytes("fig.png", &png_like(150_000));
    wait_for(LONG, "the new image on the guest", || {
        guest.read_bytes("fig.png") == Some(png_like(150_000))
    });
    guest.write_bytes("plots/new.png", &png_like(90_001));
    wait_for(LONG, "the guest's image on the host", || {
        host.read_bytes("plots/new.png") == Some(png_like(90_001))
    });
    guest.leave();
    host.leave();
}

#[test]
fn the_host_leaving_does_not_take_the_guests_files_and_the_host_is_warned_what_is_unsynced() {
    let relay = Relay::start();
    let host = Peer::new("Host", Some(&relay.url()), None);
    seed_host(&host);
    let invite = host.start_project();
    // Nobody else has any file yet: all five are unsynced, and the status
    // says so (a warning for the shell, never a block).
    wait_for(LONG, "the first statistics", || {
        host.status()["unsynced_files"] == json!(5)
    });
    assert_eq!(host.status()["files"], json!(5));

    let guest = Peer::new("Guest", Some(&relay.url()), None);
    guest.join_project(&invite).expect("join");
    guest.plan();
    guest.resolve(json!([])).expect("resolve");
    wait_for(LONG, "the guest to have everything", || {
        project_files()
            .iter()
            .all(|(p, t)| guest.read(p).as_deref() == Some(*t))
            && guest.read_bytes("fig.png") == host.read_bytes("fig.png")
    });
    // Once the guest has shown it holds the files, nothing is at risk.
    wait_for(LONG, "the host to learn the guest holds every file", || {
        host.status()["unsynced_files"] == json!(0)
    });
    wait_for(LONG, "the peers event to carry the count", || {
        host.events_of("live.peers").iter().any(|e| {
            e["unsynced_files"] == json!(0) && e["peers"].as_array().is_some_and(|p| !p.is_empty())
        })
    });
    // The host goes; everything stays with the guest, who can still work.
    host.leave();
    guest.attach("main.typ");
    guest.type_at_end("main.typ", "after the host left\n");
    assert!(
        guest
            .view_text("main.typ")
            .ends_with("after the host left\n")
    );
    for (path, text) in project_files() {
        assert_eq!(guest.read(path).as_deref(), Some(text), "{path}");
    }
    assert_eq!(guest.read_bytes("fig.png").expect("fig").len(), 70_008);
    // The host is gone: now it counts as unsynced for anyone left, but the
    // guest keeps its copy: leaving does not delete.
    guest.leave();
    for (path, text) in project_files() {
        assert_eq!(guest.read(path).as_deref(), Some(text), "{path}");
    }
}

#[test]
fn a_late_joiner_sees_the_files_as_they_are_now() {
    let (relay, host, guest) = connected_pair();
    // The guest edits main.typ in a tab; the host's disk follows.
    guest.attach("main.typ");
    guest.type_at_end("main.typ", "edited later\n");
    wait_for(LONG, "the edit on the host's disk", || {
        host.read("main.typ").as_deref() == Some("= Title\nhello é😀\nedited later\n")
    });
    // Let the node's recorded hash catch up with the new text.
    settle(2500);

    // A third person arrives with a copy of the folder as it is now (and one
    // of the original main.typ, in another room of the test below).
    let late = Peer::new("Late", Some(&relay.url()), None);
    let invite = host.call("live.invite", json!({})).expect("invite")["invite"]
        .as_str()
        .expect("invite")
        .to_owned();
    for (path, text) in project_files() {
        late.write(path, text);
    }
    late.write("main.typ", "= Title\nhello é😀\nedited later\n");
    late.write_bytes("fig.png", &png_like(70_000));
    late.join_project(&invite).expect("join");
    let plan = late.plan();
    assert!(
        plan.values().all(|c| c == "identical"),
        "the plan compares with the current text: {plan:?}"
    );
    late.resolve(json!([])).expect("resolve");
    // They are in the session: an edit from the guest reaches the late
    // joiner's disk, who never fetched anything.
    guest.type_at_end("main.typ", "and more\n");
    wait_for(LONG, "the late joiner to follow", || {
        late.read("main.typ").as_deref() == Some("= Title\nhello é😀\nedited later\nand more\n")
    });
    late.leave();
    guest.leave();
    host.leave();
}

#[test]
fn documents_are_parked_in_the_sidecar_and_come_back_with_their_history() {
    let relay = Relay::start();
    // Only two documents fit in memory on either side; six files are shared.
    let host = Peer::new("Host", Some(&relay.url()), Some(2));
    for i in 0..6 {
        host.write(&format!("f{i}.typ"), &format!("file {i}\n"));
    }
    let invite = host.start_project();
    let guest = Peer::new("Guest", Some(&relay.url()), Some(2));
    guest.join_project(&invite).expect("join");
    guest.plan();
    guest.resolve(json!([])).expect("resolve");
    wait_for(LONG, "the guest to have all six files", || {
        (0..6).all(|i| guest.read(&format!("f{i}.typ")).as_deref() == Some(&format!("file {i}\n")))
    });
    wait_for(LONG, "documents to be parked", || {
        host.sidecar_files() >= 3 && guest.sidecar_files() >= 3
    });
    // A parked document no longer confirms anything to the host, but what it
    // handed over counts: nothing is reported unsynced.
    wait_for(LONG, "the host to count the parked files as held", || {
        host.status()["unsynced_files"] == json!(0)
    });

    // Every file is edited in turn from the guest: parked documents are
    // restored, edited and parked again; the host follows each of them
    // without anything having been re-seeded (no duplicated text).
    for i in 0..6 {
        let path = format!("f{i}.typ");
        assert_eq!(guest.attach(&path), format!("file {i}\n"));
        guest.type_at_end(&path, &format!("edit {i}\n"));
        guest
            .call("live.detach", json!({ "path": path }))
            .expect("detach");
    }
    wait_for(LONG, "the host's disk to hold every edit", || {
        (0..6).all(|i| {
            host.read(&format!("f{i}.typ")).as_deref() == Some(&format!("file {i}\nedit {i}\n"))
        })
    });
    // And back: from the host to the guest.
    for i in 0..6 {
        let path = format!("f{i}.typ");
        assert_eq!(host.attach(&path), format!("file {i}\nedit {i}\n"));
        host.type_at_end(&path, &format!("host {i}\n"));
        host.call("live.detach", json!({ "path": path }))
            .expect("detach");
    }
    wait_for(LONG, "the guest's disk to hold every host edit", || {
        (0..6).all(|i| {
            guest.read(&format!("f{i}.typ")).as_deref()
                == Some(&format!("file {i}\nedit {i}\nhost {i}\n"))
        })
    });
    settle(2500);
    for i in 0..6 {
        let want = format!("file {i}\nedit {i}\nhost {i}\n");
        assert_eq!(
            host.read(&format!("f{i}.typ")).as_deref(),
            Some(want.as_str())
        );
        assert_eq!(
            guest.read(&format!("f{i}.typ")).as_deref(),
            Some(want.as_str())
        );
    }
    guest.leave();
    host.leave();
}

impl Peer {
    /// Publish a `project.changed` event by hand (no watcher involved).
    fn changed(&self, changes: Value) {
        self.events.emit(
            "project.changed",
            Payload::Json(json!({ "root": self.root, "changes": changes })),
        );
    }
}

#[test]
fn a_large_rewrite_on_disk_pauses_and_is_shared_or_discarded_on_request() {
    let (_relay, host, guest) = connected_pair();
    // No watcher: the events are published by hand, so the test controls them.
    let names: Vec<String> = (0..30).map(|i| format!("bulk/f{i:02}.typ")).collect();
    for (i, name) in names.iter().enumerate() {
        host.write(name, &format!("original {i}\n"));
    }
    let created: Vec<Value> = names
        .iter()
        .map(|n| json!({ "path": n, "kind": "created" }))
        .collect();
    host.changed(json!(created));
    wait_for(LONG, "the pause to be reported", || {
        !host.events_of("live.paused").is_empty()
    });
    assert_eq!(host.events_of("live.paused")[0]["files"], json!(30));
    assert_eq!(host.status()["paused"]["files"], json!(30));
    // Nothing was shared while it waits for an answer, and the question
    // cannot be skipped by asking for a plain resync.
    settle(1500);
    assert!(guest.read("bulk/f00.typ").is_none());
    let refused = host.call("live.resync", json!({})).expect_err("paused");
    assert_eq!(err_code(&refused), "paused");

    host.call("live.resync", json!({ "paused": "share" }))
        .expect("share");
    wait_for(LONG, "the shared files on the guest", || {
        names
            .iter()
            .enumerate()
            .all(|(i, n)| guest.read(n).as_deref() == Some(&format!("original {i}\n")))
    });
    assert_eq!(host.status()["paused"], Value::Null);

    // A second rewrite, this time discarded: the disk goes back to the
    // shared state and the rewritten copies are kept in the trash.
    for (i, name) in names.iter().enumerate() {
        host.write(name, &format!("rewritten {i}\n"));
    }
    let modified: Vec<Value> = names
        .iter()
        .map(|n| json!({ "path": n, "kind": "modified" }))
        .collect();
    host.changed(json!(modified));
    wait_for(LONG, "the second pause", || {
        host.events_of("live.paused").len() == 2
    });
    host.call("live.resync", json!({ "paused": "discard" }))
        .expect("discard");
    wait_for(LONG, "the disk to go back", || {
        names
            .iter()
            .enumerate()
            .all(|(i, n)| host.read(n).as_deref() == Some(&format!("original {i}\n")))
    });
    assert_eq!(host.trash_text("f07.typ"), vec!["rewritten 7\n"]);
    settle(1000);
    assert_eq!(guest.read("bulk/f07.typ").as_deref(), Some("original 7\n"));
    guest.leave();
    host.leave();
}

#[test]
fn commands_know_which_kind_of_session_they_are_in() {
    let relay = Relay::start();
    let host = Peer::new("Host", Some(&relay.url()), None);
    // Project commands without a session, and project sharing without a folder.
    let none = host.call("live.files", json!({})).expect_err("not live");
    assert_eq!(err_code(&none), "not-live");
    let none = host
        .call("live.start", json!({ "scope": "project" }))
        .expect_err("no folder");
    assert_eq!(err_code(&none), "no-project");

    // A single-file session keeps its behaviour, and says it is one.
    let doc = {
        let out = host.call("editor.new", json!({})).expect("editor");
        out["id"].as_u64().expect("doc")
    };
    host.call(
        "editor.insert",
        json!({ "id": doc, "at": 0, "text": "hello\n" }),
    )
    .expect("insert");
    let started = host
        .call("live.start", json!({ "doc": doc, "file": "main.typ" }))
        .expect("start");
    let invite = started["invite"].as_str().expect("invite").to_owned();
    assert!(!invite.contains("scope="), "{invite}");
    let status = host.status();
    assert_eq!(status["scope"], json!("file"));
    for command in ["live.files", "live.resolve", "live.follow"] {
        let refused = host
            .call(command, json!({ "choices": [], "peer": null }))
            .expect_err("not a project");
        assert_eq!(err_code(&refused), "not-project", "{command}");
    }
    let inspected = host
        .call("live.inspect", json!({ "invite": invite }))
        .expect("inspect");
    assert_eq!(inspected["scope"], json!("file"));
    host.leave();

    // A project session: the invite is v2, inspect reads its scope, follow
    // echoes (it is local state), and a push needs its file.
    seed_host(&host);
    let invite = host.start_project();
    assert!(
        invite.starts_with("manang-live://join?v=2&scope=project&relay="),
        "{invite}"
    );
    let inspected = host
        .call("live.inspect", json!({ "invite": invite }))
        .expect("inspect");
    assert_eq!(inspected["scope"], json!("project"));
    assert_eq!(
        host.call("live.follow", json!({ "peer": "123" }))
            .expect("follow"),
        json!({ "following": "123" })
    );
    assert_eq!(host.status()["following"], json!("123"));
    assert_eq!(
        host.call("live.follow", json!({ "peer": null }))
            .expect("unfollow"),
        json!({ "following": null })
    );
    let no_path = host
        .call("live.push", json!({ "version": 0, "changes": [] }))
        .expect_err("needs a path");
    assert_eq!(err_code(&no_path), "invalid");
    let unknown = host
        .call("live.attach", json!({ "path": "nowhere.typ" }))
        .expect_err("not in the session");
    assert_eq!(err_code(&unknown), "not-in-session");
    let binary = host
        .call("live.attach", json!({ "path": "fig.png" }))
        .expect_err("not text");
    assert_eq!(err_code(&binary), "not-text");
    let status = host.status();
    assert_eq!(status["scope"], json!("project"));
    assert_eq!(status["role"], json!("host"));
    // A second session is refused, as before.
    let again = host
        .call(
            "live.start",
            json!({ "scope": "project", "root": host.root }),
        )
        .expect_err("already live");
    assert_eq!(err_code(&again), "already-live");
    host.leave();
}

#[test]
fn the_manifest_lists_the_commands_the_frontend_mock_lists() {
    // The same twenty commands as `src/lib/kernel/mockLive.ts`.
    let mut expected = vec![
        "live.attach",
        "live.configure",
        "live.contributors",
        "live.detach",
        "live.exclude",
        "live.files",
        "live.follow",
        "live.include",
        "live.inspect",
        "live.invite",
        "live.join",
        "live.leave",
        "live.push",
        "live.resolve",
        "live.restore",
        "live.resync",
        "live.selection",
        "live.snapshot",
        "live.start",
        "live.status",
    ];
    expected.sort_unstable();
    let manifest = manang_live_plugin::manifest().expect("manifest");
    let mut ids: Vec<&str> = manifest
        .contributes
        .commands
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, expected);
}

#[test]
fn people_and_carets_name_the_file_they_are_in() {
    let (_relay, host, guest) = connected_pair();
    host.attach("main.typ");
    guest.attach("main.typ");
    // The guest has the file in front of them and a caret in it.
    let sid = guest.sid.lock().clone();
    guest
        .call(
            "live.selection",
            json!({ "session": sid, "path": "main.typ", "version": 0, "anchor": 2, "head": 7 }),
        )
        .expect("selection");
    wait_for(
        LONG,
        "the host to see the guest's caret in main.typ",
        || {
            host.events_of("live.presence").iter().any(|e| {
                e["path"] == "main.typ"
                    && e["selections"].as_array().is_some_and(|s| {
                        s.iter()
                            .any(|c| c["anchor"] == 2 && c["head"] == 7 && c["name"] == "Guest")
                    })
            })
        },
    );
    wait_for(LONG, "the host's people list to name the file", || {
        host.events_of("live.peers").iter().any(|e| {
            e["peers"].as_array().is_some_and(|p| {
                p.iter()
                    .any(|x| x["name"] == "Guest" && x["path"] == "main.typ")
            })
        })
    });
    // Joined notices came with the people.
    assert!(
        host.events_of("live.notice")
            .iter()
            .any(|n| n["kind"] == "joined"),
        "a joined notice"
    );
    // The file list says who has what open.
    let files = host.call("live.files", json!({})).expect("files");
    let main = files["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|f| f["path"] == "main.typ")
        .expect("main");
    assert_eq!(main["peers"].as_array().map(Vec::len), Some(1));
    // Re-reading an open file gives its version and text.
    let snap = host
        .call(
            "live.snapshot",
            json!({ "session": host.sid.lock().clone(), "path": "main.typ" }),
        )
        .expect("snapshot");
    assert_eq!(snap["text"], json!("= Title\nhello é😀\n"));
    // The guest closes the tab: the host's list no longer puts them in it.
    guest
        .call("live.detach", json!({ "path": "main.typ" }))
        .expect("detach");
    wait_for(LONG, "the guest to leave the file", || {
        host.events_of("live.peers").last().is_some_and(|e| {
            e["peers"]
                .as_array()
                .is_some_and(|p| p.iter().all(|x| x["path"] != "main.typ"))
        })
    });
    guest.leave();
    host.leave();
}

#[test]
fn a_file_deleted_or_renamed_under_an_open_tab_keeps_the_tab_working() {
    let (_relay, host, guest) = connected_pair();
    host.watch();
    guest.watch();
    settle(400);
    guest.attach("notes.md");
    guest.attach("chapters/two.typ");

    // A rename under the open tab: the tab follows the new path.
    fs::rename(
        host.root.join("chapters/two.typ"),
        host.root.join("chapters/zwei.typ"),
    )
    .expect("rename");
    wait_for(LONG, "the rename to reach the guest's tree", || {
        guest.events_of("live.tree").iter().any(|e| {
            e["ops"].as_array().is_some_and(|ops| {
                ops.iter()
                    .any(|o| o["from"] == "chapters/two.typ" && o["path"] == "chapters/zwei.typ")
            })
        })
    });
    // Pushes name the new path from now on.
    let sid = guest.sid.lock().clone();
    let out = guest
        .call(
            "live.push",
            json!({
                "session": sid,
                "path": "chapters/zwei.typ",
                "version": 0,
                "changes": [{ "start": 0, "delete": 0, "insert": "x" }],
            }),
        )
        .expect("push to the new path");
    assert_eq!(out["accepted"], json!(true));

    // A delete under the open tab: the tab keeps its text, the file stays on
    // disk until the tab closes, and the person is told.
    fs::remove_file(host.root.join("notes.md")).expect("delete");
    wait_for(LONG, "the delete notice", || {
        guest
            .events_of("live.notice")
            .iter()
            .any(|n| n["kind"] == "deleted" && n["path"] == "notes.md")
    });
    settle(800);
    assert_eq!(guest.read("notes.md").as_deref(), Some("# notes\n"));
    // Restoring brings it back for everyone, from what the tab holds.
    guest
        .call("live.restore", json!({ "path": "notes.md" }))
        .expect("restore");
    wait_for(LONG, "the restored file on the host", || {
        host.read("notes.md").as_deref() == Some("# notes\n")
    });
    // The tab still works on the restored file.
    guest.type_at_end("notes.md", "after the restore\n");
    guest
        .call("live.detach", json!({ "path": "notes.md" }))
        .expect("detach");
    wait_for(LONG, "the edit after the restore to reach the host", || {
        host.read("notes.md").as_deref() == Some("# notes\nafter the restore\n")
    });
    // The file was not moved to the trash by the closing tab.
    assert!(guest.trash_text("notes.md").is_empty());
    guest.leave();
    host.leave();
}

#[test]
fn edits_made_while_the_relay_is_down_merge_when_it_comes_back() {
    let (mut relay, host, guest) = {
        let (relay, host, invite) = host_with_project();
        let guest = Peer::new("Guest", Some(&relay.url()), None);
        guest.join_project(&invite).expect("join");
        guest.plan();
        guest.resolve(json!([])).expect("resolve");
        wait_for(LONG, "the guest to have every file", || {
            guest.read("main.typ").is_some() && guest.read_bytes("fig.png").is_some()
        });
        (relay, host, guest)
    };
    host.attach("main.typ");
    guest.attach("main.typ");
    relay.stop();
    wait_for(LONG, "both to notice", || {
        host.status()["state"] == json!("reconnecting")
            && guest.status()["state"] == json!("reconnecting")
    });
    // Both type in the same file while apart.
    host.type_at_end("main.typ", "host, offline\n");
    guest.type_at_end("main.typ", "guest, offline\n");
    relay.restart();
    wait_for(LONG, "both to be connected again", || {
        host.status()["state"] == json!("connected")
            && guest.status()["state"] == json!("connected")
    });
    wait_for(LONG, "the views to converge", || {
        let (a, b) = (host.view_text("main.typ"), guest.view_text("main.typ"));
        a == b && a.contains("host, offline") && a.contains("guest, offline")
    });
    guest.leave();
    host.leave();
}

#[test]
fn a_file_edited_on_disk_while_parked_is_shared_without_losing_its_history() {
    let relay = Relay::start();
    let host = Peer::new("Host", Some(&relay.url()), Some(2));
    for i in 0..6 {
        host.write(&format!("f{i}.typ"), &format!("file {i}\n"));
    }
    let invite = host.start_project();
    let guest = Peer::new("Guest", Some(&relay.url()), Some(2));
    guest.join_project(&invite).expect("join");
    guest.plan();
    guest.resolve(json!([])).expect("resolve");
    wait_for(LONG, "everything to settle", || {
        (0..6).all(|i| guest.read(&format!("f{i}.typ")).is_some())
            && host.sidecar_files() >= 3
            && guest.sidecar_files() >= 3
    });
    host.watch();
    guest.watch();
    settle(400);
    // An outside tool rewrites files whose documents are parked on both sides.
    for i in [0, 1, 2] {
        host.write(&format!("f{i}.typ"), &format!("file {i}\nedited outside\n"));
        guest.write(
            &format!("f{}.typ", i + 3),
            &format!("file {}\nguest outside\n", i + 3),
        );
    }
    wait_for(LONG, "the outside edits to cross over", || {
        (0..3).all(|i| {
            guest.read(&format!("f{i}.typ")).as_deref()
                == Some(&format!("file {i}\nedited outside\n"))
        }) && (3..6).all(|i| {
            host.read(&format!("f{i}.typ")).as_deref()
                == Some(&format!("file {i}\nguest outside\n"))
        })
    });
    guest.leave();
    host.leave();
}

#[test]
fn a_project_of_many_files_joins_within_the_memory_budget() {
    let relay = Relay::start();
    let host = Peer::new("Host", Some(&relay.url()), Some(8));
    for i in 0..150 {
        host.write(
            &format!("part{}/file{i:03}.typ", i % 7),
            &format!("= File {i}\n{}\n", "text ".repeat(40)),
        );
    }
    host.write_bytes("big.bin", &png_like(300_000));
    let invite = host.start_project();
    let guest = Peer::new("Guest", Some(&relay.url()), Some(8));
    let started = Instant::now();
    guest.join_project(&invite).expect("join");
    let plan = guest.plan();
    assert_eq!(plan.len(), 151);
    guest.resolve(json!([])).expect("resolve");
    wait_for(
        Duration::from_secs(60),
        "all 150 files and the image",
        || {
            (0..150).all(|i| {
                guest
                    .read(&format!("part{}/file{i:03}.typ", i % 7))
                    .is_some_and(|t| t.starts_with(&format!("= File {i}\n")))
            }) && guest
                .read_bytes("big.bin")
                .is_some_and(|b| b.len() == 300_008)
        },
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "150 files joined in {:?}",
        started.elapsed()
    );
    // Progress ran to the end.
    wait_for(LONG, "progress to finish", || {
        guest
            .events_of("live.progress")
            .last()
            .is_some_and(|p| p["done"] == p["total"])
    });
    // Almost everything is parked: only a handful of documents are in memory.
    wait_for(LONG, "documents to be parked", || {
        guest.sidecar_files() >= 100
    });
    guest.leave();
    host.leave();
}
