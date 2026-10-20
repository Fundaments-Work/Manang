#![no_main]

//! `BlobAssembler::accept` / `finish`: chunks of a real blob reassemble in any
//! order to the same bytes; hostile chunks never panic and never produce
//! content that does not match the announced hash.

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use manang_live::blob::{BlobAssembler, CHUNK_SIZE, blob_hash, chunk_count, split};

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // 1. A real blob, chunked, delivered shuffled and with duplicates.
    let len = u.int_in_range(0..=3 * CHUNK_SIZE / 2).unwrap_or(0);
    let body: Vec<u8> = (0..len).map(|i| (i as u8) ^ (len as u8)).collect();
    let hash = blob_hash(&body);
    let chunks = split(&body).expect("blob within the cap splits");
    let count = u32::try_from(chunks.len()).expect("count fits");
    assert_eq!(count, chunk_count(len as u64).expect("count"));
    let mut asm = BlobAssembler::new(hash, len as u64).expect("assembler");
    let mut order: Vec<usize> = (0..chunks.len()).collect();
    for i in (1..order.len()).rev() {
        let j = u.int_in_range(0..=i).unwrap_or(i);
        order.swap(i, j);
    }
    for &i in &order {
        let complete = asm.accept(i as u32, count, chunks[i]).expect("good chunk");
        assert_eq!(complete, asm.is_complete());
        // A duplicate is ignored.
        asm.accept(i as u32, count, chunks[i]).expect("duplicate");
    }
    assert_eq!(asm.missing().count(), 0);
    assert_eq!(asm.finish().expect("verified"), body);

    // 2. Hostile: arbitrary hash, size, chunk header and payload.
    let size: u64 = if u.ratio(1, 2).unwrap_or(true) {
        u.int_in_range(0..=4 * CHUNK_SIZE as u64).unwrap_or(0)
    } else {
        u.arbitrary().unwrap_or(0)
    };
    let hash: [u8; 32] = u.arbitrary().unwrap_or([0; 32]);
    let Ok(mut asm) = BlobAssembler::new(hash, size) else {
        return;
    };
    for _ in 0..u.int_in_range(0..=12u8).unwrap_or(0) {
        let index: u32 = u.int_in_range(0..=6u32).unwrap_or(0);
        let claimed: u32 = if u.ratio(3, 4).unwrap_or(true) {
            chunk_count(size).unwrap_or(0)
        } else {
            u.arbitrary().unwrap_or(0)
        };
        // Right-length payloads are the interesting ones; sometimes wrong.
        let want = manang_live::blob::chunk_range(size, index)
            .map(|r| r.len())
            .unwrap_or(0);
        let n = if u.ratio(3, 4).unwrap_or(true) {
            want
        } else {
            u.int_in_range(0..=CHUNK_SIZE + 1).unwrap_or(0)
        };
        let payload: Vec<u8> = (0..n).map(|i| (i as u8).wrapping_mul(31)).collect();
        let _ = asm.accept(index, claimed, &payload);
        if u.ratio(1, 4).unwrap_or(false) {
            // With a random hash, finishing can only ever fail.
            assert!(asm.finish().is_err() || size == 0 && blob_hash(&[]) == hash);
        }
    }
});
