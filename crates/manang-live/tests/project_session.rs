//! Project sessions: the multi-document router, join through the manifest,
//! lazy documents, paced bulk sync, blob transfer, hostile input, and
//! convergence of N peers over M documents on a lossy network.

use manang_live::blob::{CHUNK_SIZE, MAX_BLOB_BYTES, blob_hash};
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{Kind, RoomKeys, RoomSecret, seed_fingerprint};
use manang_live::manifest::ManifestEntry;
use manang_live::pacer::PaceConfig;
use manang_live::project::{FileId, NodeKind, NodeMeta, ProjectDoc};
use manang_live::protocol::Message;
use manang_live::session::{Input, NoTargets, Output, Session, SessionConfig, SessionState};
use manang_live::sim::{NetConfig, ProjectSim, Rng, SimContent};
use manang_live::{LoroEngine, TextCrdt, YrsEngine};
use proptest::prelude::*;

fn no_pacing(cfg: &mut SessionConfig) {
    cfg.pace = PaceConfig {
        bytes_per_sec: 0,
        ..PaceConfig::default()
    };
}

fn keep(_: &mut SessionConfig) {}

fn small_project() -> Vec<(&'static str, SimContent)> {
    vec![
        ("main.typ", SimContent::Text("= Title\nhello é😀\n".into())),
        ("ch/a.typ", SimContent::Text("chapter a\n".into())),
        ("ch/b.typ", SimContent::Text("chapter b\n".into())),
        (
            "fig.bin",
            SimContent::Blob(
                (0..(2 * CHUNK_SIZE + 123))
                    .map(|i| (i % 251) as u8)
                    .collect(),
            ),
        ),
    ]
}

fn live_ids(sim: &mut ProjectSim<LoroEngine>, client: usize, kind: NodeKind) -> Vec<FileId> {
    sim.clients[client]
        .session
        .project_view()
        .map(|v| {
            v.entries()
                .iter()
                .filter(|e| e.meta.kind == kind)
                .map(|e| e.id)
                .collect()
        })
        .unwrap_or_default()
}

// -- join, lazy loading, progress ---------------------------------------------

fn join_by_manifest<E: TextCrdt>() {
    let mut sim =
        ProjectSim::<E>::new(3, &small_project(), 1, NetConfig::CLEAN, keep).expect("sim");
    // Nothing is loaded before anybody asks.
    assert_eq!(sim.clients[0].session.loaded_docs().count(), 0);
    assert!(sim.clients[1].manifest.is_none());
    assert!(sim.heal(60_000), "did not converge");
    sim.advance(5_000);
    for c in &sim.clients {
        assert_eq!(c.session.state(), SessionState::Live);
        assert_eq!(c.session.loaded_docs().count(), 3);
        assert_eq!(c.blobs.len(), 1);
    }
    for guest in &sim.clients[1..] {
        let manifest = guest.manifest.as_ref().expect("manifest");
        assert_eq!(manifest.len(), 5, "3 texts + 1 blob + 1 dir");
        assert_eq!(guest.counters.blobs_received, 1);
        // Files without an editor buffer are applied to the CRDT only and
        // reported for the disk bridge.
        assert!(guest.counters.doc_changes >= 3);
        // Progress reported and ended at done == total.
        let (done, total) = guest.counters.progress.expect("progress");
        assert_eq!(done, total);
        assert!(total >= 4);
    }
}

#[test]
fn guests_join_through_the_manifest() {
    join_by_manifest::<LoroEngine>();
    join_by_manifest::<YrsEngine>();
}

#[test]
fn documents_load_lazily_and_the_host_is_asked_before_answering() {
    let secret = RoomSecret::generate(&mut Rng(3)).expect("secret");
    let mut host_project = ProjectDoc::new(100).expect("project");
    let id = host_project
        .create(
            None,
            &NodeMeta::file(
                "a.typ",
                NodeKind::Text,
                5,
                blob_hash(b"hello"),
                seed_fingerprint("hello"),
            ),
        )
        .expect("create");
    let mut host = Session::<LoroEngine, _>::new_project(
        host_project,
        secret.derive_project().expect("keys"),
        Rng(1),
        SessionConfig::default(),
    )
    .expect("host");
    let mut guest = Session::<LoroEngine, _>::new_project(
        ProjectDoc::new(200).expect("project"),
        secret.derive_project().expect("keys"),
        Rng(2),
        SessionConfig {
            await_host: true,
            ..SessionConfig::default()
        },
    )
    .expect("guest");
    assert_eq!(host.loaded_docs().count(), 0);
    let _ = host.flush_project();

    let mut to_guest: Vec<Vec<u8>> = frames(&host.handle_with(0, Input::Up, &mut NoTargets));
    let mut to_host: Vec<Vec<u8>> = frames(&guest.handle_with(0, Input::Up, &mut NoTargets));
    let mut manifest: Option<Vec<ManifestEntry>> = None;
    let mut wanted = Vec::new();
    for round in 1..30u64 {
        let now = round * 100;
        for f in std::mem::take(&mut to_host) {
            let out = host.handle_with(now, Input::Frame(&f), &mut NoTargets);
            wanted.extend(out.iter().filter_map(|o| match o {
                Output::DocWanted(d) => Some(*d),
                _ => None,
            }));
            to_guest.extend(frames(&out));
        }
        for f in std::mem::take(&mut to_guest) {
            let out = guest.handle_with(now, Input::Frame(&f), &mut NoTargets);
            for o in &out {
                if let Output::Manifest { entries, .. } = o {
                    manifest = Some(entries.clone());
                }
            }
            to_host.extend(frames(&out));
        }
        if manifest.is_some() && guest.loaded_docs().count() == 0 {
            // The caller classified and now wants the file.
            let out = guest.want_doc(now, id, true);
            to_host.extend(frames(&out));
        }
        // The host loads the document only when asked to.
        if let Some(d) = wanted.pop() {
            assert_eq!(d, id);
            assert!(!host.is_loaded(id));
            let doc = LiveDoc::<LoroEngine>::seeded(100, "hello").expect("doc");
            let out = host.provide_doc(now, id, doc, true);
            to_guest.extend(frames(&out));
        }
    }
    assert_eq!(manifest.map(|m| m.len()), Some(1));
    assert_eq!(wanted.len(), 0);
    assert!(host.is_loaded(id));
    assert_eq!(
        guest.file_doc(id).map(|d| d.engine().text()).as_deref(),
        Some("hello")
    );
    assert!(guest.is_doc_synced(id));
}

fn frames(out: &[Output]) -> Vec<Vec<u8>> {
    out.iter()
        .filter_map(|o| match o {
            Output::Send(f) => Some(f.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn manifest_join_classifies_identical_missing_and_differing_files() {
    let files = small_project();
    let mut sim = ProjectSim::<LoroEngine>::new(2, &files, 7, NetConfig::CLEAN, keep).expect("sim");
    // The guest has main.typ identical, ch/a.typ edited, ch/b.typ missing.
    let guest = &mut sim.clients[1];
    guest
        .local
        .insert("main.typ".into(), "= Title\nhello é😀\n".into());
    guest
        .local
        .insert("ch/a.typ".into(), "my own chapter\n".into());
    assert!(sim.heal(60_000));
    let g = &sim.clients[1];
    assert_eq!(g.counters.identical, 1);
    assert_eq!(g.counters.replaced, 1);
    let view = sim.clients[1].session.project_view().expect("view");
    for (path, want) in [
        ("main.typ", "= Title\nhello é😀\n"),
        ("ch/a.typ", "chapter a\n"),
        ("ch/b.typ", "chapter b\n"),
    ] {
        let id = view.id_at_path(path).expect(path);
        let got = sim.clients[1]
            .session
            .file_doc(id)
            .map(|d| d.engine().text());
        assert_eq!(got.as_deref(), Some(want), "{path}");
    }
}

#[test]
fn the_manifest_is_verified_and_lost_pages_are_asked_for_again() {
    // 40% loss: pages vanish, the guest keeps asking and still joins.
    let net = NetConfig {
        drop_pct: 40,
        dup_pct: 0,
        max_delay_ms: 50,
    };
    let many: Vec<(String, SimContent)> = (0..600)
        .map(|i| {
            (
                format!("d{}/f{i}.txt", i % 7),
                SimContent::Text(format!("t{i}")),
            )
        })
        .collect();
    let refs: Vec<(&str, SimContent)> = many.iter().map(|(p, c)| (p.as_str(), c.clone())).collect();
    let mut sim = ProjectSim::<LoroEngine>::new(2, &refs, 11, net, no_pacing).expect("sim");
    sim.advance(60_000);
    let manifest = sim.clients[1].manifest.as_ref().expect("manifest arrived");
    assert_eq!(manifest.len(), 600 + 7);
    assert!(sim.heal(1_800_000), "did not converge after the manifest");
}

// -- pacing -------------------------------------------------------------------

#[test]
fn bulk_first_sync_stays_under_the_relay_limits() {
    let mut files: Vec<(String, SimContent)> = (0..300)
        .map(|i| {
            (
                format!("src/f{i}.typ"),
                SimContent::Text(format!("file {i}\n{}", "lorem ipsum ".repeat(150))),
            )
        })
        .collect();
    files.push((
        "data/big.bin".into(),
        SimContent::Blob(
            (0..(6 * CHUNK_SIZE))
                .map(|i| (i.wrapping_mul(31) % 253) as u8)
                .collect(),
        ),
    ));
    let refs: Vec<(&str, SimContent)> =
        files.iter().map(|(p, c)| (p.as_str(), c.clone())).collect();
    let mut sim = ProjectSim::<LoroEngine>::new(2, &refs, 5, NetConfig::CLEAN, keep).expect("sim");
    assert!(sim.heal(600_000), "bulk sync did not finish");
    let (msgs, bytes) = sim.peak_send_rate();
    assert!(msgs <= 150, "{msgs} messages in one second");
    assert!(bytes <= 4 * 1024 * 1024, "{bytes} bytes in one second");
    assert_eq!(sim.clients[1].counters.blobs_received, 1);
    // The unpaced run needs far less virtual time: pacing really defers.
    assert!(sim.now() > 3_000);
}

#[test]
fn open_files_are_served_ahead_of_background_sync() {
    // The guest has file 299 open; it must be synced long before the rest.
    let files: Vec<(String, SimContent)> = (0..300)
        .map(|i| {
            (
                format!("f{i:03}.typ"),
                SimContent::Text(format!("{i}\n{}", "x".repeat(40_000))),
            )
        })
        .collect();
    let refs: Vec<(&str, SimContent)> =
        files.iter().map(|(p, c)| (p.as_str(), c.clone())).collect();
    let mut sim = ProjectSim::<LoroEngine>::new(2, &refs, 6, NetConfig::CLEAN, keep).expect("sim");
    // Wait for the manifest, then prioritise the last file.
    while sim.clients[1].manifest.is_none() {
        sim.advance(100);
        assert!(sim.now() < 30_000, "no manifest");
    }
    let id = sim.clients[1]
        .session
        .project_view()
        .and_then(|v| v.id_at_path("f299.typ"))
        .expect("id");
    sim.call(1, |s, now| s.want_doc(now, id, true));
    sim.advance(3_000);
    let synced = sim.clients[1].session.is_doc_synced(id);
    let loaded_synced = sim.clients[1]
        .session
        .loaded_docs()
        .filter(|d| sim.clients[1].session.is_doc_synced(*d))
        .count();
    assert!(synced, "the open file waited behind the bulk");
    assert!(
        loaded_synced < 300,
        "pacing must not have finished everything yet"
    );
    assert!(sim.heal(600_000));
}

// -- blobs ---------------------------------------------------------------------

struct Rig {
    keys: RoomKeys,
    rng: Rng,
}

impl Rig {
    fn frame(&mut self, msg: &Message) -> Vec<u8> {
        self.keys
            .seal(Kind::Data, &msg.encode().expect("encode"), &mut self.rng)
            .expect("seal")
    }
}

fn blob_guest() -> (Session<LoroEngine, Rng>, Rig) {
    let secret = RoomSecret::generate(&mut Rng(9)).expect("secret");
    let mut cfg = SessionConfig {
        await_host: false,
        ..SessionConfig::default()
    };
    no_pacing(&mut cfg);
    let mut s = Session::<LoroEngine, _>::new_project(
        ProjectDoc::new(300).expect("project"),
        secret.derive_project().expect("keys"),
        Rng(1),
        cfg,
    )
    .expect("session");
    let _ = s.start();
    let _ = s.handle_with(0, Input::Up, &mut NoTargets);
    (
        s,
        Rig {
            keys: secret.derive_project().expect("keys"),
            rng: Rng(77),
        },
    )
}

#[test]
fn blobs_roundtrip_in_chunks_and_verify() {
    let data: Vec<u8> = (0..(2 * CHUNK_SIZE + 5)).map(|i| (i % 249) as u8).collect();
    let hash = blob_hash(&data);
    let (mut s, mut rig) = blob_guest();
    let out = s.want_blob(10, hash, data.len() as u64).expect("want");
    assert!(!frames(&out).is_empty(), "first chunk requested");
    let mut got = None;
    for (i, part) in manang_live::blob::split(&data)
        .expect("split")
        .iter()
        .enumerate()
    {
        let msg = Message::BlobChunk {
            from: 400,
            to: None,
            hash,
            chunk: i as u32,
            count: 3,
            data: part.to_vec(),
        };
        let out = s.handle_with(
            20 + i as u64,
            Input::Frame(&rig.frame(&msg)),
            &mut NoTargets,
        );
        for o in out {
            if let Output::BlobReceived { hash: h, data } = o {
                assert_eq!(h, hash);
                got = Some(data);
            }
        }
    }
    assert_eq!(got.as_deref(), Some(data.as_slice()));
    assert_eq!(s.stats().blob_rejected, 0);
}

#[test]
fn hostile_blob_traffic_is_counted_and_never_fatal() {
    let data = vec![7u8; 1000];
    let hash = blob_hash(&data);
    let (mut s, mut rig) = blob_guest();
    // Unsolicited: nothing was asked for yet.
    let chunk = |hash, chunk, count, data: Vec<u8>| Message::BlobChunk {
        from: 400,
        to: None,
        hash,
        chunk,
        count,
        data,
    };
    let mut feed = |s: &mut Session<LoroEngine, Rng>, m: &Message, t: u64| {
        s.handle_with(t, Input::Frame(&rig.frame(m)), &mut NoTargets)
    };
    let out = feed(&mut s, &chunk(hash, 0, 1, data.clone()), 1);
    assert!(out.iter().any(|o| matches!(o, Output::Rejected(_))));
    assert_eq!(s.stats().blob_unsolicited, 1);

    // Over the size cap: refused before anything is allocated.
    assert!(s.want_blob(2, hash, MAX_BLOB_BYTES + 1).is_err());

    let _ = s.want_blob(3, hash, 1000).expect("want");
    // Wrong hash, wrong count, wrong length, bad index, oversized chunk.
    let other = blob_hash(b"other");
    for m in [
        chunk(other, 0, 1, data.clone()),
        chunk(hash, 0, 2, data.clone()),
        chunk(hash, 0, 1, vec![1; 999]),
        chunk(hash, 5, 1, data.clone()),
        chunk(hash, 0, 1, vec![0; CHUNK_SIZE + 1]),
    ] {
        let out = feed(&mut s, &m, 4);
        assert!(
            !out.iter().any(|o| matches!(o, Output::BlobReceived { .. })),
            "accepted a hostile chunk"
        );
    }
    // Right shape, wrong content: hash mismatch, counted, blob restarts.
    let out = feed(&mut s, &chunk(hash, 0, 1, vec![8; 1000]), 5);
    assert!(!out.iter().any(|o| matches!(o, Output::BlobReceived { .. })));
    assert!(s.stats().blob_rejected >= 1);
    // Garbage frames do nothing either.
    let out = s.handle_with(6, Input::Frame(&[1, 2, 3]), &mut NoTargets);
    assert!(out.iter().all(|o| matches!(o, Output::Rejected(_))));
    // The honest chunk still completes it.
    let out = feed(&mut s, &chunk(hash, 0, 1, data.clone()), 7);
    assert!(out.iter().any(|o| matches!(o, Output::BlobReceived { .. })));
    assert_eq!(s.state(), SessionState::Syncing);
}

#[test]
fn a_peer_gets_one_blob_in_flight_and_unserved_requests_are_reported() {
    let (mut s, mut rig) = blob_guest();
    let a = vec![1u8; 100];
    let b = vec![2u8; 100];
    let get = |hash| Message::BlobGet {
        from: 500,
        to: None,
        hash,
        chunk: 0,
    };
    // Not offered: the caller is asked once.
    let out = s.handle_with(
        1,
        Input::Frame(&rig.frame(&get(blob_hash(&a)))),
        &mut NoTargets,
    );
    assert_eq!(
        out.iter()
            .filter(|o| matches!(o, Output::BlobRequested { .. }))
            .count(),
        1
    );
    // A second blob from the same peer while the first is in flight: refused.
    let out = s.handle_with(
        2,
        Input::Frame(&rig.frame(&get(blob_hash(&b)))),
        &mut NoTargets,
    );
    assert!(out.iter().any(|o| matches!(o, Output::Rejected(_))));
    assert_eq!(s.stats().blob_busy, 1);
    // Offering the first answers the waiting request.
    let out = s.offer_blob(3, blob_hash(&a), a.clone());
    assert_eq!(frames(&out).len(), 1);
    // An offer that does not match its hash is refused.
    let out = s.offer_blob(4, blob_hash(&b), vec![9; 100]);
    assert!(out.iter().any(|o| matches!(o, Output::Rejected(_))));
    // After idling, the peer may switch blobs.
    let out = s.handle_with(
        20_000,
        Input::Frame(&rig.frame(&get(blob_hash(&b)))),
        &mut NoTargets,
    );
    assert!(
        out.iter()
            .any(|o| matches!(o, Output::BlobRequested { .. }))
    );
}

// -- divergence, presence, eviction -----------------------------------------------

#[test]
fn a_diverged_file_is_refetched_alone_and_the_session_survives() {
    let files = vec![
        ("a.typ", SimContent::Text("alpha".into())),
        ("b.typ", SimContent::Text("bravo b".into())),
    ];
    let mut sim =
        ProjectSim::<LoroEngine>::new(2, &files, 13, NetConfig::CLEAN, keep).expect("sim");
    // The guest loads a.typ from a wrong copy with the same seed length: same
    // version, different text.
    let view = sim.clients[0].session.project_view().expect("view");
    let a = view.id_at_path("a.typ").expect("a");
    let doc = LiveDoc::<LoroEngine>::seeded(101, "OMEGA").expect("doc");
    sim.call(1, |s, now| s.provide_doc(now, a, doc, false));
    assert!(sim.heal(120_000), "did not recover");
    assert_eq!(sim.clients[1].counters.refetches, 1);
    assert_eq!(
        sim.clients[1]
            .session
            .file_doc(a)
            .map(|d| d.engine().text())
            .as_deref(),
        Some("alpha")
    );
    assert_eq!(sim.clients[1].session.state(), SessionState::Live);
}

#[test]
fn presence_tells_which_peers_have_which_file_open() {
    let mut sim = ProjectSim::<LoroEngine>::new(3, &small_project(), 21, NetConfig::CLEAN, keep)
        .expect("sim");
    assert!(sim.heal(60_000));
    let texts = live_ids(&mut sim, 0, NodeKind::Text);
    let (a, b) = (texts[0], texts[1]);
    assert!(sim.open(1, a));
    assert!(sim.open(1, b));
    assert!(sim.open(2, b));
    assert!(sim.open(2, a));
    assert!(sim.open(2, b));
    sim.advance(1_000);
    let on_a = sim.clients[0].session.peers_by_file(a);
    assert_eq!(on_a.len(), 2, "both opened a at some point");
    assert!(on_a.iter().all(|p| !p.active), "both moved on to b");
    let on_b = sim.clients[0].session.peers_by_file(b);
    assert_eq!(on_b.len(), 2);
    assert!(on_b.iter().all(|p| p.active));
    // Closing a file removes the peer from its list.
    sim.call(2, |s, _| s.set_focus(Some(b), vec![b]));
    sim.advance(1_000);
    assert_eq!(sim.clients[0].session.peers_by_file(a).len(), 1);
}

#[test]
fn an_unloaded_document_restores_from_its_snapshot_and_merges() {
    let mut sim = ProjectSim::<LoroEngine>::new(2, &small_project(), 31, NetConfig::CLEAN, keep)
        .expect("sim");
    assert!(sim.heal(60_000));
    let texts = live_ids(&mut sim, 1, NodeKind::Text);
    let id = texts[0];
    let snapshot = sim.clients[1]
        .session
        .unload_doc(id)
        .expect("unload")
        .expect("was loaded");
    assert!(!sim.clients[1].session.is_loaded(id));
    // The host edits while the guest has it evicted.
    sim.edit(0, id, 0, 0, "NEW ").expect("edit");
    sim.advance(500);
    let doc = LiveDoc::from_engine(LoroEngine::from_snapshot(101, &snapshot).expect("restore"));
    sim.call(1, |s, now| s.provide_doc(now, id, doc, true));
    assert!(sim.heal(60_000));
}

#[test]
fn create_rename_delete_and_partitions_converge() {
    let mut sim = ProjectSim::<LoroEngine>::new(
        3,
        &small_project(),
        99,
        NetConfig::HOSTILE,
        |cfg: &mut SessionConfig| cfg.resync_interval_ms = 3_000,
    )
    .expect("sim");
    sim.advance(10_000);
    let view = sim.clients[0].session.project_view().expect("view");
    let main = view.id_at_path("main.typ").expect("main");
    let a = view.id_at_path("ch/a.typ").expect("a");
    let b = view.id_at_path("ch/b.typ").expect("b");
    // Everyone types, creates, renames and deletes, some while cut off.
    sim.disconnect(2);
    sim.edit(1, main, 0, 0, "peer1: ").expect("edit");
    sim.edit(2, main, 0, 0, "peer2: ").expect("edit");
    let new = sim
        .create_text(2, "ch/new.typ", "created offline\n")
        .expect("create");
    sim.rename(0, a, "a2.typ").expect("rename");
    sim.delete(1, b).expect("delete");
    sim.create_blob(0, "extra.bin", vec![5; 3000])
        .expect("blob");
    sim.advance(4_000);
    sim.connect(2);
    sim.edit(2, new, 0, 0, ">> ").expect("edit");
    assert!(sim.heal(300_000), "did not converge");
    let view = sim.clients[1].session.project_view().expect("view");
    assert!(view.id_at_path("ch/a2.typ").is_some());
    assert!(view.id_at_path("ch/a.typ").is_none());
    assert!(view.id_at_path("ch/b.typ").is_none());
    assert!(view.id_at_path("extra.bin").is_some());
    assert_eq!(view.id_at_path("ch/new.typ"), Some(new));
    let text = sim.clients[0]
        .session
        .file_doc(main)
        .map(|d| d.engine().text())
        .expect("main loaded");
    assert!(text.contains("peer1: ") && text.contains("peer2: "));
    let created = sim.clients[1]
        .session
        .file_doc(new)
        .map(|d| d.engine().text());
    assert_eq!(created.as_deref(), Some(">> created offline\n"));
    assert!(sim.clients.iter().all(|c| c.blobs.len() == 2));
}

// -- proptest: N peers x M docs ------------------------------------------------------

const SNIPPETS: &[&str] = &["a", "hello ", "é", "😀", "日本語", "\n", "x😀y", " "];
const NAMES: &[&str] = &["n1.typ", "n2.typ", "notes.md", "n1.typ", "Ünï.typ"];

#[derive(Debug, Clone)]
enum Op {
    Type {
        client: usize,
        file: u8,
        at: u16,
        del: u8,
        snippet: u8,
    },
    Create {
        client: usize,
        name: u8,
    },
    Rename {
        client: usize,
        file: u8,
        name: u8,
    },
    Delete {
        client: usize,
        file: u8,
    },
    Open {
        client: usize,
        file: u8,
    },
    Offline(usize),
    Online(usize),
    Wait(u16),
}

fn op_strategy(clients: usize) -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => (0..clients, any::<u8>(), 0u16..=1000, 0u8..5, any::<u8>())
            .prop_map(|(client, file, at, del, snippet)| Op::Type { client, file, at, del, snippet }),
        2 => (0..clients, any::<u8>()).prop_map(|(client, name)| Op::Create { client, name }),
        1 => (0..clients, any::<u8>(), any::<u8>()).prop_map(|(client, file, name)| Op::Rename { client, file, name }),
        1 => (0..clients, any::<u8>()).prop_map(|(client, file)| Op::Delete { client, file }),
        1 => (0..clients, any::<u8>()).prop_map(|(client, file)| Op::Open { client, file }),
        1 => (0..clients).prop_map(Op::Offline),
        1 => (0..clients).prop_map(Op::Online),
        2 => (1u16..2500).prop_map(Op::Wait),
    ]
}

fn texts_of<E: TextCrdt>(sim: &mut ProjectSim<E>, client: usize) -> Vec<FileId> {
    let loaded: Vec<FileId> = sim.clients[client].session.loaded_docs().collect();
    sim.clients[client]
        .session
        .project_view()
        .map(|v| {
            v.entries()
                .iter()
                .filter(|e| e.meta.kind == NodeKind::Text && loaded.contains(&e.id))
                .map(|e| e.id)
                .collect()
        })
        .unwrap_or_default()
}

fn any_node<E: TextCrdt>(sim: &mut ProjectSim<E>, client: usize) -> Vec<FileId> {
    sim.clients[client]
        .session
        .project_view()
        .map(|v| v.entries().iter().map(|e| e.id).collect())
        .unwrap_or_default()
}

fn run_project<E: TextCrdt>(ops: &[Op], seed: u64) -> Result<(), TestCaseError> {
    let clients = 3;
    let mut sim = ProjectSim::<E>::new(
        clients,
        &small_project(),
        seed,
        NetConfig::HOSTILE,
        |cfg: &mut SessionConfig| {
            cfg.resync_interval_ms = 3_000;
        },
    )
    .map_err(|e| TestCaseError::fail(e.to_string()))?;
    let fail = |e: manang_live::LiveError| TestCaseError::fail(e.to_string());
    let mut created = 0usize;
    for op in ops {
        match op {
            Op::Type {
                client,
                file,
                at,
                del,
                snippet,
            } => {
                let ids = texts_of(&mut sim, *client);
                if ids.is_empty() {
                    continue;
                }
                let id = ids[usize::from(*file) % ids.len()];
                let len = sim.clients[*client]
                    .session
                    .file_doc(id)
                    .map_or(0, |d| d.engine().text().chars().count());
                let from = len * usize::from(*at) / 1000;
                let to = (from + usize::from(*del)).min(len);
                let text = SNIPPETS[usize::from(*snippet) % SNIPPETS.len()];
                sim.edit(*client, id, from, to, text).map_err(fail)?;
            }
            Op::Create { client, name } => {
                created += 1;
                let path = format!("{created}-{}", NAMES[usize::from(*name) % NAMES.len()]);
                sim.create_text(*client, &path, "fresh é\n").map_err(fail)?;
            }
            Op::Rename { client, file, name } => {
                let ids = any_node(&mut sim, *client);
                if let Some(id) = ids.get(usize::from(*file) % ids.len().max(1)) {
                    let n = NAMES[usize::from(*name) % NAMES.len()];
                    // An invalid or cyclic edit is refused by the model: fine.
                    let _ = sim.rename(*client, *id, n);
                }
            }
            Op::Delete { client, file } => {
                let ids = texts_of(&mut sim, *client);
                if let Some(id) = ids.get(usize::from(*file) % ids.len().max(1)) {
                    let _ = sim.delete(*client, *id);
                }
            }
            Op::Open { client, file } => {
                let ids = texts_of(&mut sim, *client);
                if let Some(id) = ids.get(usize::from(*file) % ids.len().max(1)) {
                    sim.open(*client, *id);
                }
            }
            Op::Offline(c) => {
                if sim.clients[*c].online {
                    sim.disconnect(*c);
                }
            }
            Op::Online(c) => {
                if !sim.clients[*c].online {
                    sim.connect(*c);
                }
            }
            Op::Wait(ms) => sim.advance(u64::from(*ms)),
        }
    }
    prop_assert!(sim.heal(900_000), "did not converge");
    // Open buffers match their documents.
    for c in &sim.clients {
        for (id, rope) in &c.ropes {
            if let Some(doc) = c.session.file_doc(*id) {
                prop_assert_eq!(rope.to_string(), doc.engine().text());
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, .. ProptestConfig::default() })]

    #[test]
    fn peers_and_documents_converge_loro(
        ops in proptest::collection::vec(op_strategy(3), 1..45),
        seed in any::<u64>(),
    ) {
        run_project::<LoroEngine>(&ops, seed)?;
    }

    #[test]
    fn peers_and_documents_converge_yrs(
        ops in proptest::collection::vec(op_strategy(3), 1..45),
        seed in any::<u64>(),
    ) {
        run_project::<YrsEngine>(&ops, seed)?;
    }
}
