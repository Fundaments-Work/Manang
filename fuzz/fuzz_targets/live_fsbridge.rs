#![no_main]

//! `fsbridge::parse_changed_event` on arbitrary JSON (never panics, only
//! well-formed changes come back) and `text_edits` / `apply_edits` on two
//! arbitrary texts (round trip, sorted and non-overlapping edits).

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use manang_live_plugin::fsbridge::{apply_edits, parse_changed_event, text_edits};

fuzz_target!(|data: &[u8]| {
    // Event payloads: any JSON value.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
        let changes = parse_changed_event(&value);
        let listed = value
            .get("changes")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        assert!(changes.len() <= listed);
    }

    // Edit round trip. Two texts, built so they often share long runs
    // (the second is a mutation of the first) and sometimes are unrelated.
    let mut u = Unstructured::new(data);
    let old: String = u.arbitrary().unwrap_or_default();
    let new: String = if u.ratio(1, 4).unwrap_or(false) {
        u.arbitrary().unwrap_or_default()
    } else {
        let mut s = old.clone();
        for _ in 0..u.int_in_range(0..=4u8).unwrap_or(0) {
            let cut = u.arbitrary::<String>().unwrap_or_default();
            let at = u.int_in_range(0..=s.len()).unwrap_or(0);
            let at = (0..=at).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0);
            if u.arbitrary().unwrap_or(false) {
                s.insert_str(at, &cut);
            } else {
                let end = (at..=s.len())
                    .take(9)
                    .filter(|i| s.is_char_boundary(*i))
                    .last()
                    .unwrap_or(at);
                s.replace_range(at..end, "");
            }
        }
        s
    };
    let edits = text_edits(&old, &new);
    assert_eq!(edits.is_empty(), old == new);
    let mut at = 0usize;
    for e in &edits {
        assert!(e.start >= at && e.start <= e.end && e.end <= old.len());
        assert!(old.is_char_boundary(e.start) && old.is_char_boundary(e.end));
        at = e.end;
    }
    assert_eq!(
        apply_edits(&old, &edits).as_deref(),
        Some(new.as_str()),
        "apply_edits(old, text_edits(old, new)) != new"
    );
});
