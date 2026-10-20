#![no_main]

//! `TextCrdt::apply_update` and `from_snapshot` (Loro and yrs) on hostile
//! bytes, including mutated genuine updates and Loro snapshots with a repaired
//! checksum. Never panics; a genuine update applies and converges.

#[path = "common.rs"]
mod common;

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use manang_live::{LoroEngine, TextCrdt, YrsEngine};

/// `hostile_apply`: whether hostile bytes may reach `apply_update` /
/// `from_snapshot`. Not for yrs, whose integrator can abort the process (see
/// the note on `guarded` in `yrs_engine.rs`); yrs still gets hostile version
/// vectors and cursors, which it decodes behind a guard.
fn run<E: TextCrdt>(mode: u8, data: &[u8], hostile_apply: bool) {
    let mut u = Unstructured::new(data);
    match mode % 3 {
        0 => {
            let mut bytes = data.to_vec();
            common::fix_loro_checksum(&mut bytes);
            if let Ok(mut e) = E::new(10) {
                let _ = e.export_since(&bytes);
                let _ = e.cursor_pos(&bytes);
                if !hostile_apply {
                    return;
                }
                let _ = e.apply_update(&bytes);
                // The replica must stay usable after a refused update.
                let _ = e.text();
                let _ = e.insert(0, "x");
                let _ = e.take_local_updates();
            }
        }
        1 => {
            let mut bytes = data.to_vec();
            common::fix_loro_checksum(&mut bytes);
            if !hostile_apply {
                return;
            }
            if let Ok(e) = E::from_snapshot(11, &bytes) {
                let _ = e.text();
                let _ = e.export_snapshot();
                let _ = e.version();
            }
        }
        _ => {
            let (Ok(mut a), Ok(mut b)) = (E::new(20), E::new(21)) else {
                return;
            };
            let mut sent: Vec<Vec<u8>> = Vec::new();
            for _ in 0..u.int_in_range(0..=12u8).unwrap_or(0) {
                let text = a.text();
                let ins = common::small_text(&mut u);
                // Char-boundary offsets only.
                let boundaries: Vec<usize> = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain([text.len()])
                    .collect();
                let at = boundaries[u.choose_index(boundaries.len()).unwrap_or(0)];
                if u.ratio(1, 3).unwrap_or(false) && at < text.len() {
                    let next = text[at..].chars().next().map_or(0, char::len_utf8);
                    let _ = a.delete(at, next);
                } else {
                    let _ = a.insert(at, &ins);
                }
                if let Ok(mut ups) = a.take_local_updates() {
                    sent.append(&mut ups);
                }
            }
            // Genuine updates converge.
            for up in &sent {
                b.apply_update(up).expect("a genuine update applies");
            }
            assert_eq!(a.text(), b.text(), "replicas diverged");
            // Mutated genuine updates and snapshots: no panic.
            let mut blobs = sent;
            if let Ok(s) = a.export_snapshot() {
                blobs.push(s);
            }
            if let Ok(s) = a.export_trimmed() {
                blobs.push(s);
            }
            for blob in blobs {
                let bad = common::mutate(&mut u, blob);
                let _ = a.export_since(&bad);
                let _ = a.cursor_pos(&bad);
                if !hostile_apply {
                    continue;
                }
                if let Ok(mut c) = E::new(22) {
                    let _ = c.apply_update(&bad);
                    let _ = c.text();
                }
                if let Ok(c) = E::from_snapshot(23, &bad) {
                    let _ = c.text();
                }
                let _ = b.apply_update(&bad);
                let _ = b.text();
            }
        }
    }
}

fuzz_target!(init: common::tolerate_yrs_decoder_panics(), |data: &[u8]| {
    let Some((&mode, rest)) = data.split_first() else {
        return;
    };
    run::<LoroEngine>(mode, rest, true);
    run::<YrsEngine>(mode, rest, false);
});
