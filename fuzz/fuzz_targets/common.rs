//! Helpers shared by the live-collaboration fuzz targets (included with
//! `#[path = "common.rs"] mod common;`).
#![allow(dead_code)] // each target uses a subset

use libfuzzer_sys::arbitrary::Unstructured;

/// Loro refuses any blob whose header checksum is wrong, which would keep a
/// byte-level fuzzer in the first 20 bytes. Rewrite the checksum
/// (`xxh32(blob[20..], seed "LORO")`, little endian, at `16..20`) of anything
/// that starts with the magic, so mutated bodies reach the decoder.
pub fn fix_loro_checksum(bytes: &mut [u8]) {
    if bytes.len() >= 20 && &bytes[..4] == b"loro" {
        let sum = xxhash_rust::xxh32::xxh32(&bytes[20..], u32::from_le_bytes(*b"LORO"));
        bytes[16..20].copy_from_slice(&sum.to_le_bytes());
    }
}

/// Apply up to four byte-level mutations (flip, truncate, append) driven by
/// `u`, then repair the Loro checksum.
pub fn mutate(u: &mut Unstructured<'_>, mut bytes: Vec<u8>) -> Vec<u8> {
    for _ in 0..u.int_in_range(0..=4u8).unwrap_or(0) {
        match u.int_in_range(0..=2u8).unwrap_or(0) {
            0 if !bytes.is_empty() => {
                let i = u.choose_index(bytes.len()).unwrap_or(0);
                bytes[i] ^= u.arbitrary::<u8>().unwrap_or(1) | 1;
            }
            1 => {
                let keep = u.int_in_range(0..=bytes.len()).unwrap_or(0);
                bytes.truncate(keep);
            }
            _ => bytes.push(u.arbitrary().unwrap_or(0)),
        }
    }
    fix_loro_checksum(&mut bytes);
    bytes
}

/// A short string from `u`, including multi-byte characters.
pub fn small_text(u: &mut Unstructured<'_>) -> String {
    let n = u.int_in_range(0..=8u8).unwrap_or(0);
    (0..n)
        .map(|_| match u.int_in_range(0..=5u8).unwrap_or(0) {
            0 => '\u{1F600}',
            1 => 'é',
            2 => '\n',
            _ => char::from(b'a' + u.int_in_range(0..=25u8).unwrap_or(0)),
        })
        .collect()
}
