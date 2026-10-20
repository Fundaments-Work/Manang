//! Building the host's project tree from the folder.
//!
//! [`scan_project`] walks the folder through the [`PathPolicy`] (so a path the
//! watcher would never report and a path a guest could never be sent is not
//! part of the project), reads and hashes every shareable file once and
//! returns what the tree needs: directories, text files (with the seed
//! fingerprint a joiner uses to seed an identical document) and blobs.
//! [`build_project`] turns that into a [`ProjectDoc`].
//!
//! Nothing is written. Symlinks are never followed or shared; files over the
//! blob cap are skipped and counted; a text file over the text cap becomes a
//! blob (last writer wins) instead of a document in memory.

use std::collections::BTreeMap;
use std::fs;

use manang_live::LiveError;
use manang_live::envelope::seed_fingerprint;
use manang_live::project::{FileId, NodeKind, NodeMeta, ProjectDoc};

use crate::disk::{DiskError, Hash, MAX_SCAN_FILES, read_capped, sha256};
use crate::fsbridge::classify;
use crate::pathsec::{MAX_BLOB_BYTES, PathPolicy};

/// One shareable entry of the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanItem {
    /// Project-relative path, `/` separated.
    pub rel: String,
    /// Directory, text or blob.
    pub kind: NodeKind,
    /// Content size (0 for a directory).
    pub size: u64,
    /// SHA-256 of the content (zeros for a directory).
    pub sha256: Hash,
    /// Seed fingerprint of a text file's content (zeros otherwise).
    pub seed_fp: [u8; 16],
}

/// The folder as the host will share it.
#[derive(Debug, Default)]
pub(crate) struct Scan {
    /// Entries sorted by path.
    pub items: Vec<ScanItem>,
    /// Files left out (over the size cap or unreadable).
    pub skipped: usize,
}

/// Walk the folder under `policy` and describe everything that may be shared.
pub(crate) fn scan_project(policy: &PathPolicy) -> Result<Scan, DiskError> {
    let mut scan = Scan::default();
    let mut seen = 0usize;
    walk(policy, "", &mut scan, &mut seen)?;
    scan.items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(scan)
}

/// Describe everything shareable below the directory `rel_dir` (a directory
/// that appeared while the watcher was still attaching to it).
pub(crate) fn scan_dir(policy: &PathPolicy, rel_dir: &str) -> Result<Scan, DiskError> {
    let mut scan = Scan::default();
    let mut seen = 0usize;
    walk(policy, rel_dir, &mut scan, &mut seen)?;
    scan.items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(scan)
}

fn walk(
    policy: &PathPolicy,
    rel_dir: &str,
    scan: &mut Scan,
    seen: &mut usize,
) -> Result<(), DiskError> {
    let dir = policy.root().path().join(rel_dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let rel = if rel_dir.is_empty() {
            name
        } else {
            format!("{rel_dir}/{name}")
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || policy.check_rel(&rel).is_err() {
            continue;
        }
        *seen += 1;
        if *seen > MAX_SCAN_FILES {
            return Err(DiskError::TooMany);
        }
        if file_type.is_dir() {
            scan.items.push(ScanItem {
                rel: rel.clone(),
                kind: NodeKind::Dir,
                size: 0,
                sha256: [0; 32],
                seed_fp: [0; 16],
            });
            walk(policy, &rel, scan, seen)?;
        } else if file_type.is_file() {
            match describe_file(policy, &entry.path(), &rel) {
                Some(item) => scan.items.push(item),
                None => scan.skipped += 1,
            }
        }
    }
    Ok(())
}

fn describe_file(policy: &PathPolicy, abs: &std::path::Path, rel: &str) -> Option<ScanItem> {
    let bytes = read_capped(abs, MAX_BLOB_BYTES).ok()?;
    let (kind, text) = classify(&bytes);
    let size = bytes.len() as u64;
    let (kind, seed_fp) = match (kind, text) {
        (NodeKind::Text, Some(text)) if policy.check_size(NodeKind::Text, size).is_ok() => {
            (NodeKind::Text, seed_fingerprint(&text))
        }
        _ => (NodeKind::Blob, [0; 16]),
    };
    Some(ScanItem {
        rel: rel.to_owned(),
        kind,
        size,
        sha256: sha256(&bytes),
        seed_fp,
    })
}

/// Build the host's tree from a scan. Returns the tree and the id of every
/// path, for the first file loads.
pub(crate) fn build_project(
    peer: u64,
    scan: &Scan,
) -> Result<(ProjectDoc, BTreeMap<String, FileId>), LiveError> {
    let mut project = ProjectDoc::new(peer)?;
    let mut ids: BTreeMap<String, FileId> = BTreeMap::new();
    for item in &scan.items {
        let (parent_path, name) = split(&item.rel);
        let parent = match parent_path {
            Some(dir) => Some(ensure_dir_cached(&mut project, &mut ids, dir)?),
            None => None,
        };
        if item.kind == NodeKind::Dir {
            if !ids.contains_key(&item.rel) {
                let id = project.create(parent, &NodeMeta::dir(name))?;
                ids.insert(item.rel.clone(), id);
            }
            continue;
        }
        let meta = NodeMeta::file(name, item.kind, item.size, item.sha256, item.seed_fp);
        let id = project.create(parent, &meta)?;
        ids.insert(item.rel.clone(), id);
    }
    Ok((project, ids))
}

/// `(parent directory, name)` of a project path.
pub(crate) fn split(path: &str) -> (Option<&str>, &str) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, path),
    }
}

fn ensure_dir_cached(
    project: &mut ProjectDoc,
    ids: &mut BTreeMap<String, FileId>,
    dir: &str,
) -> Result<FileId, LiveError> {
    let mut parent: Option<FileId> = None;
    let mut prefix = String::new();
    for part in dir.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        let id = match ids.get(&prefix) {
            Some(id) => *id,
            None => {
                let id = project.create(parent, &NodeMeta::dir(part))?;
                ids.insert(prefix.clone(), id);
                id
            }
        };
        parent = Some(id);
    }
    parent.ok_or_else(|| LiveError::Project("empty directory path".into()))
}

/// The directory node of `dir`, creating missing levels (local tree
/// operations; looks the tree up once).
pub(crate) fn ensure_dir(project: &mut ProjectDoc, dir: &str) -> Result<FileId, LiveError> {
    let view = project.view();
    let mut ids: BTreeMap<String, FileId> = BTreeMap::new();
    let mut prefix = String::new();
    for part in dir.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if let Some(id) = view.id_at_path(&prefix) {
            ids.insert(prefix.clone(), id);
        }
    }
    ensure_dir_cached(project, &mut ids, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_skips_what_the_policy_refuses_and_describes_the_rest() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path();
        fs::create_dir_all(root.join("ch")).expect("dir");
        fs::create_dir_all(root.join(".git")).expect("git");
        fs::create_dir_all(root.join("node_modules/x")).expect("nm");
        fs::write(root.join("main.typ"), "hello").expect("w");
        fs::write(root.join("ch/a.typ"), "a").expect("w");
        fs::write(root.join("fig.png"), [0u8, 159, 146, 150]).expect("w");
        fs::write(root.join(".env"), "SECRET").expect("w");
        fs::write(root.join("manang.toml"), "[project]").expect("w");
        fs::write(root.join(".git/config"), "x").expect("w");
        fs::write(root.join("node_modules/x/i.js"), "x").expect("w");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", root.join("link.txt")).expect("link");
        let policy = PathPolicy::new(root).expect("policy");
        let scan = scan_project(&policy).expect("scan");
        let listed: Vec<(&str, NodeKind)> = scan
            .items
            .iter()
            .map(|i| (i.rel.as_str(), i.kind))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("ch", NodeKind::Dir),
                ("ch/a.typ", NodeKind::Text),
                ("fig.png", NodeKind::Blob),
                ("main.typ", NodeKind::Text),
            ]
        );
        let main = scan.items.iter().find(|i| i.rel == "main.typ").expect("m");
        assert_eq!(main.seed_fp, seed_fingerprint("hello"));
        assert_eq!(main.sha256, sha256(b"hello"));
        let (project, ids) = build_project(10, &scan).expect("build");
        assert_eq!(ids.len(), 4);
        let view = project.view();
        assert_eq!(view.entries().len(), 4);
        assert_eq!(view.path_of(ids["ch/a.typ"]), Some("ch/a.typ"));
    }
}
