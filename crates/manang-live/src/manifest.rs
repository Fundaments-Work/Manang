//! The project manifest: what a joiner fetches first.
//!
//! A manifest lists every node of the project (path, kind, size, content
//! hash, seed fingerprint). It travels as a few bounded pages
//! ([`crate::protocol::Message::Manifest`]); [`ManifestAssembler`] puts the
//! pages back together and checks the announced root hash. The *root* is a
//! 16-byte digest of the content-relevant fields only (no file ids), so a
//! joiner can compute the same root from its own disk and tell "identical
//! project" from "differs" before connecting; it fits the `seed` field of an
//! invite ([`crate::envelope::Invite`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::blob::MAX_BLOB_BYTES;
use crate::error::LiveError;
use crate::project::{FileId, NodeKind, valid_name};

/// Longest accepted project-relative path, in bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// Most entries in one manifest page.
pub const MAX_PAGE_ENTRIES: usize = 256;
/// Most entries in a whole manifest.
pub const MAX_MANIFEST_ENTRIES: usize = 65_536;
/// Most pages in a manifest.
pub const MAX_MANIFEST_PAGES: u32 = (MAX_MANIFEST_ENTRIES / MAX_PAGE_ENTRIES) as u32;

/// Which document a message is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocId {
    /// The project tree ([`crate::project::ProjectDoc`]).
    Project,
    /// The live text document of one file.
    File(FileId),
}

/// One manifest line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Stable identity.
    pub id: FileId,
    /// Project-relative path, `/` separated, collision-resolved.
    pub path: String,
    /// Kind of node.
    pub kind: NodeKind,
    /// Content size in bytes (0 for directories).
    pub size: u64,
    /// SHA-256 of the content (zeros for directories).
    pub sha256: [u8; 32],
    /// Seed fingerprint (zeros unless a text file).
    pub seed_fp: [u8; 16],
}

/// Whether `path` is a safe relative path: components all [`valid_name`].
#[must_use]
pub fn valid_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= MAX_PATH_BYTES && path.split('/').all(valid_name)
}

impl ManifestEntry {
    /// Check a (possibly hostile) entry.
    pub fn validate(&self) -> Result<(), LiveError> {
        if !valid_path(&self.path) {
            return Err(LiveError::Project("manifest path is not valid".into()));
        }
        if self.size > MAX_BLOB_BYTES {
            return Err(LiveError::Project("manifest size over the cap".into()));
        }
        if self.kind == NodeKind::Dir && self.size != 0 {
            return Err(LiveError::Project("directory with a size".into()));
        }
        Ok(())
    }
}

/// Manifest root: 16 bytes of SHA-256 over every entry's `(path, kind, size,
/// sha256, seed_fp)`, sorted, with length prefixes. Independent of file ids
/// and of the order of `entries`.
#[must_use]
pub fn manifest_root(entries: &[ManifestEntry]) -> [u8; 16] {
    let mut keys: Vec<&ManifestEntry> = entries.iter().collect();
    keys.sort_by(|a, b| {
        (&a.path, a.kind, a.size, a.sha256, a.seed_fp)
            .cmp(&(&b.path, b.kind, b.size, b.sha256, b.seed_fp))
    });
    let mut h = Sha256::new();
    h.update(b"manang-live/manifest-root/v1");
    h.update((keys.len() as u64).to_le_bytes());
    for e in keys {
        h.update((e.path.len() as u64).to_le_bytes());
        h.update(e.path.as_bytes());
        h.update([match e.kind {
            NodeKind::Dir => 0u8,
            NodeKind::Text => 1,
            NodeKind::Blob => 2,
        }]);
        h.update(e.size.to_le_bytes());
        h.update(e.sha256);
        h.update(e.seed_fp);
    }
    let digest = h.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

/// Split a manifest into pages of at most [`MAX_PAGE_ENTRIES`] entries (at
/// least one page, possibly empty, so an empty project is still answerable).
#[must_use]
pub fn paginate(entries: &[ManifestEntry]) -> Vec<Vec<ManifestEntry>> {
    if entries.is_empty() {
        return vec![Vec::new()];
    }
    entries
        .chunks(MAX_PAGE_ENTRIES)
        .map(<[ManifestEntry]>::to_vec)
        .collect()
}

/// Reassembles the pages of one announced manifest.
#[derive(Debug)]
pub struct ManifestAssembler {
    root: [u8; 16],
    pages: u32,
    got: BTreeMap<u32, Vec<ManifestEntry>>,
}

impl ManifestAssembler {
    /// Start from the first page's header.
    pub fn new(root: [u8; 16], pages: u32) -> Result<Self, LiveError> {
        if pages == 0 || pages > MAX_MANIFEST_PAGES {
            return Err(LiveError::Project(
                "manifest page count out of range".into(),
            ));
        }
        Ok(Self {
            root,
            pages,
            got: BTreeMap::new(),
        })
    }

    /// Accept a page. Returns the full, verified entry list once every page
    /// is in. A root or page-count mismatch is an error (the sender changed
    /// its project mid-transfer: ask again).
    pub fn accept(
        &mut self,
        root: [u8; 16],
        page: u32,
        pages: u32,
        entries: Vec<ManifestEntry>,
    ) -> Result<Option<Vec<ManifestEntry>>, LiveError> {
        if root != self.root || pages != self.pages {
            return Err(LiveError::Project(
                "manifest changed during transfer".into(),
            ));
        }
        if page >= self.pages || entries.len() > MAX_PAGE_ENTRIES {
            return Err(LiveError::Project("manifest page out of range".into()));
        }
        for e in &entries {
            e.validate()?;
        }
        self.got.insert(page, entries);
        if self.got.len() != self.pages as usize {
            return Ok(None);
        }
        let all: Vec<ManifestEntry> = std::mem::take(&mut self.got)
            .into_values()
            .flatten()
            .collect();
        if all.len() > MAX_MANIFEST_ENTRIES {
            return Err(LiveError::Project("manifest too large".into()));
        }
        if manifest_root(&all) != self.root {
            return Err(LiveError::Project("manifest root mismatch".into()));
        }
        Ok(Some(all))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(i: u128, path: &str) -> ManifestEntry {
        ManifestEntry {
            id: FileId(i),
            path: path.into(),
            kind: NodeKind::Text,
            size: i as u64,
            sha256: [i as u8; 32],
            seed_fp: [0; 16],
        }
    }

    #[test]
    fn root_ignores_ids_and_order() {
        let a = vec![entry(1, "a.typ"), entry(2, "b/c.typ")];
        let mut b = vec![entry(2, "b/c.typ"), entry(1, "a.typ")];
        b[0].id = FileId(99);
        b[1].id = FileId(98);
        assert_eq!(manifest_root(&a), manifest_root(&b));
        b[0].size += 1;
        assert_ne!(manifest_root(&a), manifest_root(&b));
    }

    #[test]
    fn pages_reassemble_and_verify() {
        let all: Vec<ManifestEntry> = (0..600).map(|i| entry(i, &format!("f{i}.txt"))).collect();
        let root = manifest_root(&all);
        let pages = paginate(&all);
        assert_eq!(pages.len(), 3);
        let mut asm = ManifestAssembler::new(root, 3).expect("asm");
        assert_eq!(asm.accept(root, 2, 3, pages[2].clone()).expect("p2"), None);
        assert_eq!(asm.accept(root, 0, 3, pages[0].clone()).expect("p0"), None);
        let done = asm.accept(root, 1, 3, pages[1].clone()).expect("p1");
        assert_eq!(done.map(|v| v.len()), Some(600));
    }

    #[test]
    fn hostile_pages_fail() {
        let root = [1; 16];
        assert!(ManifestAssembler::new(root, 0).is_err());
        assert!(ManifestAssembler::new(root, MAX_MANIFEST_PAGES + 1).is_err());
        let mut asm = ManifestAssembler::new(root, 1).expect("asm");
        assert!(asm.accept(root, 1, 1, vec![]).is_err());
        assert!(asm.accept([2; 16], 0, 1, vec![]).is_err());
        assert!(asm.accept(root, 0, 1, vec![entry(1, "../x")]).is_err());
        assert!(asm.accept(root, 0, 1, vec![entry(1, "a.txt")]).is_err()); // wrong root
        assert_eq!(paginate(&[]), vec![Vec::<ManifestEntry>::new()]);
    }

    #[test]
    fn path_validation() {
        assert!(valid_path("a/b.txt"));
        for bad in [
            "", "/a", "a//b", "a/../b", "a\\b", "a/", ".", "a\0b", "con:",
        ] {
            assert!(!valid_path(bad), "{bad:?}");
        }
        assert!(!valid_path(&"a".repeat(MAX_PATH_BYTES + 1)));
    }
}
