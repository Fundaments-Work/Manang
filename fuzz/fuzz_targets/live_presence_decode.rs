#![no_main]

//! `PresenceMessage::decode` (v1 and `StateV2`): never panics; accepted
//! values are within the documented clamps and decoding is idempotent.

use libfuzzer_sys::fuzz_target;
use manang_live::presence::{
    MAX_CURSOR, MAX_EMAIL, MAX_NAME, MAX_OPEN, PALETTE_SLOTS, Presence, PresenceMessage,
};

fn within_clamps(p: &Presence) {
    assert!(p.name.len() <= MAX_NAME);
    assert!(p.slot < PALETTE_SLOTS);
    if let Some(e) = &p.email {
        assert!(e.len() <= MAX_EMAIL);
    }
    if let Some(s) = &p.selection {
        assert!(s.anchor.len() <= MAX_CURSOR && s.head.len() <= MAX_CURSOR);
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(msg) = PresenceMessage::decode(data) else {
        return;
    };
    match &msg {
        // v1 `State` is passed through as is (the book clamps it on update);
        // only `StateV2` is clamped by `decode`.
        PresenceMessage::State(_) | PresenceMessage::Leave { .. } => {}
        PresenceMessage::StateV2(v2) => {
            within_clamps(&v2.presence);
            assert!(v2.open.len() <= MAX_OPEN);
            let mut seen = v2.open.clone();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), v2.open.len(), "open list has repeats");
        }
    }
    let encoded = msg.encode().expect("an accepted message encodes");
    assert_eq!(
        PresenceMessage::decode(&encoded).expect("re-decodes"),
        msg,
        "decode is not idempotent"
    );
});
