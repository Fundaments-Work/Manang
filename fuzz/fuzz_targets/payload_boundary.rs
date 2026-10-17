#![no_main]

//! The host/guest payload boundary: whatever a guest hands back, and whatever
//! the shell hands in, must be validated without panics, and accepted values
//! must respect the size caps and round-trip.

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use manang_kernel::Payload;
use manang_wasm_host::Limits;
use manang_wasm_host::boundary::{RawPayload, decode_output, encode_input};

fuzz_target!(|data: &[u8]| {
    let Some((&tag, rest)) = data.split_first() else {
        return;
    };
    // A small cap keeps the over-limit paths easy to reach.
    let limits = Limits {
        max_payload_bytes: 4096,
        max_output_bytes: 4096,
        ..Limits::default()
    };

    // Guest -> host.
    let raw = match tag % 3 {
        0 => RawPayload::Empty,
        1 => RawPayload::Json(String::from_utf8_lossy(rest).into_owned()),
        _ => RawPayload::Binary(rest.to_vec()),
    };
    if let Ok(payload) = decode_output(raw.clone(), &limits) {
        match (&raw, &payload) {
            (RawPayload::Empty, Payload::Empty) => {}
            (RawPayload::Binary(b), Payload::Binary(p)) => {
                assert_eq!(b.as_slice(), &p[..]);
                assert!(b.len() <= limits.max_output_bytes);
            }
            (RawPayload::Json(t), Payload::Json(_)) => {
                assert!(t.len() <= limits.max_output_bytes);
            }
            other => panic!("variant changed across the boundary: {other:?}"),
        }
        // Host -> guest -> host is the identity on accepted values, unless the
        // re-encoding crosses the input cap.
        if let Ok(lowered) = encode_input(&payload, &limits) {
            let again = decode_output(lowered, &limits).expect("re-lifting a lowered payload");
            assert_eq!(again, payload);
        }
    }

    // Host -> guest from arbitrary JSON and binary.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(rest) {
        let _ = encode_input(&Payload::Json(value), &limits);
    }
    let _ = encode_input(&Payload::Binary(Arc::from(rest)), &limits);
});
