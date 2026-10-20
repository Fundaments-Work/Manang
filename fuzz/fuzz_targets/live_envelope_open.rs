#![no_main]

//! `RoomKeys::open` with random keys, nonces and frames: never panics; a
//! frame sealed under one key opens only under that key and kind, and any
//! single-byte change to it is refused.

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use manang_live::envelope::{Kind, NONCE_LEN, RoomSecret, SECRET_LEN};

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let key: [u8; SECRET_LEN] = u.arbitrary().unwrap_or([7; SECRET_LEN]);
    let other: [u8; SECRET_LEN] = u.arbitrary().unwrap_or([9; SECRET_LEN]);
    let nonce: [u8; NONCE_LEN] = u.arbitrary().unwrap_or([0; NONCE_LEN]);
    let kind = if u.arbitrary().unwrap_or(false) {
        Kind::Data
    } else {
        Kind::Presence
    };
    let project = u.arbitrary().unwrap_or(false);
    let Ok(secret) = RoomSecret::from_bytes(&key) else {
        return;
    };
    let keys = if project {
        secret.derive_project()
    } else {
        secret.derive()
    }
    .expect("derivation");

    // Arbitrary frames: only a panic is a bug.
    let frame_len = u.int_in_range(0..=u.len()).unwrap_or(0);
    let frame = u.bytes(frame_len).unwrap_or(&[]).to_vec();
    let _ = keys.open(&frame);
    // A frame with a valid header shape but garbage ciphertext.
    let mut shaped = vec![1u8, kind as u8];
    shaped.extend_from_slice(&nonce);
    shaped.extend_from_slice(&frame);
    let _ = keys.open(&shaped);

    // A genuine frame round-trips; tampering and the wrong key fail.
    let plaintext = u.bytes(u.len().min(256)).unwrap_or(&[]).to_vec();
    let sealed = keys
        .seal_with_nonce(kind, &plaintext, &nonce)
        .expect("seal");
    let (k, back) = keys.open(&sealed).expect("open own frame");
    assert_eq!(k, kind);
    assert_eq!(back, plaintext);
    if !sealed.is_empty() {
        let i = u.choose_index(sealed.len()).unwrap_or(0);
        let mut bad = sealed.clone();
        bad[i] ^= u.arbitrary::<u8>().unwrap_or(1) | 1;
        assert!(keys.open(&bad).is_err(), "tampered frame accepted");
    }
    if other != key
        && let Ok(s2) = RoomSecret::from_bytes(&other)
    {
        let k2 = s2.derive().expect("derive");
        let k2p = s2.derive_project().expect("derive");
        assert!(k2.open(&sealed).is_err());
        assert!(k2p.open(&sealed).is_err());
    }
    // Same secret, other room id (v1 vs project): the AAD binds the room.
    let twin = if project {
        secret.derive()
    } else {
        secret.derive_project()
    }
    .expect("derive");
    assert_ne!(twin.room_id(), keys.room_id());
    assert!(twin.open(&sealed).is_err(), "frame crossed rooms");
});
