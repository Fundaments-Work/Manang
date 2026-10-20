#![no_main]

//! `ProjectDoc::apply_update` / `from_snapshot` on hostile bytes (Loro tree
//! snapshots with a repaired checksum, mutated genuine updates). Never
//! panics; whatever tree results resolves to valid, unique paths.

#[path = "common.rs"]
mod common;

use std::collections::HashSet;

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use manang_live::manifest::valid_path;
use manang_live::project::{FileId, NodeKind, NodeMeta, ProjectDoc};

fn check(doc: &ProjectDoc) {
    let view = doc.view();
    // Same-named directories merge, so directories may share a path; every
    // other node has a path of its own.
    let mut dirs = HashSet::new();
    let mut files = HashSet::new();
    for e in view.entries() {
        assert!(valid_path(&e.path), "resolved path {:?} is not valid", e.path);
        if e.meta.kind == NodeKind::Dir {
            dirs.insert(e.path.clone());
        } else {
            assert!(files.insert(e.path.clone()), "duplicate path {:?}", e.path);
        }
    }
    assert!(dirs.is_disjoint(&files), "a file shares a directory's path");
}

fuzz_target!(|data: &[u8]| {
    let Some((&mode, rest)) = data.split_first() else {
        return;
    };
    let mut u = Unstructured::new(rest);
    match mode % 3 {
        0 => {
            let mut bytes = rest.to_vec();
            common::fix_loro_checksum(&mut bytes);
            if let Ok(mut doc) = ProjectDoc::new(10) {
                let _ = doc.apply_update(&bytes);
                check(&doc);
            }
        }
        1 => {
            let mut bytes = rest.to_vec();
            common::fix_loro_checksum(&mut bytes);
            if let Ok(doc) = ProjectDoc::from_snapshot(11, &bytes) {
                check(&doc);
                let _ = doc.export_snapshot();
            }
        }
        _ => {
            let (Ok(mut a), Ok(mut b)) = (ProjectDoc::new(20), ProjectDoc::new(21)) else {
                return;
            };
            let names = ["a", "b", "c.txt", "d.typ", "a", "Dir"];
            let mut ids: Vec<FileId> = Vec::new();
            let mut sent: Vec<Vec<u8>> = Vec::new();
            for _ in 0..u.int_in_range(0..=16u8).unwrap_or(0) {
                let name = names[u.choose_index(names.len()).unwrap_or(0)];
                let pick = |u: &mut Unstructured<'_>, ids: &[FileId]| {
                    if ids.is_empty() || u.ratio(1, 3).unwrap_or(true) {
                        None
                    } else {
                        Some(ids[u.choose_index(ids.len()).unwrap_or(0)])
                    }
                };
                match u.int_in_range(0..=4u8).unwrap_or(0) {
                    0 => {
                        let parent = pick(&mut u, &ids);
                        if let Ok(id) = a.create(parent, &NodeMeta::dir(name)) {
                            ids.push(id);
                        }
                    }
                    1 => {
                        let parent = pick(&mut u, &ids);
                        let meta = NodeMeta::file(name, NodeKind::Text, 3, [1; 32], [0; 16]);
                        if let Ok(id) = a.create(parent, &meta) {
                            ids.push(id);
                        }
                    }
                    2 => {
                        if let Some(id) = pick(&mut u, &ids) {
                            let _ = a.rename(id, name);
                        }
                    }
                    3 => {
                        if let (Some(id), parent) = (pick(&mut u, &ids), pick(&mut u, &ids)) {
                            let _ = a.move_to(id, parent);
                        }
                    }
                    _ => {
                        if let Some(id) = pick(&mut u, &ids) {
                            let _ = a.delete(id);
                        }
                    }
                }
                if let Ok(mut ups) = a.take_local_updates() {
                    sent.append(&mut ups);
                }
            }
            for up in &sent {
                b.apply_update(up).expect("a genuine update applies");
            }
            assert_eq!(a.view().entries().len(), b.view().entries().len());
            check(&a);
            check(&b);
            let mut blobs = sent;
            if let Ok(s) = a.export_snapshot() {
                blobs.push(s);
            }
            for blob in blobs {
                let bad = common::mutate(&mut u, blob);
                if let Ok(mut c) = ProjectDoc::new(22) {
                    let _ = c.apply_update(&bad);
                    check(&c);
                }
                if let Ok(c) = ProjectDoc::from_snapshot(23, &bad) {
                    check(&c);
                }
                let _ = b.apply_update(&bad);
                check(&b);
            }
        }
    }
});
