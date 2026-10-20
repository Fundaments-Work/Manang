//! Two kernel clients through a real relay, no UI.
//!
//! Each `Client` is a whole kernel (editor plugin + live plugin) and plays the
//! web view's part of the edit protocol (see `driver.rs`): it keeps a model of
//! the view's document, pushes edits with the version it has seen, retries when
//! told it is stale, and applies the `live.remote` events in order. The relay
//! runs in-process on a random port and records every frame it forwards.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_editor::{Editor, EditorPlugin};
use manang_kernel::{EventBus, Kernel, Payload, Subscription};
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{OsEntropy, RoomSecret};
use manang_live::session::{Input, Output, Session, SessionConfig};
use manang_live::transport::{SyncTransport, TransportEvent};
use manang_live::{LoroEngine, TextCrdt};
use manang_live_plugin::LivePlugin;
use manang_relay::{RelayClient, RelayConfig, RunningRelay};
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::{Value, json};
use tokio::runtime::Runtime;

const SEED: &str = "Abstract. We study the méthode of confidential reviewers 😀.\n";

struct Relay {
    rt: Runtime,
    running: Option<RunningRelay>,
    addr: SocketAddr,
    frames: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Relay {
    fn start() -> Self {
        let rt = Runtime::new().expect("runtime");
        let frames: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let (running, addr) = Self::bind(&rt, "127.0.0.1:0".parse().expect("addr"), &frames);
        Self {
            rt,
            running: Some(running),
            addr,
            frames,
        }
    }

    fn bind(
        rt: &Runtime,
        addr: SocketAddr,
        frames: &Arc<Mutex<Vec<Vec<u8>>>>,
    ) -> (RunningRelay, SocketAddr) {
        let store = Arc::clone(frames);
        let config = RelayConfig {
            tap: Some(Arc::new(move |frame: &[u8]| {
                store.lock().push(frame.to_vec())
            })),
            ..RelayConfig::default()
        };
        let running = rt
            .block_on(manang_relay::spawn(addr, config))
            .expect("relay binds");
        let bound = running.addr;
        (running, bound)
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
        let (running, _) = Self::bind(&self.rt, self.addr, &self.frames);
        self.running = Some(running);
    }
}

struct Client {
    kernel: Kernel,
    editor: Arc<Editor>,
    sub: Subscription,
    doc: u64,
    sid: String,
    version: u64,
    /// What the web view would show, UTF-16 like CodeMirror.
    view: Vec<u16>,
    held: BTreeMap<u64, Value>,
    states: Vec<String>,
    presence: Vec<Value>,
    config_dir: tempfile::TempDir,
}

fn err_code(error: &str) -> String {
    serde_json::from_str::<Value>(error)
        .ok()
        .and_then(|v| v.get("code").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| error.to_owned())
}

impl Client {
    fn new(name: &str, relay: Option<&str>, text: &str) -> Self {
        let events = EventBus::new();
        let editor = Arc::new(Editor::new());
        let config_dir = tempfile::tempdir().expect("tmp");
        let kernel = Kernel::new();
        kernel
            .register(
                manang_editor::manifest().expect("editor manifest"),
                Arc::new(EditorPlugin::new(Arc::clone(&editor))),
            )
            .expect("register editor");
        kernel
            .register(
                manang_live_plugin::manifest().expect("live manifest"),
                Arc::new(LivePlugin::new(
                    Arc::clone(&editor),
                    events.clone(),
                    config_dir.path().to_path_buf(),
                )),
            )
            .expect("register live");
        let sub = events.subscribe("live.*");
        let doc = editor.create();
        if !text.is_empty() {
            editor.insert(Some(doc), 0, text).expect("seed text");
        }
        let client = Self {
            kernel,
            editor,
            sub,
            doc,
            sid: String::new(),
            version: 0,
            view: text.encode_utf16().collect(),
            held: BTreeMap::new(),
            states: Vec::new(),
            presence: Vec::new(),
            config_dir,
        };
        let mut values = json!({ "live.display-name": name });
        if let Some(relay) = relay {
            values["live.relay-url"] = json!(relay);
        }
        client
            .call("live.configure", json!({ "values": values }))
            .expect("configure");
        client
    }

    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(_) => Ok(Value::Null),
            Err(e) => Err(e.to_string()),
        }
    }

    fn start(&mut self) -> String {
        let out = self
            .call("live.start", json!({ "doc": self.doc, "file": "main.typ" }))
            .expect("start");
        self.sid = out["session"].as_str().expect("session").to_owned();
        self.version = 0;
        out["invite"].as_str().expect("invite").to_owned()
    }

    fn join(&mut self, invite: &str, resolution: Option<&str>) -> Result<(), String> {
        let mut args = json!({ "doc": self.doc, "invite": invite, "name": "main.typ" });
        if let Some(r) = resolution {
            args["resolution"] = json!(r);
        }
        let out = self.call("live.join", args)?;
        self.sid = out["session"].as_str().expect("session").to_owned();
        self.version = out["version"].as_u64().expect("version");
        if let Some(reset) = out.get("reset").filter(|r| !r.is_null()) {
            self.apply(reset);
        }
        Ok(())
    }

    fn apply(&mut self, change: &Value) {
        let start = change["start"].as_u64().expect("start") as usize;
        let delete = change["delete"].as_u64().expect("delete") as usize;
        let insert: Vec<u16> = change["insert"]
            .as_str()
            .expect("insert")
            .encode_utf16()
            .collect();
        self.view.splice(start..start + delete, insert);
    }

    fn view_text(&self) -> String {
        String::from_utf16(&self.view).expect("the view model stays valid UTF-16")
    }

    fn kernel_text(&self) -> String {
        self.editor.text(Some(self.doc)).expect("text")
    }

    /// Read events for up to `wait`, applying them in order.
    fn process(&mut self, wait: Duration) {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(event) = self.sub.recv_timeout(left.min(Duration::from_millis(20))) else {
                if Instant::now() >= deadline {
                    return;
                }
                continue;
            };
            let Payload::Json(body) = event.payload else {
                continue;
            };
            if body["session"] != json!(self.sid) {
                continue;
            }
            match event.topic.as_str() {
                "live.remote" => {
                    let version = body["version"].as_u64().expect("version");
                    self.held.insert(version, body);
                    while let Some(next) = self.held.remove(&(self.version + 1)) {
                        for change in next["changes"].as_array().expect("changes") {
                            self.apply(change);
                        }
                        self.version += 1;
                    }
                }
                "live.state" => self
                    .states
                    .push(body["state"].as_str().expect("state").to_owned()),
                "live.presence" => {
                    self.presence = body["selections"].as_array().cloned().unwrap_or_default();
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return;
            }
        }
    }

    fn catch_up(&mut self, version: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.version < version {
            assert!(
                Instant::now() < deadline,
                "never saw version {version}; at {}",
                self.version
            );
            self.process(Duration::from_millis(20));
        }
    }

    /// Push one edit computed from the current view; retries when stale.
    fn edit(&mut self, make: impl Fn(&[u16]) -> (usize, usize, String)) {
        for _ in 0..200 {
            let (start, delete, insert) = make(&self.view);
            let out = self
                .call(
                    "live.push",
                    json!({
                        "session": self.sid,
                        "version": self.version,
                        "changes": [{ "start": start, "delete": delete, "insert": insert }],
                    }),
                )
                .expect("push");
            let version = out["version"].as_u64().expect("version");
            if out["accepted"] == json!(true) {
                self.apply(&json!({ "start": start, "delete": delete, "insert": insert }));
                self.catch_up(version);
                return;
            }
            self.catch_up(version);
        }
        panic!("edit never accepted");
    }

    fn insert_at_start(&mut self, text: &str) {
        let text = text.to_owned();
        self.edit(move |_| (0, 0, text.clone()));
    }

    fn insert_at_end(&mut self, text: &str) {
        let text = text.to_owned();
        self.edit(move |view| (view.len(), 0, text.clone()));
    }

    fn status(&self) -> Value {
        self.call("live.status", json!({})).expect("status")
    }

    fn leave(&mut self) {
        self.call("live.leave", json!({})).expect("leave");
    }
}

fn converge(clients: &mut [&mut Client], limit: Duration) {
    let end = Instant::now() + limit;
    loop {
        for c in clients.iter_mut() {
            c.process(Duration::from_millis(15));
        }
        let first = clients[0].kernel_text();
        if clients
            .iter()
            .all(|c| c.kernel_text() == first && c.view_text() == first)
        {
            return;
        }
        assert!(
            Instant::now() < end,
            "no convergence: {:?}",
            clients
                .iter()
                .map(|c| (c.kernel_text(), c.view_text()))
                .collect::<Vec<_>>()
        );
    }
}

fn wait_for(limit: Duration, what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn start_needs_a_relay_and_one_session_at_a_time() {
    let mut a = Client::new("ada", None, SEED);
    // The default is the relay run by Fundaments; emptying it turns sharing off.
    assert_eq!(a.status()["relay_set"], json!(true));
    a.call(
        "live.configure",
        json!({ "values": { "live.relay-url": "" } }),
    )
    .expect("clear the relay");
    let err = a
        .call("live.start", json!({ "doc": a.doc }))
        .expect_err("no relay set");
    assert_eq!(err_code(&err), "no-relay");
    assert_eq!(a.status()["relay_set"], json!(false));

    let relay = Relay::start();
    a.call(
        "live.configure",
        json!({ "values": { "live.relay-url": relay.url() } }),
    )
    .expect("configure");
    a.start();
    let err = a
        .call("live.start", json!({ "doc": a.doc }))
        .expect_err("already live");
    assert_eq!(err_code(&err), "already-live");
    assert_eq!(a.status()["active"], json!(true));
    a.leave();
    assert_eq!(a.status()["active"], json!(false));
    // Leaving twice is harmless.
    assert_eq!(
        a.call("live.leave", json!({})).expect("leave")["left"],
        json!(false)
    );
}

#[test]
fn bad_invites_and_bad_pushes_are_errors() {
    let relay = Relay::start();
    let mut a = Client::new("ada", Some(&relay.url()), SEED);
    for invite in [
        "",
        "nonsense",
        "manang-live://join?relay=ws%3A%2F%2Fx#short",
    ] {
        let err = a
            .call("live.join", json!({ "doc": a.doc, "invite": invite }))
            .expect_err("bad invite");
        assert_eq!(err_code(&err), "bad-invite", "{invite:?}");
    }
    a.start();
    // Stale version: refused, nothing applied.
    let out = a
        .call(
            "live.push",
            json!({ "session": a.sid, "version": 7, "changes": [{ "start": 0, "insert": "x" }] }),
        )
        .expect("push");
    assert_eq!(out["accepted"], json!(false));
    assert_eq!(a.kernel_text(), SEED);
    // Out of range: an error, and the snapshot still names a consistent text.
    let err = a
        .call("live.push", json!({ "session": a.sid, "version": 0, "changes": [{ "start": 9999, "insert": "x" }] }))
        .expect_err("out of range");
    assert_eq!(err_code(&err), "invalid");
    let snap = a.call("live.snapshot", json!({})).expect("snapshot");
    assert_eq!(snap["text"], json!(SEED));
    // A split surrogate pair is refused too.
    let emoji = SEED
        .encode_utf16()
        .position(|u| (0xD800..0xDC00).contains(&u))
        .expect("emoji");
    let err = a
        .call(
            "live.push",
            json!({ "session": a.sid, "version": snap["version"], "changes": [{ "start": emoji + 1, "delete": 1, "insert": "" }] }),
        )
        .expect_err("inside a pair");
    assert_eq!(err_code(&err), "core");
    a.leave();
}

#[test]
fn two_kernel_clients_converge_and_the_relay_sees_only_ciphertext() {
    let relay = Relay::start();
    let mut host = Client::new("Ada Lovelace", Some(&relay.url()), SEED);
    let mut guest = Client::new("Grace Hopper", None, SEED);
    let invite = host.start();
    // The guest has no relay of its own: the invite carries it.
    guest.join(&invite, None).expect("same file: seeds locally");
    wait_for(Duration::from_secs(10), "both connected", || {
        host.status()["state"] == json!("connected")
            && guest.status()["state"] == json!("connected")
    });

    // Concurrent typing at both ends, including astral characters.
    for i in 0..15 {
        host.insert_at_start(&format!("H{i}😀 "));
        guest.insert_at_end(&format!(" G{i}é"));
    }
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));
    let merged = host.kernel_text();
    for i in 0..15 {
        assert!(merged.contains(&format!("H{i}😀 ")) && merged.contains(&format!(" G{i}é")));
    }
    // Deleting a surrogate pair whole is fine.
    let at = host
        .view
        .iter()
        .position(|u| (0xD800..0xDC00).contains(u))
        .expect("emoji");
    host.edit(move |_| (at, 2, String::new()));
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));
    // The file on disk is plain text: the shared copy is just the text.
    assert_eq!(host.kernel_text(), guest.kernel_text());

    // Carets: the guest's selection reaches the host, resolved to UTF-16.
    let version = guest.version;
    guest
        .call(
            "live.selection",
            json!({ "session": guest.sid, "version": version, "anchor": 3, "head": 7 }),
        )
        .expect("selection");
    let end = Instant::now() + Duration::from_secs(10);
    while !host.presence.iter().any(|s| {
        s["name"] == json!("Grace Hopper") && s["anchor"] == json!(3) && s["head"] == json!(7)
    }) {
        assert!(
            Instant::now() < end,
            "the host never saw the guest's caret: {:?}; guest view version {}; status {}; guest status {}",
            host.presence,
            guest.version,
            host.status(),
            guest.status()
        );
        host.process(Duration::from_millis(20));
    }
    let peers = host.status()["peers"].clone();
    assert_eq!(peers[0]["name"], json!("Grace Hopper"));

    // Leaving tells the room.
    guest.leave();
    wait_for(
        Duration::from_secs(10),
        "the host to see the guest leave",
        || host.status()["peers"].as_array().is_some_and(Vec::is_empty),
    );
    guest.process(Duration::from_millis(100));
    assert_eq!(guest.states.last().map(String::as_str), Some("left"));
    host.leave();

    // The relay forwarded real traffic, and none of it is readable.
    let frames = relay.frames.lock().clone();
    assert!(frames.len() >= 20, "expected traffic, saw {}", frames.len());
    let secrets = [
        "Abstract",
        "méthode",
        "confidential",
        "reviewers",
        "Ada Lovelace",
        "Grace Hopper",
        "main.typ",
    ];
    // Only markers long enough that random ciphertext cannot contain them by
    // chance: a 2-byte marker appears in any few kilobytes of random bytes, so
    // checking one would fail the test at random (it did, in CI).
    for frame in &frames {
        assert!(frame.len() >= 42 && frame[0] == 1 && (frame[1] == 1 || frame[1] == 2));
        for s in secrets {
            assert!(
                !frame.windows(s.len()).any(|w| w == s.as_bytes()),
                "the relay saw {s:?} in the clear"
            );
        }
    }
    // The invite's key never appears on the wire either.
    let fragment = invite.split_once('#').expect("fragment").1;
    for frame in &frames {
        assert!(
            !frame
                .windows(fragment.len())
                .any(|w| w == fragment.as_bytes())
        );
    }
}

#[test]
fn edits_made_while_the_relay_is_down_merge_on_reconnect() {
    let mut relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, SEED);
    let invite = host.start();
    guest.join(&invite, None).expect("join");
    wait_for(Duration::from_secs(10), "connected", || {
        host.status()["state"] == json!("connected")
            && guest.status()["state"] == json!("connected")
    });
    host.insert_at_start("ONLINE ");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));

    relay.stop();
    wait_for(Duration::from_secs(10), "both to notice", || {
        host.status()["state"] == json!("reconnecting")
            && guest.status()["state"] == json!("reconnecting")
    });
    // Both keep typing while apart: the edits are accepted locally.
    host.insert_at_start("OFFLINE-HOST ");
    guest.insert_at_end(" OFFLINE-GUEST");
    assert_ne!(host.kernel_text(), guest.kernel_text());

    relay.restart();
    converge(&mut [&mut host, &mut guest], Duration::from_secs(30));
    let merged = host.kernel_text();
    assert!(
        merged.starts_with("OFFLINE-HOST ONLINE Abstract"),
        "{merged:?}"
    );
    assert!(merged.ends_with(" OFFLINE-GUEST"), "{merged:?}");
    wait_for(Duration::from_secs(10), "connected again", || {
        host.process(Duration::from_millis(20));
        guest.process(Duration::from_millis(20));
        host.status()["state"] == json!("connected")
            && guest.status()["state"] == json!("connected")
    });
    assert!(host.states.iter().any(|s| s == "reconnecting"));
    host.leave();
    guest.leave();
}

#[test]
fn leave_and_rejoin_never_replaces_a_copy_silently() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, SEED);
    let invite = host.start();
    guest.join(&invite, None).expect("join");
    wait_for(Duration::from_secs(10), "connected", || {
        host.status()["state"] == json!("connected")
            && guest.status()["state"] == json!("connected")
    });
    guest.insert_at_end("guest line\n");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));
    guest.leave();
    assert_eq!(guest.status()["active"], json!(false));
    // The guest edits the file on its own while away, so its copy is no longer
    // the room's last state (a rejoin would otherwise restore the snapshot).
    guest
        .editor
        .insert(Some(guest.doc), 0, "offline note\n")
        .expect("local edit");
    guest.view = guest.kernel_text().encode_utf16().collect();

    // The host carries on; the guest's file is now behind.
    host.insert_at_end("host only\n");
    let hosted = host.kernel_text();

    // Rejoining with a different copy asks first.
    let err = guest.join(&invite, None).expect_err("copies differ");
    assert_eq!(err_code(&err), "differs");
    assert_eq!(guest.status()["active"], json!(false), "nothing started");
    assert!(
        guest.kernel_text().contains("guest line"),
        "nothing replaced"
    );
    assert!(guest.kernel_text().starts_with("offline note"));

    // Confirmed: the host's version wins, and the old copy is saved first.
    guest.join(&invite, Some("take-host")).expect("take host");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(20));
    assert_eq!(guest.kernel_text(), hosted);
    let saved: Vec<_> = std::fs::read_dir(guest.config_dir.path().join("live-trash"))
        .expect("trash dir")
        .collect();
    assert_eq!(saved.len(), 1);
    let kept = std::fs::read_to_string(saved[0].as_ref().expect("entry").path()).expect("read");
    assert!(kept.contains("guest line") && !kept.contains("host only"));

    // And an identical copy rejoins without asking.
    guest.leave();
    host.insert_at_end("more\n");
    let current = host.kernel_text();
    let mut third = Client::new("third", None, &current);
    // The invite's seed is the host's *start* text, so even this differs, but
    // an empty file never needs confirming.
    let mut empty = Client::new("empty", None, "");
    empty
        .join(&invite, None)
        .expect("an empty copy has nothing to lose");
    converge(&mut [&mut host, &mut empty], Duration::from_secs(20));
    assert_eq!(empty.kernel_text(), current);
    assert_eq!(
        err_code(&third.join(&invite, None).expect_err("differs")),
        "differs"
    );
    host.leave();
    empty.leave();
}

#[test]
fn a_wrong_key_cannot_read_or_join_the_conversation() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let invite = host.start();
    let mut guest = Client::new("guest", None, SEED);
    guest.join(&invite, None).expect("join");
    wait_for(Duration::from_secs(10), "connected", || {
        host.status()["state"] == json!("connected")
            && guest.status()["state"] == json!("connected")
    });

    // An eavesdropper with the room URL (the relay's view) but not the key.
    let link = manang_live_plugin::InviteLink::parse(&invite).expect("parse");
    let right = manang_live::envelope::Invite::from_fragment(&link.fragment)
        .expect("fragment")
        .secret
        .derive()
        .expect("keys");
    let room_url = format!("{}/room/{}", link.relay, right.room_id_hex());
    let rt = Runtime::new().expect("runtime");
    let mut snoop = RelayClient::new(room_url, rt.handle().clone());
    snoop.connect().expect("connect");

    let wrong = RoomSecret::generate(&mut OsEntropy)
        .expect("secret")
        .derive()
        .expect("keys");
    let mut session = Session::new(
        LiveDoc::<LoroEngine>::empty(4242).expect("doc"),
        RoomSecret::generate(&mut OsEntropy)
            .expect("secret")
            .derive()
            .expect("keys"),
        OsEntropy,
        SessionConfig::default(),
    );
    let mut rope = Rope::new();

    host.insert_at_end("SECRET-LINE\n");
    let end = Instant::now() + Duration::from_secs(5);
    let mut frames = 0;
    while Instant::now() < end {
        host.process(Duration::from_millis(10));
        guest.process(Duration::from_millis(10));
        while let Some(event) = snoop.poll() {
            if let TransportEvent::Frame(frame) = event {
                frames += 1;
                assert!(
                    wrong.open(&frame).is_err(),
                    "a stranger's key opened a frame"
                );
                assert!(right.open(&frame).is_ok(), "the room's own key opens it");
                let out = session.handle(0, Input::Frame(&frame), &mut rope);
                assert!(
                    out.iter().all(|o| matches!(o, Output::Rejected(_))),
                    "a frame sealed for another key must only be rejected: {out:?}"
                );
            }
        }
    }
    assert!(frames >= 2, "the snooper saw {frames} frames");
    assert_eq!(
        rope.len_chars(),
        0,
        "nothing was applied from unreadable frames"
    );
    assert_eq!(session.doc().engine().text(), "");
    host.leave();
    guest.leave();
}

fn wait_state(client: &Client, state: &str, limit: Duration) {
    wait_for(limit, state, || client.status()["state"] == json!(state));
}

#[test]
fn joining_an_empty_room_keeps_the_file_and_never_says_live() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, "My own draft.\n");
    let invite = host.start();
    host.leave(); // nobody is home

    guest.join(&invite, Some("take-host")).expect("join");
    for _ in 0..80 {
        guest.process(Duration::from_millis(50));
        assert_eq!(
            guest.status()["state"],
            json!("waiting"),
            "states so far: {:?}",
            guest.states
        );
    }
    assert!(!guest.states.iter().any(|s| s == "connected"));
    assert_eq!(
        guest.kernel_text(),
        "My own draft.\n",
        "the file is untouched"
    );
    assert_eq!(guest.view_text(), "My own draft.\n");

    // Typing is refused while waiting, with a code the shell understands.
    let pushed = guest.call(
        "live.push",
        json!({
            "session": guest.sid, "version": 0,
            "changes": [{ "start": 0, "delete": 0, "insert": "x" }],
        }),
    );
    assert_eq!(err_code(&pushed.expect_err("refused")), "waiting");
    assert_eq!(guest.kernel_text(), "My own draft.\n");

    // The old copy was saved up front.
    let saved: Vec<_> = std::fs::read_dir(guest.config_dir.path().join("live-trash"))
        .expect("trash dir")
        .collect();
    assert_eq!(saved.len(), 1);
    guest.leave();
}

#[test]
fn a_late_host_answers_a_waiting_joiner_and_only_then_the_file_is_replaced() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, "My own draft.\n");
    let invite = host.start();
    host.insert_at_end("A host line.\n");
    host.leave();

    guest.join(&invite, Some("take-host")).expect("join");
    wait_state(&guest, "waiting", Duration::from_secs(5));
    assert_eq!(guest.kernel_text(), "My own draft.\n");

    // The host comes back through its own invite: its file equals the snapshot
    // it left behind, so the room is reseeded from it.
    host.join(&invite, None).expect("the host rejoins");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(30));
    assert_eq!(guest.kernel_text(), host.kernel_text());
    assert!(guest.kernel_text().contains("A host line."));
    wait_state(&guest, "connected", Duration::from_secs(10));
    assert!(guest.status()["error"].is_null());
    host.leave();
    guest.leave();
}

#[test]
fn a_document_survives_everyone_leaving_and_the_disk_always_wins() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let invite = host.start();
    host.insert_at_end("typed in the session\n");
    let last = host.kernel_text();
    host.leave();

    let snaps = host.config_dir.path().join("live-snapshots");
    assert_eq!(
        std::fs::read_dir(&snaps).expect("snapshot dir").count(),
        1,
        "one snapshot for the room"
    );

    // An empty room reseeds from the sidecar when the file still matches it.
    host.join(&invite, None).expect("rejoin");
    wait_state(&host, "connected", Duration::from_secs(10));
    assert_eq!(host.kernel_text(), last);
    let mut late = Client::new("late", None, "");
    late.join(&invite, None).expect("an empty copy joins");
    converge(&mut [&mut host, &mut late], Duration::from_secs(30));
    assert_eq!(late.kernel_text(), last);
    host.leave();
    late.leave();

    // A file edited since is never overwritten by the sidecar: the join asks.
    host.editor
        .insert(Some(host.doc), 0, "edited on disk\n")
        .expect("edit");
    let err = host.join(&invite, None).expect_err("the disk differs");
    assert_eq!(err_code(&err), "differs");
}

#[test]
fn contributors_come_back_as_commit_trailers() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, SEED);
    guest
        .call(
            "live.configure",
            json!({ "values": { "live.email": "guest@example.org" } }),
        )
        .expect("configure");
    let invite = host.start();
    guest.join(&invite, None).expect("join");
    wait_state(&guest, "connected", Duration::from_secs(10));
    wait_state(&host, "connected", Duration::from_secs(10));
    guest.insert_at_end("guest wrote this\n");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));

    let out = host
        .call(
            "live.contributors",
            json!({ "committer_name": "host", "committer_email": "host@example.org" }),
        )
        .expect("contributors");
    assert_eq!(
        out["trailers"],
        json!(["Co-authored-by: guest <guest@example.org>"])
    );
    // Taken: the next commit starts from nothing.
    let again = host
        .call("live.contributors", json!({}))
        .expect("contributors");
    assert_eq!(again["trailers"], json!([]));
    host.leave();
    guest.leave();
}

#[test]
fn a_resync_rebuilds_the_document_from_the_room_and_the_session_recovers() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, SEED);
    let invite = host.start();
    guest.join(&invite, None).expect("join");
    wait_state(&guest, "connected", Duration::from_secs(10));
    host.insert_at_end("one\n");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));

    guest.call("live.resync", json!({})).expect("resync");
    assert_eq!(guest.status()["state"], json!("waiting"));
    host.insert_at_end("two\n");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(30));
    wait_state(&guest, "connected", Duration::from_secs(10));
    assert!(guest.kernel_text().ends_with("one\ntwo\n"));
    assert!(guest.status()["error"].is_null());
    // Editing works again, in both directions.
    guest.insert_at_end("three\n");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(15));
    assert!(host.kernel_text().ends_with("two\nthree\n"));
    host.leave();
    guest.leave();
}

#[test]
fn nobody_answering_is_a_clear_error_that_clears_when_someone_does() {
    let relay = Relay::start();
    let mut host = Client::new("host", Some(&relay.url()), SEED);
    let mut guest = Client::new("guest", None, "Draft.\n");
    let invite = host.start();
    host.leave();
    guest.join(&invite, Some("take-host")).expect("join");

    // The host-answer timeout is 20 s.
    wait_state(&guest, "error", Duration::from_secs(40));
    let message = guest.status()["error"].as_str().unwrap_or("").to_owned();
    assert!(message.contains("Nobody has answered"), "{message}");
    assert_eq!(
        guest.kernel_text(),
        "Draft.\n",
        "still the guest's own file"
    );

    host.join(&invite, None).expect("the host comes back");
    converge(&mut [&mut host, &mut guest], Duration::from_secs(30));
    wait_state(&guest, "connected", Duration::from_secs(10));
    assert!(guest.status()["error"].is_null());
    assert_eq!(guest.kernel_text(), host.kernel_text());
    host.leave();
    guest.leave();
}
