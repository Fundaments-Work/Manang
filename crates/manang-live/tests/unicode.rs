//! UTF-8 / UTF-16 / char index mapping through the rope bridge.
//!
//! ropey indexes scalars, CodeMirror indexes UTF-16 units, the engines count
//! in one or the other. These tests drive random edits over text full of
//! astral characters (emoji, mathematical alphanumerics), ZWJ sequences,
//! combining marks and CJK, and check three independent views stay equal: the
//! rope, the CRDT, and a `Vec<u16>` that applies the UTF-16 change lists
//! exactly as CodeMirror would.

#[cfg(feature = "yrs-engine")]
use manang_live::YrsEngine;
use manang_live::bridge::{LiveDoc, TextTarget};
use manang_live::{LoroEngine, TextCrdt};
use proptest::prelude::*;
use ropey::Rope;

const SNIPPETS: &[&str] = &[
    "a",
    "é",
    "e\u{301}", // e + combining acute
    "😀",
    "👩‍👩‍👧", // ZWJ family
    "🇳🇵", // regional indicator pair
    "日本語",
    "𝔘𝔫𝔦",
    "\n",
    "\r\n",
    "\u{10FFFF}",
    "x",
];

fn apply_u16(doc: &mut Vec<u16>, start: usize, delete: usize, insert: &str) {
    doc.splice(start..start + delete, insert.encode_utf16());
}

fn run<E: TextCrdt>(edits: &[(u16, u8, u8)], seed_idx: u8) -> Result<(), TestCaseError> {
    let seed = SNIPPETS[usize::from(seed_idx) % SNIPPETS.len()].repeat(8);
    let mut ra = Rope::from_str(&seed);
    let mut rb = Rope::from_str(&seed);
    let mut a = LiveDoc::<E>::seeded(10, &seed).map_err(|e| TestCaseError::fail(e.to_string()))?;
    let mut b = LiveDoc::<E>::seeded(11, &seed).map_err(|e| TestCaseError::fail(e.to_string()))?;
    // What "the web view" of b holds: UTF-16 units updated only from change lists.
    let mut view_b: Vec<u16> = seed.encode_utf16().collect();

    for (i, (at, del, snip)) in edits.iter().enumerate() {
        let text = SNIPPETS[usize::from(*snip) % SNIPPETS.len()];
        let len = ra.len_chars();
        let from = len * usize::from(*at) / 1000;
        let to = (from + usize::from(*del)).min(len);
        if i % 2 == 0 {
            // character-range route (editor plugin)
            a.local_replace_chars(&mut ra, from, to, text)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
        } else {
            // UTF-16 route (CodeMirror), boundaries taken from the rope
            let u_from = ra.char_to_utf16(from);
            let u_to = ra.char_to_utf16(to);
            a.local_replace_utf16(&mut ra, u_from, u_to, text)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
        prop_assert_eq!(a.engine().text(), ra.to_string());
        prop_assert_eq!(a.engine().len_bytes(), ra.len_bytes());
        for update in a
            .take_local_updates()
            .map_err(|e| TestCaseError::fail(e.to_string()))?
        {
            let applied = b
                .apply_remote(&mut rb, &update)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            for c in &applied.changes {
                // the two unit systems must describe the same edit
                prop_assert!(c.utf16_start >= c.char_start);
                apply_u16(&mut view_b, c.utf16_start, c.utf16_delete, &c.insert);
            }
        }
        prop_assert_eq!(rb.to_string(), ra.to_string());
        prop_assert_eq!(String::from_utf16_lossy(&view_b), ra.to_string());
        prop_assert_eq!(view_b.len(), rb.len_utf16_cu());
        prop_assert_eq!(b.engine().text(), ra.to_string());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, .. ProptestConfig::default() })]

    #[test]
    fn index_mapping_is_consistent_loro(
        edits in proptest::collection::vec((0u16..=1000, 0u8..8, any::<u8>()), 1..60),
        seed in any::<u8>(),
    ) {
        run::<LoroEngine>(&edits, seed)?;
    }

    #[cfg(feature = "yrs-engine")]
    #[test]
    fn index_mapping_is_consistent_yrs(
        edits in proptest::collection::vec((0u16..=1000, 0u8..8, any::<u8>()), 1..60),
        seed in any::<u8>(),
    ) {
        run::<YrsEngine>(&edits, seed)?;
    }
}

#[test]
fn rope_primitives_agree_on_known_text() {
    // "a😀é日" = a(1) 😀(2) é(1) 日(1) UTF-16 units; 4 chars; 12 UTF-8 bytes? no: 1+4+2+3
    let r = Rope::from_str("a😀é日");
    assert_eq!(r.len_chars(), 4);
    assert_eq!(r.len_utf16_cu(), 5);
    assert_eq!(r.len_bytes(), 10);
    assert_eq!(r.char_to_utf16(2), 3);
    assert_eq!(r.utf16_to_char(3), 2);
    assert_eq!(r.utf16_to_char(2), 1); // inside the pair rounds down; the bridge rejects it
}

#[test]
fn edits_inside_a_surrogate_pair_are_refused_not_corrupting() {
    let mut r = Rope::from_str("a😀b");
    let mut d = LiveDoc::<LoroEngine>::seeded(3, "a😀b").expect("doc");
    assert!(d.local_replace_utf16(&mut r, 2, 3, "").is_err());
    assert_eq!(r.to_string(), "a😀b");
    assert_eq!(d.engine().text(), "a😀b");
}
