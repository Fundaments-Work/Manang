//! Content-addressed blobs: chunking, verification and bounds.
//!
//! Binary and figure files are tree nodes ([`crate::project`]) whose content
//! is addressed by SHA-256 and moved in chunks of at most [`CHUNK_SIZE`]
//! bytes. Everything here is pure and bounded: a hostile peer can make an
//! assembler fail, never allocate past [`MAX_BLOB_BYTES`] or panic.

use sha2::{Digest, Sha256};

use crate::error::LiveError;

/// Largest chunk on the wire, in bytes.
pub const CHUNK_SIZE: usize = 1024 * 1024;
/// Largest blob (and largest non-directory file) a project session carries.
pub const MAX_BLOB_BYTES: u64 = 64 * 1024 * 1024;
/// Most chunks a blob can have.
pub const MAX_CHUNKS: u32 = (MAX_BLOB_BYTES / CHUNK_SIZE as u64) as u32;

/// A SHA-256 content address.
pub type BlobHash = [u8; 32];

/// SHA-256 of `data`.
#[must_use]
pub fn blob_hash(data: &[u8]) -> BlobHash {
    Sha256::digest(data).into()
}

/// How many chunks a blob of `size` bytes has (an empty blob has one empty
/// chunk, so every blob needs at least one `BlobChunk` to be transferred).
pub fn chunk_count(size: u64) -> Result<u32, LiveError> {
    if size > MAX_BLOB_BYTES {
        return Err(LiveError::Blob("blob exceeds the size cap"));
    }
    let n = size.div_ceil(CHUNK_SIZE as u64).max(1);
    u32::try_from(n).map_err(|_| LiveError::Blob("too many chunks"))
}

/// Byte range of chunk `index` of a blob of `size` bytes.
pub fn chunk_range(size: u64, index: u32) -> Result<std::ops::Range<usize>, LiveError> {
    if index >= chunk_count(size)? {
        return Err(LiveError::Blob("chunk index out of range"));
    }
    let start = u64::from(index) * CHUNK_SIZE as u64;
    let end = (start + CHUNK_SIZE as u64).min(size);
    let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
        return Err(LiveError::Blob("blob does not fit in memory"));
    };
    Ok(start..end)
}

/// Split `data` into wire chunks (`[b""]` for an empty blob).
pub fn split(data: &[u8]) -> Result<Vec<&[u8]>, LiveError> {
    let size = u64::try_from(data.len()).map_err(|_| LiveError::Blob("blob too large"))?;
    let n = chunk_count(size)?;
    (0..n)
        .map(|i| {
            chunk_range(size, i)
                .and_then(|r| data.get(r).ok_or(LiveError::Blob("chunk out of range")))
        })
        .collect()
}

/// Collects the chunks of one expected blob and verifies its hash.
#[derive(Debug)]
pub struct BlobAssembler {
    hash: BlobHash,
    size: u64,
    chunks: Vec<Option<Vec<u8>>>,
    received: u32,
}

impl BlobAssembler {
    /// Expect a blob with this hash and size (both from a verified manifest).
    pub fn new(hash: BlobHash, size: u64) -> Result<Self, LiveError> {
        let n = chunk_count(size)?;
        Ok(Self {
            hash,
            size,
            chunks: vec![None; n as usize],
            received: 0,
        })
    }

    /// The blob's expected hash.
    #[must_use]
    pub fn hash(&self) -> &BlobHash {
        &self.hash
    }

    /// Chunks still missing, in order.
    pub fn missing(&self) -> impl Iterator<Item = u32> + '_ {
        self.chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_none())
            .filter_map(|(i, _)| u32::try_from(i).ok())
    }

    /// Whether every chunk has arrived.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.received as usize == self.chunks.len()
    }

    /// Accept one chunk. A duplicate of a held chunk is ignored. Returns
    /// whether the blob is now complete.
    pub fn accept(&mut self, index: u32, count: u32, data: &[u8]) -> Result<bool, LiveError> {
        if count as usize != self.chunks.len() {
            return Err(LiveError::Blob("chunk count disagrees with the manifest"));
        }
        let range = chunk_range(self.size, index)?;
        if data.len() != range.len() {
            return Err(LiveError::Blob("chunk has the wrong length"));
        }
        let Some(slot) = self.chunks.get_mut(index as usize) else {
            return Err(LiveError::Blob("chunk index out of range"));
        };
        if slot.is_none() {
            *slot = Some(data.to_vec());
            self.received += 1;
        }
        Ok(self.is_complete())
    }

    /// The verified content. Fails if chunks are missing or the hash differs
    /// (the chunks are then discarded so the blob can be fetched again).
    pub fn finish(&mut self) -> Result<Vec<u8>, LiveError> {
        if !self.is_complete() {
            return Err(LiveError::Blob("blob is incomplete"));
        }
        let mut out = Vec::with_capacity(usize::try_from(self.size).unwrap_or(0));
        for c in self.chunks.iter().flatten() {
            out.extend_from_slice(c);
        }
        if blob_hash(&out) != self.hash {
            self.chunks.iter_mut().for_each(|c| *c = None);
            self.received = 0;
            return Err(LiveError::Blob("hash mismatch"));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn counts_and_ranges() {
        assert_eq!(chunk_count(0).ok(), Some(1));
        assert_eq!(chunk_count(1).ok(), Some(1));
        assert_eq!(chunk_count(CHUNK_SIZE as u64).ok(), Some(1));
        assert_eq!(chunk_count(CHUNK_SIZE as u64 + 1).ok(), Some(2));
        assert!(chunk_count(MAX_BLOB_BYTES + 1).is_err());
        assert_eq!(chunk_range(0, 0).ok(), Some(0..0));
        assert!(chunk_range(10, 1).is_err());
        assert_eq!(
            chunk_range(CHUNK_SIZE as u64 + 5, 1).ok(),
            Some(CHUNK_SIZE..CHUNK_SIZE + 5)
        );
    }

    #[test]
    fn roundtrip_and_tamper() {
        let data: Vec<u8> = (0..(2 * CHUNK_SIZE + 17))
            .map(|i| (i % 251) as u8)
            .collect();
        let parts = split(&data).expect("split");
        assert_eq!(parts.len(), 3);
        let mut a = BlobAssembler::new(blob_hash(&data), data.len() as u64).expect("new");
        for (i, p) in parts.iter().enumerate().rev() {
            a.accept(i as u32, 3, p).expect("accept");
        }
        assert_eq!(a.finish().expect("finish"), data);

        let mut bad = BlobAssembler::new(blob_hash(&data), data.len() as u64).expect("new");
        let mut tampered = parts[0].to_vec();
        tampered[0] ^= 1;
        bad.accept(0, 3, &tampered).expect("accept");
        bad.accept(1, 3, parts[1]).expect("accept");
        bad.accept(2, 3, parts[2]).expect("accept");
        assert_eq!(bad.finish(), Err(LiveError::Blob("hash mismatch")));
        assert_eq!(bad.missing().count(), 3);
    }

    #[test]
    fn hostile_chunks_are_rejected() {
        let mut a = BlobAssembler::new([0; 32], 10).expect("new");
        assert!(a.accept(0, 2, &[0; 10]).is_err());
        assert!(a.accept(1, 1, &[0; 10]).is_err());
        assert!(a.accept(0, 1, &[0; 9]).is_err());
        assert!(a.finish().is_err());
        assert!(BlobAssembler::new([0; 32], u64::MAX).is_err());
    }

    #[test]
    fn empty_blob() {
        let parts = split(&[]).expect("split");
        assert_eq!(parts, vec![&[0u8; 0][..]]);
        let mut a = BlobAssembler::new(blob_hash(&[]), 0).expect("new");
        assert!(a.accept(0, 1, &[]).expect("accept"));
        assert_eq!(a.finish().expect("finish"), Vec::<u8>::new());
    }

    proptest! {
        #[test]
        fn assembler_never_panics(
            size in 0u64..(3 * CHUNK_SIZE as u64),
            ops in proptest::collection::vec((any::<u32>(), any::<u32>(), 0usize..64), 0..20),
        ) {
            if let Ok(mut a) = BlobAssembler::new([7; 32], size) {
                for (i, c, len) in ops {
                    let _ = a.accept(i, c, &vec![0u8; len]);
                }
                let _ = a.finish();
            }
        }
    }
}
