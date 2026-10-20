//! Project tree: convergence, collisions, oracle, wire frames.

use manang_live::manifest::{DocId, ManifestEntry, manifest_root, paginate};
use manang_live::presence::{MAX_OPEN, Presence, PresenceMessage, PresenceV2};
use manang_live::project::{
    FileId, LoroTreeEngine, NodeKind, NodeMeta, ProjectDoc, TreeChange, TreeCrdt, valid_name,
};
use manang_live::protocol::{Message, SyncStage};
use proptest::prelude::*;
use std::collections::BTreeMap;

type Doc = ProjectDoc<LoroTreeEngine>;

fn file(name: &str) -> NodeMeta {
    NodeMeta::file(name, NodeKind::Text, 3, [7; 32], [1; 16])
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Deliver everything `from` has that `to` lacks.
fn sync_one_way(from: &Doc, to: &mut Doc) {
    let diff = from.export_since(&to.version()).expect("export");
    to.apply_update(&diff).expect("apply");
}

fn sync_pair(a: &mut Doc, b: &mut Doc) {
    sync_one_way(a, b);
    sync_one_way(b, a);
    sync_one_way(a, b);
}

fn paths(d: &Doc) -> Vec<(String, FileId)> {
    d.view()
        .entries()
        .iter()
        .map(|e| (e.path.clone(), e.id))
        .collect()
}

#[test]
fn basic_ops_and_paths() {
    let mut d = Doc::new(10).expect("doc");
    let src = d.create(None, &NodeMeta::dir("src")).expect("dir");
    let f = d.create(Some(src), &file("main.typ")).expect("file");
    assert_eq!(d.path_of(f).as_deref(), Some("src/main.typ"));
    let before = d.view();
    d.rename(f, "lib.typ").expect("rename");
    assert_eq!(d.path_of(f).as_deref(), Some("src/lib.typ"));
    d.move_to(f, None).expect("move");
    assert_eq!(d.path_of(f).as_deref(), Some("lib.typ"));
    assert_eq!(d.view().id_at_path("lib.typ"), Some(f));
    assert_eq!(d.view().id_at_path("src/lib.typ"), None);
    d.set_content(f, 9, [2; 32], [0; 16]).expect("content");
    assert!(
        d.diff_since(&before)
            .iter()
            .any(|c| matches!(c, TreeChange::Moved { id, .. } if *id == f))
    );
    d.delete(src).expect("delete");
    assert!(d.path_of(src).is_none());
    assert!(d.delete(src).is_err());
    // Bad input is refused locally.
    assert!(d.create(None, &file("a/b")).is_err());
    assert!(d.create(Some(f), &file("x")).is_err());
    assert!(d.rename(f, "..").is_err());
    assert!(d.create(Some(FileId(u128::MAX)), &file("x")).is_err());
}

#[test]
fn deleting_a_directory_hides_its_subtree() {
    let mut d = Doc::new(10).expect("doc");
    let dir = d.create(None, &NodeMeta::dir("d")).expect("dir");
    let inner = d.create(Some(dir), &NodeMeta::dir("e")).expect("dir");
    let f = d.create(Some(inner), &file("f")).expect("file");
    d.delete(dir).expect("delete");
    assert!(d.view().entries().is_empty(), "{:?}", d.view());
    assert!(d.path_of(f).is_none());
}

#[test]
fn directory_cycle_is_refused_locally() {
    let mut d = Doc::new(10).expect("doc");
    let a = d.create(None, &NodeMeta::dir("a")).expect("a");
    let b = d.create(Some(a), &NodeMeta::dir("b")).expect("b");
    assert!(d.move_to(a, Some(b)).is_err());
    assert!(d.move_to(a, Some(a)).is_err());
}

#[test]
fn concurrent_moves_into_each_other_do_not_cycle() {
    let mut a = Doc::new(1).expect("a");
    let x = a.create(None, &NodeMeta::dir("x")).expect("x");
    let y = a.create(None, &NodeMeta::dir("y")).expect("y");
    let mut b = Doc::new(2).expect("b");
    sync_one_way(&a, &mut b);
    a.move_to(x, Some(y)).expect("x into y");
    b.move_to(y, Some(x)).expect("y into x");
    sync_pair(&mut a, &mut b);
    assert_eq!(paths(&a), paths(&b));
    // Exactly one of them won; the tree is still a tree (both visible).
    assert_eq!(a.view().entries().len(), 2);
}

#[test]
fn same_path_collision_is_deterministic() {
    let mut a = Doc::new(1).expect("a");
    let mut b = Doc::new(2).expect("b");
    let fa = a.create(None, &file("notes.typ")).expect("fa");
    let fb = b.create(None, &file("notes.typ")).expect("fb");
    sync_pair(&mut a, &mut b);
    let pa = paths(&a);
    assert_eq!(pa, paths(&b));
    assert_eq!(pa.len(), 2);
    // Equal Lamport: the lower peer wins the name.
    assert_eq!(a.path_of(fa).as_deref(), Some("notes.typ"));
    let loser = a.path_of(fb).expect("loser");
    assert!(loser.starts_with("notes (conflict "), "{loser}");
    assert!(loser.ends_with(").typ"), "{loser}");
    // A later rename of the loser to a free name clears the conflict.
    b.rename(fb, "other.typ").expect("rename");
    sync_pair(&mut a, &mut b);
    assert_eq!(a.path_of(fb).as_deref(), Some("other.typ"));
}

#[test]
fn earlier_creation_wins_regardless_of_peer() {
    let mut a = Doc::new(9).expect("a");
    let mut b = Doc::new(2).expect("b");
    // `a` creates first, then b learns of it and creates a second node:
    // not a collision of concurrent ops but the same path nonetheless.
    let first = a.create(None, &file("x.txt")).expect("first");
    sync_one_way(&a, &mut b);
    let second = b
        .create(None, &file("X.TXT"))
        .expect("second (case differs)");
    sync_pair(&mut a, &mut b);
    assert_eq!(a.path_of(first).as_deref(), Some("x.txt"));
    assert!(a.path_of(second).expect("p").contains("conflict"));
    assert_eq!(paths(&a), paths(&b));
}

#[test]
fn same_named_directories_merge() {
    let mut a = Doc::new(1).expect("a");
    let mut b = Doc::new(2).expect("b");
    let da = a.create(None, &NodeMeta::dir("src")).expect("da");
    let db = b.create(None, &NodeMeta::dir("src")).expect("db");
    let fa = a.create(Some(da), &file("a.typ")).expect("fa");
    let fb = b.create(Some(db), &file("b.typ")).expect("fb");
    sync_pair(&mut a, &mut b);
    assert_eq!(a.path_of(fa).as_deref(), Some("src/a.typ"));
    assert_eq!(a.path_of(fb).as_deref(), Some("src/b.typ"));
    assert_eq!(paths(&a), paths(&b));
}

#[test]
fn hostile_remote_names_are_neutralised() {
    // A peer that writes straight to the engine, bypassing ProjectDoc checks.
    let mut evil = LoroTreeEngine::new(66).expect("evil");
    let id = evil
        .create(
            None,
            &NodeMeta::file("../../etc/passwd", NodeKind::Text, 0, [0; 32], [0; 16]),
        )
        .expect("create");
    let nul = evil.create(None, &NodeMeta::dir("a\0b")).expect("create");
    let update = evil.export_snapshot().expect("snap");
    let mut d = Doc::new(1).expect("d");
    d.apply_update(&update).expect("apply");
    for e in d.view().entries() {
        assert!(e.path.split('/').all(valid_name), "{}", e.path);
    }
    assert!(d.path_of(id).expect("p").starts_with("invalid-"));
    assert!(d.path_of(nul).is_some());
}

#[test]
fn garbage_updates_are_errors() {
    let mut d = Doc::new(1).expect("d");
    for len in 0..64u8 {
        let junk: Vec<u8> = (0..len)
            .map(|i| i.wrapping_mul(31).wrapping_add(len))
            .collect();
        let _ = d.apply_update(&junk);
        let _ = d.export_since(&junk);
    }
    assert!(d.apply_update(&[0xff; 40]).is_err());
    assert!(d.view().entries().is_empty());
}

#[test]
fn snapshot_roundtrip() {
    let mut d = Doc::new(1).expect("d");
    let dir = d.create(None, &NodeMeta::dir("d")).expect("dir");
    d.create(Some(dir), &file("f.txt")).expect("f");
    let snap = d.export_snapshot().expect("snap");
    let r = Doc::from_snapshot(2, &snap).expect("restore");
    assert_eq!(paths(&d), paths(&r));
    assert_eq!(d.view().manifest(), r.view().manifest());
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Op {
    Create {
        replica: usize,
        parent: usize,
        name: u8,
        dir: bool,
    },
    Rename {
        replica: usize,
        node: usize,
        name: u8,
    },
    Move {
        replica: usize,
        node: usize,
        parent: usize,
    },
    Delete {
        replica: usize,
        node: usize,
    },
    Content {
        replica: usize,
        node: usize,
        size: u8,
    },
    /// One-way delivery, modelling delay; a partition is simply no delivery.
    Deliver {
        from: usize,
        to: usize,
    },
}

const REPLICAS: usize = 3;

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..REPLICAS, 0..8usize, 0..4u8, any::<bool>())
            .prop_map(|(replica, parent, name, dir)| Op::Create { replica, parent, name, dir }),
        2 => (0..REPLICAS, 0..8usize, 0..4u8)
            .prop_map(|(replica, node, name)| Op::Rename { replica, node, name }),
        3 => (0..REPLICAS, 0..8usize, 0..8usize)
            .prop_map(|(replica, node, parent)| Op::Move { replica, node, parent }),
        2 => (0..REPLICAS, 0..8usize).prop_map(|(replica, node)| Op::Delete { replica, node }),
        1 => (0..REPLICAS, 0..8usize, any::<u8>())
            .prop_map(|(replica, node, size)| Op::Content { replica, node, size }),
        5 => (0..REPLICAS, 0..REPLICAS).prop_map(|(from, to)| Op::Deliver { from, to }),
    ]
}

fn name_of(n: u8) -> String {
    ["a", "b", "A.txt", "a.txt"][usize::from(n % 4)].to_owned()
}

fn pick(d: &Doc, i: usize) -> Option<FileId> {
    let v = d.view();
    let e = v.entries();
    (!e.is_empty()).then(|| e[i % e.len()].id)
}

fn check_invariants(d: &Doc) {
    let v = d.view();
    let mut seen = BTreeMap::new();
    for e in v.entries() {
        // Paths are unique (case-folded), safe, and each parent exists and is a dir.
        assert!(
            seen.insert(e.path.to_lowercase(), e.id).is_none() || e.meta.kind == NodeKind::Dir,
            "duplicate path {}",
            e.path
        );
        assert!(e.path.split('/').all(valid_name), "{}", e.path);
        if let Some(p) = e.parent {
            let pe = v.get(p).expect("parent is live");
            assert_eq!(pe.meta.kind, NodeKind::Dir);
            assert!(e.path.starts_with(&format!("{}/", pe.path)));
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Random concurrent edits with delays and partitions: after a final full
    /// exchange every replica shows the same tree, the same manifest and the
    /// same root, and the tree invariants hold throughout.
    #[test]
    fn replicas_converge(ops in proptest::collection::vec(op_strategy(), 1..60)) {
        let mut docs: Vec<Doc> = (0..REPLICAS).map(|i| Doc::new(100 + i as u64).expect("doc")).collect();
        for op in ops {
            match op {
                Op::Create { replica, parent, name, dir } => {
                    let p = pick(&docs[replica], parent).filter(|id| {
                        docs[replica].view().get(*id).is_some_and(|e| e.meta.kind == NodeKind::Dir)
                    });
                    let meta = if dir { NodeMeta::dir(name_of(name)) } else { file(&name_of(name)) };
                    let _ = docs[replica].create(p, &meta);
                }
                Op::Rename { replica, node, name } => {
                    if let Some(id) = pick(&docs[replica], node) {
                        let _ = docs[replica].rename(id, &name_of(name));
                    }
                }
                Op::Move { replica, node, parent } => {
                    if let (Some(id), p) = (pick(&docs[replica], node), pick(&docs[replica], parent)) {
                        let _ = docs[replica].move_to(id, p);
                    }
                }
                Op::Delete { replica, node } => {
                    if let Some(id) = pick(&docs[replica], node) {
                        let _ = docs[replica].delete(id);
                    }
                }
                Op::Content { replica, node, size } => {
                    if let Some(id) = pick(&docs[replica], node) {
                        let _ = docs[replica].set_content(id, u64::from(size), [size; 32], [0; 16]);
                    }
                }
                Op::Deliver { from, to } => {
                    if from != to {
                        let diff = docs[from].export_since(&docs[to].version()).expect("export");
                        docs[to].apply_update(&diff).expect("apply");
                    }
                }
            }
            for d in &docs { check_invariants(d); }
        }
        // Heal the partition: two full rounds of all-pairs exchange.
        for _ in 0..2 {
            for from in 0..REPLICAS {
                for to in 0..REPLICAS {
                    if from != to {
                        let diff = docs[from].export_since(&docs[to].version()).expect("export");
                        docs[to].apply_update(&diff).expect("apply");
                    }
                }
            }
        }
        let reference = docs[0].view();
        let root = manifest_root(&reference.manifest());
        for d in &docs[1..] {
            let v = d.view();
            prop_assert_eq!(v.entries(), reference.entries());
            prop_assert_eq!(manifest_root(&d.view().manifest()), root);
        }
        for d in &docs { check_invariants(d); }
    }

    /// A single replica against a trivial model: ops that succeed change the
    /// model exactly as expected, ops that fail change nothing.
    #[test]
    fn single_replica_matches_oracle(ops in proptest::collection::vec(op_strategy(), 1..60)) {
        let mut d = Doc::new(5).expect("doc");
        // id -> (parent, name, is_dir)
        let mut model: BTreeMap<FileId, (Option<FileId>, String, bool)> = BTreeMap::new();
        let subtree = |model: &BTreeMap<FileId, (Option<FileId>, String, bool)>, root: FileId| {
            let mut out = vec![root];
            let mut i = 0;
            while i < out.len() {
                let cur = out[i];
                out.extend(model.iter().filter(|(_, v)| v.0 == Some(cur)).map(|(k, _)| *k));
                i += 1;
            }
            out
        };
        for op in ops {
            let ids: Vec<FileId> = model.keys().copied().collect();
            let nth = |i: usize| (!ids.is_empty()).then(|| ids[i % ids.len()]);
            match op {
                Op::Create { parent, name, dir, .. } => {
                    let p = nth(parent).filter(|id| model[id].2);
                    let meta = if dir { NodeMeta::dir(name_of(name)) } else { file(&name_of(name)) };
                    let id = d.create(p, &meta).expect("create under a live dir");
                    model.insert(id, (p, name_of(name), dir));
                }
                Op::Rename { node, name, .. } => if let Some(id) = nth(node) {
                    d.rename(id, &name_of(name)).expect("rename");
                    if let Some(m) = model.get_mut(&id) { m.1 = name_of(name); }
                },
                Op::Move { node, parent, .. } => if let Some(id) = nth(node) {
                    let p = nth(parent);
                    let allowed = p.is_none_or(|p| model[&p].2 && !subtree(&model, id).contains(&p));
                    let r = d.move_to(id, p);
                    prop_assert_eq!(r.is_ok(), allowed);
                    if allowed && let Some(m) = model.get_mut(&id) {
                        m.0 = p;
                    }
                },
                Op::Delete { node, .. } => if let Some(id) = nth(node) {
                    d.delete(id).expect("delete");
                    for gone in subtree(&model, id) { model.remove(&gone); }
                },
                Op::Content { node, size, .. } => if let Some(id) = nth(node) {
                    let r = d.set_content(id, u64::from(size), [size; 32], [0; 16]);
                    prop_assert_eq!(r.is_ok(), !model[&id].2);
                },
                Op::Deliver { .. } => {}
            }
            let view = d.view();
            prop_assert_eq!(view.entries().len(), model.len());
            for (id, (parent, _, _)) in &model {
                let e = view.get(*id).expect("model node is in the view");
                prop_assert_eq!(e.parent, *parent);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Wire v2
// ---------------------------------------------------------------------------

fn sample_entry() -> ManifestEntry {
    ManifestEntry {
        id: FileId(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
        path: "src/main.typ".into(),
        kind: NodeKind::Text,
        size: 42,
        sha256: [0xab; 32],
        seed_fp: [0xcd; 16],
    }
}

fn presence() -> Presence {
    Presence {
        peer: 7,
        seq: 3,
        name: "Ada".into(),
        slot: 2,
        email: None,
        selection: None,
    }
}

#[test]
fn wire_frames_snapshot() {
    let frames: Vec<(&str, Vec<u8>)> = vec![
        (
            "sync_v1",
            Message::Sync {
                from: 1,
                to: None,
                stage: SyncStage::Request,
                version: vec![1],
                digest: 5,
                update: vec![],
            }
            .encode()
            .expect("enc"),
        ),
        (
            "update_v1",
            Message::Update {
                from: 1,
                update: vec![9],
            }
            .encode()
            .expect("enc"),
        ),
        (
            "doc_sync",
            Message::DocSync {
                doc: DocId::File(FileId(5)),
                from: 1,
                to: Some(2),
                stage: SyncStage::Reply,
                version: vec![1, 2],
                digest: 9,
                update: vec![3],
            }
            .encode()
            .expect("enc"),
        ),
        (
            "doc_update",
            Message::DocUpdate {
                doc: DocId::Project,
                from: 1,
                update: vec![4],
            }
            .encode()
            .expect("enc"),
        ),
        (
            "manifest",
            Message::Manifest {
                from: 1,
                to: None,
                root: [1; 16],
                page: 0,
                pages: 1,
                entries: vec![sample_entry()],
            }
            .encode()
            .expect("enc"),
        ),
        (
            "blob_get",
            Message::BlobGet {
                from: 1,
                to: Some(2),
                hash: [8; 32],
                chunk: 0,
            }
            .encode()
            .expect("enc"),
        ),
        (
            "blob_chunk",
            Message::BlobChunk {
                from: 1,
                to: None,
                hash: [8; 32],
                chunk: 0,
                count: 1,
                data: vec![1, 2, 3],
            }
            .encode()
            .expect("enc"),
        ),
        (
            "manifest_get",
            Message::ManifestGet { from: 1, to: None }
                .encode()
                .expect("enc"),
        ),
        (
            "presence_v2",
            PresenceMessage::StateV2(PresenceV2 {
                presence: presence(),
                doc: Some(FileId(5)),
                open: vec![FileId(5), FileId(6)],
            })
            .encode()
            .expect("enc"),
        ),
        (
            "presence_v1",
            PresenceMessage::State(presence()).encode().expect("enc"),
        ),
    ];
    let text: String = frames
        .iter()
        .map(|(n, b)| format!("{n}: {}\n", hex(b)))
        .collect();
    insta::assert_snapshot!(text);
}

#[test]
fn v2_roundtrips() {
    let msgs = vec![
        Message::DocSync {
            doc: DocId::Project,
            from: 1,
            to: None,
            stage: SyncStage::Final,
            version: vec![],
            digest: 0,
            update: vec![1; 500],
        },
        Message::DocUpdate {
            doc: DocId::File(FileId(u128::MAX)),
            from: 2,
            update: vec![],
        },
        Message::Manifest {
            from: 1,
            to: Some(3),
            root: [9; 16],
            page: 1,
            pages: 2,
            entries: vec![sample_entry(), sample_entry()],
        },
        Message::BlobGet {
            from: 1,
            to: None,
            hash: [1; 32],
            chunk: 63,
        },
        Message::BlobChunk {
            from: 1,
            to: None,
            hash: [1; 32],
            chunk: 1,
            count: 2,
            data: vec![0; 1000],
        },
        Message::ManifestGet {
            from: 1,
            to: Some(9),
        },
    ];
    for m in msgs {
        assert_eq!(Message::decode(&m.encode().expect("enc")).expect("dec"), m);
    }
}

#[test]
fn v1_frames_keep_their_indices() {
    // Variant 0 and 1 are the v1 messages; v2 starts at 2.
    let sync = Message::Sync {
        from: 1,
        to: None,
        stage: SyncStage::Request,
        version: vec![],
        digest: 0,
        update: vec![],
    };
    let upd = Message::Update {
        from: 1,
        update: vec![],
    };
    assert_eq!(sync.encode().expect("enc")[0], 0);
    assert_eq!(upd.encode().expect("enc")[0], 1);
    let doc = Message::DocUpdate {
        doc: DocId::Project,
        from: 1,
        update: vec![],
    };
    assert_eq!(doc.encode().expect("enc")[0], 3);
    let ps = PresenceMessage::StateV2(PresenceV2 {
        presence: presence(),
        doc: None,
        open: vec![],
    });
    assert_eq!(ps.encode().expect("enc")[0], 2);
    assert_eq!(
        PresenceMessage::Leave { peer: 1 }.encode().expect("enc")[0],
        1
    );
}

#[test]
fn oversized_v2_messages_are_rejected() {
    let too_many: Vec<ManifestEntry> = (0..300).map(|_| sample_entry()).collect();
    let cases = vec![
        Message::Manifest {
            from: 1,
            to: None,
            root: [0; 16],
            page: 0,
            pages: 1,
            entries: too_many,
        },
        Message::Manifest {
            from: 1,
            to: None,
            root: [0; 16],
            page: 1,
            pages: 1,
            entries: vec![],
        },
        Message::Manifest {
            from: 1,
            to: None,
            root: [0; 16],
            page: 0,
            pages: 0,
            entries: vec![],
        },
        Message::Manifest {
            from: 1,
            to: None,
            root: [0; 16],
            page: 0,
            pages: 1,
            entries: vec![ManifestEntry {
                path: "../x".into(),
                ..sample_entry()
            }],
        },
        Message::Manifest {
            from: 1,
            to: None,
            root: [0; 16],
            page: 0,
            pages: 1,
            entries: vec![ManifestEntry {
                path: "a/".repeat(600),
                ..sample_entry()
            }],
        },
        Message::BlobGet {
            from: 1,
            to: None,
            hash: [0; 32],
            chunk: u32::MAX,
        },
        Message::BlobChunk {
            from: 1,
            to: None,
            hash: [0; 32],
            chunk: 0,
            count: 1,
            data: vec![0; 1024 * 1024 + 1],
        },
        Message::BlobChunk {
            from: 1,
            to: None,
            hash: [0; 32],
            chunk: 1,
            count: 1,
            data: vec![],
        },
        Message::BlobChunk {
            from: 1,
            to: None,
            hash: [0; 32],
            chunk: 0,
            count: 1000,
            data: vec![],
        },
        Message::DocSync {
            doc: DocId::Project,
            from: 1,
            to: None,
            stage: SyncStage::Request,
            version: vec![0; 70_000],
            digest: 0,
            update: vec![],
        },
    ];
    for m in cases {
        let bytes = m.encode().expect("enc");
        assert!(Message::decode(&bytes).is_err(), "accepted {m:?}");
    }
}

#[test]
fn presence_v2_is_bounded_and_sanitised() {
    let mut p = presence();
    p.name = "é".repeat(100);
    p.slot = 250;
    p.email = Some("not an email".into());
    let open: Vec<FileId> = (0..40).map(|i| FileId(i % 30)).collect();
    let m = PresenceMessage::StateV2(PresenceV2 {
        presence: p,
        doc: Some(FileId(1)),
        open,
    });
    let PresenceMessage::StateV2(got) =
        PresenceMessage::decode(&m.encode().expect("enc")).expect("dec")
    else {
        panic!("wrong variant");
    };
    assert!(got.open.len() <= MAX_OPEN);
    let mut dedup = got.open.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), got.open.len());
    assert!(got.presence.name.len() <= 64);
    assert!(got.presence.slot < 8);
    assert!(got.presence.email.is_none());

    let mut huge = presence();
    huge.selection = Some(manang_live::presence::Selection {
        anchor: vec![0; 300],
        head: vec![],
    });
    let bad = PresenceMessage::StateV2(PresenceV2 {
        presence: huge,
        doc: None,
        open: vec![],
    });
    assert!(PresenceMessage::decode(&bad.encode().expect("enc")).is_err());
}

#[test]
fn project_room_id_is_separate() {
    use manang_live::envelope::{Entropy, RoomSecret};
    struct Fixed;
    impl Entropy for Fixed {
        fn fill(&mut self, buf: &mut [u8]) -> Result<(), manang_live::LiveError> {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = i as u8;
            }
            Ok(())
        }
    }
    let s = RoomSecret::generate(&mut Fixed).expect("secret");
    let single = s.derive().expect("keys");
    let project = s.derive_project().expect("keys");
    assert_ne!(single.room_id(), project.room_id());
    insta::assert_snapshot!(format!(
        "{} {}",
        single.room_id_hex(),
        project.room_id_hex()
    ));
    // The same AEAD key: a frame sealed for the project room opens with the
    // project keys, but not under the single-file room id (AAD binds it).
    let sealed = project
        .seal_with_nonce(manang_live::envelope::Kind::Data, b"hi", &[0; 24])
        .expect("seal");
    assert!(project.open(&sealed).is_ok());
    assert!(single.open(&sealed).is_err());
}

proptest! {
    #[test]
    fn decoders_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        let _ = Message::decode(&bytes);
        let _ = PresenceMessage::decode(&bytes);
    }

    #[test]
    fn mutated_valid_frames_never_panic(idx in 0usize..200, byte in any::<u8>()) {
        let mut frame = Message::Manifest {
            from: 1, to: None, root: [1; 16], page: 0, pages: 1, entries: vec![sample_entry()],
        }.encode().expect("enc");
        let i = idx % frame.len();
        frame[i] = byte;
        let _ = Message::decode(&frame);
    }
}

#[test]
fn manifest_of_a_view_roundtrips_through_pages() {
    let mut d = Doc::new(1).expect("d");
    for i in 0..300 {
        d.create(None, &file(&format!("f{i}.typ"))).expect("create");
    }
    let m = d.view().manifest();
    let root = manifest_root(&m);
    let pages = paginate(&m);
    assert_eq!(pages.len(), 2);
    let mut asm = manang_live::manifest::ManifestAssembler::new(root, 2).expect("asm");
    let mut done = None;
    for (i, p) in pages.into_iter().enumerate() {
        done = asm.accept(root, i as u32, 2, p).expect("accept");
    }
    assert_eq!(done, Some(m));
}
