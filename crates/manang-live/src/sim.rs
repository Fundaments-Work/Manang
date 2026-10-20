//! A deterministic in-memory network for tests and benchmarks.
//!
//! Clients are full [`Session`]s over [`ropey::Rope`]s. The network delivers
//! frames after random delays (which reorders them), duplicates some, drops
//! some, and can cut a client off entirely (a partition). Time is virtual, so
//! runs are reproducible from a seed. Not for production use: the entropy
//! source is a seeded PRNG.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use ropey::Rope;

use crate::blob::{BlobHash, MAX_BLOB_BYTES, blob_hash};
use crate::bridge::{LiveDoc, TextTarget};
use crate::engine::TextCrdt;
use crate::envelope::{Entropy, RoomKeys, RoomSecret, seed_fingerprint};
use crate::error::LiveError;
use crate::manifest::ManifestEntry;
use crate::project::{FileId, NodeKind, NodeMeta, ProjectDoc, ProjectView, TreeChange};
use crate::session::{Input, Output, Session, SessionConfig};

/// SplitMix64: tiny, fast, good enough for test scheduling.
#[derive(Debug, Clone)]
pub struct Rng(pub u64);

impl Rng {
    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`; returns 0 for `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }
}

impl Entropy for Rng {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), LiveError> {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(())
    }
}

/// Network behaviour.
#[derive(Debug, Clone, Copy)]
pub struct NetConfig {
    /// Percent of frames silently lost.
    pub drop_pct: u64,
    /// Percent of frames delivered twice.
    pub dup_pct: u64,
    /// Maximum one-way delay in ms; delays are uniform in `0..=max_delay_ms`.
    pub max_delay_ms: u64,
}

impl NetConfig {
    /// A perfect network.
    pub const CLEAN: Self = Self {
        drop_pct: 0,
        dup_pct: 0,
        max_delay_ms: 0,
    };
    /// Reordering, duplication and loss.
    pub const HOSTILE: Self = Self {
        drop_pct: 10,
        dup_pct: 15,
        max_delay_ms: 400,
    };
}

/// One simulated participant.
pub struct SimClient<E: TextCrdt> {
    /// The session under test.
    pub session: Session<E, Rng>,
    /// The editor's rope.
    pub rope: Rope,
    /// Whether the link is up.
    pub online: bool,
}

enum Delivery {
    Frame(Vec<u8>),
}

/// The whole simulated room.
pub struct Sim<E: TextCrdt> {
    /// Participants.
    pub clients: Vec<SimClient<E>>,
    /// Every frame any client put on the wire (what a relay would see).
    pub wire: Vec<Vec<u8>>,
    inflight: Vec<(u64, u64, usize, Delivery)>,
    seq: u64,
    now: u64,
    /// Network behaviour; may be changed between steps.
    pub net: NetConfig,
    /// Scheduling randomness.
    pub rng: Rng,
}

impl<E: TextCrdt> Sim<E> {
    /// A room of `n` peers. Peer 0 hosts, seeding `seed_text`; the others
    /// join empty and receive it through the sync exchange.
    pub fn new(n: usize, seed_text: &str, seed: u64, net: NetConfig) -> Result<Self, LiveError> {
        let mut rng = Rng(seed);
        let secret = RoomSecret::generate(&mut rng)?;
        let mut clients = Vec::new();
        for i in 0..n {
            let keys: RoomKeys = secret.derive()?;
            let peer = 100 + i as u64;
            let (doc, rope) = if i == 0 {
                (
                    LiveDoc::<E>::seeded(peer, seed_text)?,
                    Rope::from_str(seed_text),
                )
            } else {
                (LiveDoc::<E>::empty(peer)?, Rope::new())
            };
            let session = Session::new(
                doc,
                keys,
                Rng(seed ^ ((i as u64 + 1) * 0x1234_5678_9abc)),
                SessionConfig {
                    name: format!("peer{i}"),
                    slot: (i % 8) as u8,
                    ..SessionConfig::default()
                },
            );
            clients.push(SimClient {
                session,
                rope,
                online: false,
            });
        }
        let mut sim = Self {
            clients,
            wire: Vec::new(),
            inflight: Vec::new(),
            seq: 0,
            now: 0,
            net,
            rng,
        };
        for i in 0..n {
            let out = sim.clients[i].session.start();
            sim.route(i, out);
            sim.connect(i);
        }
        Ok(sim)
    }

    /// Virtual time in ms.
    #[must_use]
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Bring client `i` online (a fresh connection).
    pub fn connect(&mut self, i: usize) {
        self.clients[i].online = true;
        let out = {
            let c = &mut self.clients[i];
            c.session.handle(self.now, Input::Up, &mut c.rope)
        };
        self.route(i, out);
    }

    /// Cut client `i` off: its pending inbound frames are lost.
    pub fn disconnect(&mut self, i: usize) {
        self.clients[i].online = false;
        self.inflight.retain(|(_, _, to, _)| *to != i);
        let out = {
            let c = &mut self.clients[i];
            c.session.handle(self.now, Input::Down, &mut c.rope)
        };
        self.route(i, out);
    }

    /// Client `i` types: replace characters `from..to` with `text`.
    pub fn edit(&mut self, i: usize, from: usize, to: usize, text: &str) -> Result<(), LiveError> {
        {
            let c = &mut self.clients[i];
            c.session
                .doc_mut()
                .local_replace_chars(&mut c.rope, from, to, text)?;
        }
        let out = self.clients[i].session.flush_local();
        self.route(i, out);
        Ok(())
    }

    fn route(&mut self, from: usize, outputs: Vec<Output>) {
        for output in outputs {
            let Output::Send(frame) = output else {
                continue;
            };
            if !self.clients[from].online {
                continue; // sent while down: lost, resync recovers it
            }
            self.wire.push(frame.clone());
            for to in 0..self.clients.len() {
                if to == from || !self.clients[to].online {
                    continue;
                }
                if self.rng.below(100) < self.net.drop_pct {
                    continue;
                }
                let copies = if self.rng.below(100) < self.net.dup_pct {
                    2
                } else {
                    1
                };
                for _ in 0..copies {
                    let delay = self.rng.below(self.net.max_delay_ms + 1);
                    self.seq += 1;
                    self.inflight.push((
                        self.now + delay,
                        self.seq,
                        to,
                        Delivery::Frame(frame.clone()),
                    ));
                }
            }
        }
    }

    /// Advance virtual time by `ms`, delivering due frames and ticking
    /// sessions every 100 ms.
    pub fn advance(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 100).min(end);
            loop {
                let due: Vec<usize> = {
                    let mut idx: Vec<usize> = (0..self.inflight.len())
                        .filter(|&k| self.inflight[k].0 <= self.now)
                        .collect();
                    idx.sort_by_key(|&k| (self.inflight[k].0, self.inflight[k].1));
                    idx
                };
                if due.is_empty() {
                    break;
                }
                // Take the earliest only, then re-scan: delivery can enqueue more.
                let k = due[0];
                let (_, _, to, Delivery::Frame(frame)) = self.inflight.remove(k);
                if !self.clients[to].online {
                    continue;
                }
                let out = {
                    let c = &mut self.clients[to];
                    c.session
                        .handle(self.now, Input::Frame(&frame), &mut c.rope)
                };
                self.route(to, out);
            }
            for i in 0..self.clients.len() {
                // Offline clients tick too: their reconnect timers run.
                let out = {
                    let c = &mut self.clients[i];
                    c.session.handle(self.now, Input::Tick, &mut c.rope)
                };
                self.route(i, out);
            }
        }
    }

    /// Heal the network, reconnect everyone and run until all ropes are
    /// equal (or `budget_ms` of virtual time passes). Returns whether they are.
    pub fn heal(&mut self, budget_ms: u64) -> bool {
        self.net = NetConfig::CLEAN;
        for i in 0..self.clients.len() {
            if !self.clients[i].online {
                self.connect(i);
            }
        }
        let end = self.now + budget_ms;
        while self.now < end {
            self.advance(1_000);
            if self.converged() {
                return true;
            }
        }
        self.converged()
    }

    /// Whether every rope and every CRDT text are byte-equal.
    #[must_use]
    pub fn converged(&self) -> bool {
        let first = self.clients[0].rope.to_text();
        self.clients
            .iter()
            .all(|c| c.rope.to_text() == first && c.session.doc().engine().text() == first)
    }
}

// ---------------------------------------------------------------------------
// Project sessions
// ---------------------------------------------------------------------------

/// Content of a file in a simulated project.
#[derive(Debug, Clone)]
pub enum SimContent {
    /// A text file (a live document).
    Text(String),
    /// Any other file (a blob).
    Blob(Vec<u8>),
}

/// Counters a [`ProjectClient`] keeps of what its caller-side policy did.
#[derive(Debug, Clone, Default)]
pub struct ProjectCounters {
    /// Tree outputs seen.
    pub tree_events: usize,
    /// Files re-fetched after a divergence.
    pub refetches: usize,
    /// Files whose local copy differed from the host's and was replaced.
    pub replaced: usize,
    /// Files whose local copy matched the host's seed.
    pub identical: usize,
    /// Last progress report.
    pub progress: Option<(usize, usize)>,
    /// Detached documents that changed (disk writes a real client would do).
    pub doc_changes: usize,
    /// Blobs received.
    pub blobs_received: usize,
}

/// One participant of a [`ProjectSim`]: a project [`Session`] plus the
/// caller-side policy a real client would implement (load files lazily,
/// classify local files against the manifest, serve blobs).
pub struct ProjectClient<E: TextCrdt> {
    /// The session under test.
    pub session: Session<E, Rng>,
    /// Editor buffers of the files this client has open.
    pub ropes: BTreeMap<FileId, Rope>,
    /// Whether the link is up.
    pub online: bool,
    /// The files on this peer's disk, by project path.
    pub local: BTreeMap<String, String>,
    /// Blobs this peer holds, by hash.
    pub blobs: BTreeMap<BlobHash, Vec<u8>>,
    /// The manifest received from the host, if any.
    pub manifest: Option<Vec<ManifestEntry>>,
    /// What the policy did.
    pub counters: ProjectCounters,
    peer: u64,
}

/// A simulated project room: peer 0 hosts, the others join through the
/// manifest. Same network model as [`Sim`].
pub struct ProjectSim<E: TextCrdt> {
    /// Participants.
    pub clients: Vec<ProjectClient<E>>,
    /// `(time, client, bytes)` of every frame a client put on the wire.
    pub sent: Vec<(u64, usize, usize)>,
    inflight: Vec<(u64, u64, usize, Vec<u8>)>,
    seq: u64,
    now: u64,
    /// Network behaviour; may be changed between steps.
    pub net: NetConfig,
    /// Scheduling randomness.
    pub rng: Rng,
}

fn split_parent(path: &str) -> (Option<&str>, &str) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, path),
    }
}

impl<E: TextCrdt> ProjectSim<E> {
    /// A room of `n` peers. Peer 0 hosts `files` (paths may contain `/`;
    /// directories are created as needed); the others join empty. `config`
    /// adjusts every peer's [`SessionConfig`].
    pub fn new(
        n: usize,
        files: &[(&str, SimContent)],
        seed: u64,
        net: NetConfig,
        config: impl Fn(&mut SessionConfig),
    ) -> Result<Self, LiveError> {
        let mut rng = Rng(seed);
        let secret = RoomSecret::generate(&mut rng)?;
        let mut clients = Vec::new();
        for i in 0..n {
            let keys: RoomKeys = secret.derive_project()?;
            let peer = 100 + i as u64;
            let mut cfg = SessionConfig {
                name: format!("peer{i}"),
                slot: (i % 8) as u8,
                await_host: i != 0,
                ..SessionConfig::default()
            };
            config(&mut cfg);
            let session = Session::new_project(
                ProjectDoc::new(peer)?,
                keys,
                Rng(seed ^ ((i as u64 + 1) * 0x1234_5678_9abc)),
                cfg,
            )?;
            clients.push(ProjectClient {
                session,
                ropes: BTreeMap::new(),
                online: false,
                local: BTreeMap::new(),
                blobs: BTreeMap::new(),
                manifest: None,
                counters: ProjectCounters::default(),
                peer,
            });
        }
        let mut sim = Self {
            clients,
            sent: Vec::new(),
            inflight: Vec::new(),
            seq: 0,
            now: 0,
            net,
            rng,
        };
        for (path, content) in files {
            sim.host_add(path, content)?;
        }
        if n > 0 {
            sim.clients[0].session.flush_project(); // drain the setup operations
        }
        for i in 0..n {
            let out = sim.clients[i].session.start();
            sim.absorb(i, out);
            sim.connect(i);
        }
        Ok(sim)
    }

    fn host_add(&mut self, path: &str, content: &SimContent) -> Result<(), LiveError> {
        let c = &mut self.clients[0];
        let mut parent: Option<FileId> = None;
        let mut prefix = String::new();
        let parts: Vec<&str> = path.split('/').collect();
        for dir in &parts[..parts.len() - 1] {
            prefix.push_str(dir);
            let view = c.session.project_view();
            let existing = view.as_ref().and_then(|v| v.id_at_path(&prefix));
            parent = match existing {
                Some(id) => Some(id),
                None => {
                    let p = c
                        .session
                        .project_mut()
                        .ok_or_else(|| LiveError::Project("not a project session".into()))?;
                    Some(p.create(parent, &NodeMeta::dir(*dir))?)
                }
            };
            prefix.push('/');
        }
        let name = parts[parts.len() - 1];
        let meta = match content {
            SimContent::Text(t) => {
                c.local.insert(path.to_owned(), t.clone());
                NodeMeta::file(
                    name,
                    NodeKind::Text,
                    t.len() as u64,
                    blob_hash(t.as_bytes()),
                    seed_fingerprint(t),
                )
            }
            SimContent::Blob(b) => {
                let h = blob_hash(b);
                c.blobs.insert(h, b.clone());
                NodeMeta::file(name, NodeKind::Blob, b.len() as u64, h, [0; 16])
            }
        };
        c.session
            .project_mut()
            .ok_or_else(|| LiveError::Project("not a project session".into()))?
            .create(parent, &meta)?;
        Ok(())
    }

    /// Virtual time in ms.
    #[must_use]
    pub fn now(&self) -> u64 {
        self.now
    }

    fn route(&mut self, from: usize, frame: Vec<u8>) {
        if !self.clients[from].online {
            return; // sent while down: lost, resync recovers it
        }
        self.sent.push((self.now, from, frame.len()));
        for to in 0..self.clients.len() {
            if to == from || !self.clients[to].online {
                continue;
            }
            if self.rng.below(100) < self.net.drop_pct {
                continue;
            }
            let copies = if self.rng.below(100) < self.net.dup_pct {
                2
            } else {
                1
            };
            for _ in 0..copies {
                let delay = self.rng.below(self.net.max_delay_ms + 1);
                self.seq += 1;
                self.inflight
                    .push((self.now + delay, self.seq, to, frame.clone()));
            }
        }
    }

    /// Route sends and run the caller-side policy on everything else.
    fn absorb(&mut self, i: usize, outputs: Vec<Output>) {
        let mut pending: VecDeque<Output> = outputs.into();
        while let Some(output) = pending.pop_front() {
            match output {
                Output::Send(frame) => self.route(i, frame),
                other => {
                    let more = self.policy(i, other);
                    pending.extend(more);
                }
            }
        }
    }

    /// What a real client does with a non-send output.
    fn policy(&mut self, i: usize, output: Output) -> Vec<Output> {
        let now = self.now;
        let c = &mut self.clients[i];
        let peer = c.peer;
        match output {
            Output::DocWanted(id) => {
                let path = c
                    .session
                    .project_view()
                    .and_then(|v| v.path_of(id).map(str::to_owned));
                let text = path.and_then(|p| c.local.get(&p).cloned());
                match text.and_then(|t| LiveDoc::<E>::seeded(peer, &t).ok()) {
                    Some(doc) => c.session.provide_doc(now, id, doc, false),
                    None => c.session.want_doc(now, id, false),
                }
            }
            Output::Manifest { entries, .. } => {
                let mut out = Vec::new();
                for e in &entries {
                    match e.kind {
                        NodeKind::Dir => {}
                        NodeKind::Text => {
                            if c.session.is_loaded(e.id) {
                                continue;
                            }
                            let local = c.local.get(&e.path).cloned();
                            let same = local
                                .as_deref()
                                .is_some_and(|t| seed_fingerprint(t) == e.seed_fp);
                            match (local, same) {
                                (Some(t), true) => {
                                    c.counters.identical += 1;
                                    if let Ok(doc) = LiveDoc::<E>::seeded(peer, &t) {
                                        out.extend(c.session.provide_doc(now, e.id, doc, false));
                                    }
                                }
                                (Some(_), false) => {
                                    c.counters.replaced += 1;
                                    out.extend(c.session.want_doc(now, e.id, false));
                                }
                                (None, _) => out.extend(c.session.want_doc(now, e.id, false)),
                            }
                        }
                        NodeKind::Blob => {
                            if !c.blobs.contains_key(&e.sha256)
                                && let Ok(o) = c.session.want_blob(now, e.sha256, e.size)
                            {
                                out.extend(o);
                            }
                        }
                    }
                }
                c.manifest = Some(entries);
                out
            }
            Output::Tree(changes) => {
                c.counters.tree_events += 1;
                let Some(view) = c.session.project_view() else {
                    return Vec::new();
                };
                let mut out = Vec::new();
                for change in changes {
                    let (TreeChange::Created { id, .. } | TreeChange::Content { id, .. }) = change
                    else {
                        continue;
                    };
                    let Some(entry) = view.get(id) else { continue };
                    match entry.meta.kind {
                        NodeKind::Text if !c.session.is_loaded(id) => {
                            out.extend(c.session.want_doc(now, id, false));
                        }
                        NodeKind::Blob
                            if !c.blobs.contains_key(&entry.meta.sha256)
                                && entry.meta.size <= MAX_BLOB_BYTES =>
                        {
                            if let Ok(o) =
                                c.session.want_blob(now, entry.meta.sha256, entry.meta.size)
                            {
                                out.extend(o);
                            }
                        }
                        _ => {}
                    }
                }
                out
            }
            Output::BlobRequested { hash, .. } => match c.blobs.get(&hash) {
                Some(data) => c.session.offer_blob(now, hash, data.clone()),
                None => {
                    c.session.decline_blob(hash);
                    Vec::new()
                }
            },
            Output::BlobReceived { hash, data } => {
                c.counters.blobs_received += 1;
                c.blobs.insert(hash, data);
                Vec::new()
            }
            Output::DocDiverged { refetching, .. } => {
                if refetching {
                    c.counters.refetches += 1;
                }
                Vec::new()
            }
            Output::DocChanged(_) => {
                c.counters.doc_changes += 1;
                Vec::new()
            }
            Output::Progress { done, total } => {
                c.counters.progress = Some((done, total));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Run a call on client `i`'s session (given the current virtual time)
    /// and route its outputs like a real driver would.
    pub fn call(&mut self, i: usize, f: impl FnOnce(&mut Session<E, Rng>, u64) -> Vec<Output>) {
        let now = self.now;
        let out = f(&mut self.clients[i].session, now);
        self.absorb(i, out);
    }

    /// Bring client `i` online (a fresh connection).
    pub fn connect(&mut self, i: usize) {
        self.clients[i].online = true;
        let now = self.now;
        let out = {
            let c = &mut self.clients[i];
            c.session.handle_with(now, Input::Up, &mut c.ropes)
        };
        self.absorb(i, out);
    }

    /// Cut client `i` off: its pending inbound frames are lost.
    pub fn disconnect(&mut self, i: usize) {
        self.clients[i].online = false;
        self.inflight.retain(|(_, _, to, _)| *to != i);
        let now = self.now;
        let out = {
            let c = &mut self.clients[i];
            c.session.handle_with(now, Input::Down, &mut c.ropes)
        };
        self.absorb(i, out);
    }

    /// Client `i` opens file `id` in an editor buffer. Returns whether the
    /// document was loaded.
    pub fn open(&mut self, i: usize, id: FileId) -> bool {
        let c = &mut self.clients[i];
        let Some(doc) = c.session.file_doc(id) else {
            return false;
        };
        c.ropes.insert(id, Rope::from_str(&doc.engine().text()));
        let open: Vec<FileId> = c.ropes.keys().copied().collect();
        let out = c.session.set_focus(Some(id), open);
        self.absorb(i, out);
        true
    }

    /// Client `i` types in file `id`: replace characters `from..to` with
    /// `text`. Works whether or not the file is open in a buffer.
    pub fn edit(
        &mut self,
        i: usize,
        id: FileId,
        from: usize,
        to: usize,
        text: &str,
    ) -> Result<(), LiveError> {
        {
            let c = &mut self.clients[i];
            let Some(doc) = c.session.file_doc_mut(id) else {
                return Err(LiveError::Project("document not loaded".into()));
            };
            match c.ropes.get_mut(&id) {
                Some(rope) => doc.local_replace_chars(rope, from, to, text)?,
                None => {
                    let full = doc.engine().text();
                    let byte =
                        |ch: usize| full.char_indices().nth(ch).map_or(full.len(), |(b, _)| b);
                    let (bf, bt) = (byte(from), byte(to));
                    doc.record_local_bytes(bf, bt.saturating_sub(bf), text)?;
                }
            }
        }
        let out = self.clients[i].session.flush_doc(id);
        self.absorb(i, out);
        Ok(())
    }

    /// Client `i` creates a text file at `path` (its directory must exist or
    /// be empty) with `text`. Returns the new id.
    pub fn create_text(&mut self, i: usize, path: &str, text: &str) -> Result<FileId, LiveError> {
        let now = self.now;
        let (dir, name) = split_parent(path);
        let (id, doc) = {
            let c = &mut self.clients[i];
            let parent = match dir {
                None => None,
                Some(d) => Some(
                    c.session
                        .project_view()
                        .and_then(|v| v.id_at_path(d))
                        .ok_or_else(|| LiveError::Project("no such directory".into()))?,
                ),
            };
            let meta = NodeMeta::file(
                name,
                NodeKind::Text,
                text.len() as u64,
                blob_hash(text.as_bytes()),
                seed_fingerprint(text),
            );
            let id = c
                .session
                .project_mut()
                .ok_or_else(|| LiveError::Project("not a project session".into()))?
                .create(parent, &meta)?;
            c.local.insert(path.to_owned(), text.to_owned());
            (id, LiveDoc::<E>::seeded(c.peer, text)?)
        };
        let mut out = self.clients[i].session.provide_doc(now, id, doc, false);
        out.extend(self.clients[i].session.flush_project());
        self.absorb(i, out);
        Ok(id)
    }

    /// Client `i` creates a blob file at the top level.
    pub fn create_blob(
        &mut self,
        i: usize,
        name: &str,
        data: Vec<u8>,
    ) -> Result<FileId, LiveError> {
        let c = &mut self.clients[i];
        let h = blob_hash(&data);
        let meta = NodeMeta::file(name, NodeKind::Blob, data.len() as u64, h, [0; 16]);
        c.blobs.insert(h, data);
        let id = c
            .session
            .project_mut()
            .ok_or_else(|| LiveError::Project("not a project session".into()))?
            .create(None, &meta)?;
        let out = c.session.flush_project();
        self.absorb(i, out);
        Ok(id)
    }

    /// Client `i` renames a node.
    pub fn rename(&mut self, i: usize, id: FileId, name: &str) -> Result<(), LiveError> {
        let c = &mut self.clients[i];
        c.session
            .project_mut()
            .ok_or_else(|| LiveError::Project("not a project session".into()))?
            .rename(id, name)?;
        let out = c.session.flush_project();
        self.absorb(i, out);
        Ok(())
    }

    /// Client `i` deletes a node (and everything under it).
    pub fn delete(&mut self, i: usize, id: FileId) -> Result<(), LiveError> {
        let c = &mut self.clients[i];
        c.session
            .project_mut()
            .ok_or_else(|| LiveError::Project("not a project session".into()))?
            .delete(id)?;
        let out = c.session.flush_project();
        self.absorb(i, out);
        Ok(())
    }

    /// Advance virtual time by `ms`, delivering due frames and ticking
    /// sessions every 100 ms.
    pub fn advance(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 100).min(end);
            loop {
                let earliest = (0..self.inflight.len())
                    .filter(|&k| self.inflight[k].0 <= self.now)
                    .min_by_key(|&k| (self.inflight[k].0, self.inflight[k].1));
                let Some(k) = earliest else { break };
                let (_, _, to, frame) = self.inflight.remove(k);
                if !self.clients[to].online {
                    continue;
                }
                let now = self.now;
                let out = {
                    let c = &mut self.clients[to];
                    c.session
                        .handle_with(now, Input::Frame(&frame), &mut c.ropes)
                };
                self.absorb(to, out);
            }
            for i in 0..self.clients.len() {
                let now = self.now;
                let out = {
                    let c = &mut self.clients[i];
                    c.session.handle_with(now, Input::Tick, &mut c.ropes)
                };
                self.absorb(i, out);
            }
        }
    }

    /// Heal the network, reconnect everyone and run until converged (or
    /// `budget_ms` of virtual time passes). Returns whether they are.
    pub fn heal(&mut self, budget_ms: u64) -> bool {
        self.net = NetConfig::CLEAN;
        for i in 0..self.clients.len() {
            if !self.clients[i].online {
                self.connect(i);
            }
        }
        let end = self.now + budget_ms;
        while self.now < end {
            self.advance(1_000);
            if self.converged() {
                return true;
            }
        }
        self.converged()
    }

    /// Whether every peer has the same project tree, every live text file
    /// loaded with byte-equal text (CRDT and open buffer), and every live
    /// blob.
    #[must_use]
    pub fn converged(&mut self) -> bool {
        let views: Vec<Option<Arc<ProjectView>>> = self
            .clients
            .iter_mut()
            .map(|c| c.session.project_view())
            .collect();
        let Some(Some(first)) = views.first() else {
            return true;
        };
        if views
            .iter()
            .any(|v| v.as_ref().is_none_or(|v| v.entries() != first.entries()))
        {
            return false;
        }
        for entry in first.entries() {
            match entry.meta.kind {
                NodeKind::Dir => {}
                NodeKind::Text => {
                    let texts: Vec<Option<String>> = self
                        .clients
                        .iter()
                        .map(|c| c.session.file_doc(entry.id).map(|d| d.engine().text()))
                        .collect();
                    let Some(Some(want)) = texts.first() else {
                        return false;
                    };
                    if texts.iter().any(|t| t.as_ref() != Some(want)) {
                        return false;
                    }
                    if self
                        .clients
                        .iter()
                        .any(|c| c.ropes.get(&entry.id).is_some_and(|r| r.to_text() != *want))
                    {
                        return false;
                    }
                }
                NodeKind::Blob => {
                    if self
                        .clients
                        .iter()
                        .any(|c| !c.blobs.contains_key(&entry.meta.sha256))
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Most frames and bytes any one client sent in a one-second window.
    #[must_use]
    pub fn peak_send_rate(&self) -> (usize, usize) {
        let mut peak = (0usize, 0usize);
        for client in 0..self.clients.len() {
            let mine: Vec<&(u64, usize, usize)> =
                self.sent.iter().filter(|(_, c, _)| *c == client).collect();
            for (k, (t0, _, _)) in mine.iter().enumerate() {
                let window = mine[k..].iter().take_while(|(t, _, _)| *t < t0 + 1_000);
                let (n, bytes) =
                    window.fold((0usize, 0usize), |(n, b), (_, _, len)| (n + 1, b + len));
                peak = (peak.0.max(n), peak.1.max(bytes));
            }
        }
        peak
    }
}
