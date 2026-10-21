//! Measurements behind the engine choice. Run with
//! `cargo run --release -p manang-live --features yrs-engine --example measure`.
//!
//! Prints Markdown tables. Memory is live heap bytes from a counting global
//! allocator, so it includes everything the engine retains and nothing else.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[cfg(feature = "yrs-engine")]
use manang_live::YrsEngine;
use manang_live::bridge::{LiveDoc, TextTarget};
use manang_live::envelope::{Kind, RoomSecret};
use manang_live::sim::Rng;
use manang_live::{LoroEngine, TextCrdt};
use ropey::Rope;

struct Counting;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator unchanged; only counts bytes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same contract as `System.alloc`.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: same contract as `System.dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: same contract as `System.realloc`.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                let now = CURRENT.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size
                    - layout.size();
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static A: Counting = Counting;

fn heap() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

const WORDS: &[&str] = &[
    "the",
    "quantum",
    "of",
    "we",
    "show",
    "that",
    "\\cite{x}",
    "equation",
    "result",
    "method",
    "analysis",
    "figure",
    "data",
    "model",
    "résumé",
    "naïve",
    "proof",
    "lemma",
    "#set",
    "text",
];

fn make_text(bytes: usize, rng: &mut Rng) -> String {
    let mut s = String::with_capacity(bytes + 16);
    while s.len() < bytes {
        let w = WORDS[rng.below(WORDS.len() as u64) as usize];
        s.push_str(w);
        match rng.below(40) {
            0 => s.push_str(" 日本語 "),
            1 => s.push_str(" 😀 "),
            2..=4 => s.push('\n'),
            _ => s.push(' '),
        }
    }
    s
}

fn rand_chars(rng: &mut Rng, n: usize) -> String {
    (0..n)
        .map(|_| match rng.below(60) {
            0 => '😀',
            1 => 'é',
            2 => '日',
            3 => '\n',
            k => char::from(b'a' + (k % 26) as u8),
        })
        .collect()
}

fn rand_n(rng: &mut Rng, max: u64) -> String {
    let n = 1 + rng.below(max) as usize;
    rand_chars(rng, n)
}

#[derive(Clone)]
struct Edit {
    start: usize,
    del_len: usize,
    text: String,
}

/// Random edits valid against a rope that tracks the evolving text.
fn gen_edits(rope: &mut Rope, n: usize, rng: &mut Rng) -> Vec<Edit> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let len = rope.len_chars();
        let from = rng.below(len as u64 + 1) as usize;
        let (del, text) = match rng.below(10) {
            0..=6 => (0, rand_n(rng, 8)),
            7..=8 => (((1 + rng.below(8)) as usize).min(len - from), String::new()),
            _ => (
                ((1 + rng.below(4)) as usize).min(len - from),
                rand_n(rng, 4),
            ),
        };
        let b_from = rope.char_to_byte(from);
        let b_to = rope.char_to_byte(from + del);
        rope.replace_chars(from, from + del, &text);
        out.push(Edit {
            start: b_from,
            del_len: b_to - b_from,
            text,
        });
    }
    out
}

fn apply_local<E: TextCrdt>(e: &mut E, edit: &Edit) {
    if edit.del_len > 0 {
        e.delete(edit.start, edit.del_len).ok();
    }
    if !edit.text.is_empty() {
        e.insert(edit.start, &edit.text).ok();
    }
}

fn kib(b: usize) -> String {
    format!("{:.1} KiB", b as f64 / 1024.0)
}

fn pct(sorted: &[u64], p: f64) -> u64 {
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn section_a_b_c<E: TextCrdt>() {
    let mut rng = Rng(42);
    let text = make_text(200 * 1024, &mut rng);
    let mut rope = Rope::from_str(&text);
    let edits = gen_edits(&mut rope, 10_000, &mut Rng(7));

    // (a) 10,000 local edits on a 200 KB document
    let base = heap();
    let mut eng = E::seeded(10, &text).expect("seed");
    let after_seed = heap() - base;
    let mut updates: Vec<Vec<u8>> = Vec::with_capacity(edits.len());
    PEAK.store(heap(), Ordering::Relaxed);
    let t = Instant::now();
    for e in &edits {
        apply_local(&mut eng, e);
        updates.extend(eng.take_local_updates().expect("updates"));
    }
    let local = t.elapsed();
    let upd_bytes: usize = updates.iter().map(Vec::len).sum();
    let upd_mem: usize = updates.iter().map(|u| u.capacity() + 24).sum();
    let after_edits = heap() - base - upd_mem;
    let peak = PEAK.load(Ordering::Relaxed) - base;
    assert_eq!(eng.text(), rope.to_string());
    println!(
        "| {} | a: 10k local edits | {:.1} ms total, {:.1} us/edit | engine heap after seed {} -> after edits {} (peak incl. update blobs {}) |",
        E::name(),
        local.as_secs_f64() * 1e3,
        local.as_secs_f64() * 1e6 / edits.len() as f64,
        kib(after_seed),
        kib(after_edits),
        kib(peak)
    );

    // (b) apply those updates as remote updates on a second replica
    let base = heap();
    let mut remote = E::seeded(11, &text).expect("seed");
    let t = Instant::now();
    let mut changes = 0usize;
    for u in &updates {
        changes += remote.apply_update(u).expect("apply").changes.len();
    }
    let rem = t.elapsed();
    let rem_heap = heap() - base;
    assert_eq!(remote.text(), rope.to_string());
    println!(
        "| {} | b: apply 10k remote updates | {:.1} ms total, {:.1} us/update ({} change records) | replica heap {} |",
        E::name(),
        rem.as_secs_f64() * 1e3,
        rem.as_secs_f64() * 1e6 / updates.len() as f64,
        changes,
        kib(rem_heap)
    );

    // (c) sizes
    let snap = eng.export_snapshot().expect("snap");
    let trimmed = eng.export_trimmed().expect("trim");
    let fresh = E::seeded(10, &text).expect("seed");
    let seed_snap = fresh.export_snapshot().expect("snap");
    println!(
        "| {} | c: sizes | text {} ; seed state {} ; full state after 10k edits {} ; trimmed {} ; incremental updates: mean {:.0} B, max {} B, total {} | restore-from-trimmed ok: {} |",
        E::name(),
        kib(text.len()),
        kib(seed_snap.len()),
        kib(snap.len()),
        kib(trimmed.len()),
        upd_bytes as f64 / updates.len() as f64,
        updates.iter().map(Vec::len).max().unwrap_or(0),
        kib(upd_bytes),
        E::from_snapshot(12, &trimmed)
            .map(|e| e.text() == eng.text())
            .unwrap_or(false)
    );
    let base = heap();
    let restored = E::from_snapshot(12, &trimmed).expect("restore");
    let trimmed_heap = heap() - base;
    let base = heap();
    let restored_full = E::from_snapshot(12, &snap).expect("restore");
    let full_heap = heap() - base;
    println!(
        "| {} | c: heap after restore | from full snapshot {} ; from trimmed {} | |",
        E::name(),
        kib(full_heap),
        kib(trimmed_heap)
    );
    drop((restored, restored_full));
}

fn section_d<E: TextCrdt>() {
    let mut rng = Rng(99);
    let text = make_text(1024 * 1024, &mut rng);
    let mut rope = Rope::from_str(&text);
    let mut doc = LiveDoc::<E>::seeded(10, &text).expect("seed");
    let mut peer = LiveDoc::<E>::seeded(11, &text).expect("seed");
    let mut peer_rope = Rope::from_str(&text);
    let keys = RoomSecret::generate(&mut Rng(5))
        .and_then(|s| s.derive())
        .expect("keys");
    let mut ent = Rng(6);

    // typing: a caret that mostly advances, sometimes jumps
    let n = 20_000usize;
    let mut caret = rope.len_chars() / 2;
    let mut bridge_ns = Vec::with_capacity(n);
    let mut full_ns = Vec::with_capacity(n);
    let mut recv_ns = Vec::with_capacity(n);
    let mut recv_full_ns = Vec::with_capacity(n);
    let chars = ['a', 'b', 'e', ' ', 'é', '😀', 'x', '\n'];
    for i in 0..n {
        if rng.below(50) == 0 {
            caret = rng.below(rope.len_chars() as u64) as usize;
        }
        let ch = chars[i % chars.len()].to_string();
        let backspace = rng.below(8) == 0 && caret > 0;
        let t = Instant::now();
        if backspace {
            doc.local_replace_chars(&mut rope, caret - 1, caret, "")
                .expect("edit");
            caret -= 1;
        } else {
            doc.local_replace_chars(&mut rope, caret, caret, &ch)
                .expect("edit");
            caret += 1;
        }
        let updates = doc.take_local_updates().expect("updates");
        let bridge = t.elapsed();
        let mut frames = Vec::new();
        for u in &updates {
            frames.push(keys.seal(Kind::Data, u, &mut ent).expect("seal"));
        }
        let full = t.elapsed();
        bridge_ns.push(bridge.as_nanos() as u64);
        full_ns.push(full.as_nanos() as u64);
        // receive side on the peer: decrypt, import, patch the peer rope
        for f in &frames {
            let t = Instant::now();
            let (_, plain) = keys.open(f).expect("open");
            let dec = t.elapsed();
            let t2 = Instant::now();
            peer.apply_remote(&mut peer_rope, &plain).expect("apply");
            let imp = t2.elapsed();
            recv_ns.push(imp.as_nanos() as u64);
            recv_full_ns.push((dec + imp).as_nanos() as u64);
        }
    }
    assert_eq!(rope.to_string(), peer_rope.to_string());
    for v in [
        &mut bridge_ns,
        &mut full_ns,
        &mut recv_ns,
        &mut recv_full_ns,
    ] {
        v.sort_unstable();
    }
    let row = |label: &str, v: &[u64]| {
        println!(
            "| {} | d: {} (1 MB doc, {} keystrokes) | p50 {:.1} us, p99 {:.1} us, p99.9 {:.1} us, max {:.1} us | |",
            E::name(),
            label,
            v.len(),
            pct(v, 0.5) as f64 / 1e3,
            pct(v, 0.99) as f64 / 1e3,
            pct(v, 0.999) as f64 / 1e3,
            *v.last().unwrap_or(&0) as f64 / 1e3
        );
    };
    row("local keystroke: rope + CRDT + update bytes", &bridge_ns);
    row("local keystroke + AEAD seal", &full_ns);
    row("remote update: CRDT import + rope patch", &recv_ns);
    row("remote update: AEAD open + import + patch", &recv_full_ns);
}

fn section_g<E: TextCrdt>(burst: usize) {
    // 2 hours at 8 edits/s across 3 collaborators = 57,600 edits.
    const TOTAL: usize = 57_600;
    const CHECK: usize = TOTAL / 8; // every 15 minutes
    let mut rng = Rng(1234);
    let text = make_text(50 * 1024, &mut rng);
    let mut peers: Vec<E> = (0..3)
        .map(|i| E::seeded(20 + i as u64, &text).expect("seed"))
        .collect();
    let mut rope = Rope::from_str(&text);
    let mut carets = [
        rope.len_chars() / 4,
        rope.len_chars() / 2,
        3 * rope.len_chars() / 4,
    ];
    // Pass 1: generate the update stream.
    let mut stream: Vec<Vec<u8>> = Vec::with_capacity(TOTAL);
    for i in 0..TOTAL {
        let who = if burst == 1 {
            rng.below(3) as usize
        } else {
            (i / burst) % 3
        };
        let len = rope.len_chars();
        let caret = carets[who].min(len);
        // collaborators type near their own caret, occasionally delete, rarely jump
        let (del, ins): (usize, String) = match rng.below(20) {
            0..=14 => (0, rand_chars(&mut rng, 1)),
            15..=18 => (usize::from(caret > 0), String::new()),
            _ => (0, " ".to_owned()),
        };
        let from = caret - del;
        let b_from = rope.char_to_byte(from);
        let b_to = rope.char_to_byte(caret);
        rope.replace_chars(from, caret, &ins);
        let edit = Edit {
            start: b_from,
            del_len: b_to - b_from,
            text: ins.clone(),
        };
        // the author applies locally; the others receive the update
        apply_local(&mut peers[who], &edit);
        let ups = peers[who].take_local_updates().expect("updates");
        for u in &ups {
            for (k, p) in peers.iter_mut().enumerate() {
                if k != who {
                    p.apply_update(u).expect("apply");
                }
            }
        }
        stream.extend(ups);
        let new_caret = from + ins.chars().count();
        for (k, c) in carets.iter_mut().enumerate() {
            if k == who {
                *c = if rng.below(200) == 0 {
                    rng.below(rope.len_chars() as u64 + 1) as usize
                } else {
                    new_caret
                };
            } else if *c > from {
                *c = (*c + ins.chars().count())
                    .saturating_sub(del)
                    .min(rope.len_chars());
            }
        }
    }
    assert_eq!(peers[0].text(), rope.to_string());
    drop(peers);
    // Pass 2: one fresh replica receives the stream; sample memory.
    let base = heap();
    let mut r = E::seeded(99, &text).expect("seed");
    let mut line = Vec::new();
    for (i, u) in stream.iter().enumerate() {
        r.apply_update(u).expect("apply");
        if (i + 1) % CHECK == 0 {
            let before_compact = heap() - base;
            r.compact_memory();
            let after_compact = heap() - base;
            let snap = r.export_snapshot().expect("snap").len();
            let trim = r.export_trimmed().expect("trim").len();
            line.push(format!(
                "{} min: heap {} (after compact_memory {}) / full {} / trimmed {}",
                (i + 1) / CHECK * 15,
                kib(before_compact),
                kib(after_compact),
                kib(snap),
                kib(trim)
            ));
        }
    }
    println!(
        "| {} | g: 2 h session, 57.6k edits, 3 peers, author changes every {} edit(s), doc ends {} | {} | |",
        E::name(),
        burst,
        kib(rope.len_bytes()),
        line.join(" ; ")
    );
    // compaction: reload from the trimmed snapshot and compare heap
    let trimmed = r.export_trimmed().expect("trim");
    drop(r);
    let b2 = heap();
    let compact = E::from_snapshot(98, &trimmed).expect("restore");
    println!(
        "| {} | g: heap after compaction (reload from trimmed snapshot) | {} | |",
        E::name(),
        kib(heap() - b2)
    );
    drop(compact);
}

fn main() {
    println!("| engine | measurement | result | notes |");
    println!("| --- | --- | --- | --- |");
    section_a_b_c::<LoroEngine>();
    #[cfg(feature = "yrs-engine")]
    section_a_b_c::<YrsEngine>();
    section_d::<LoroEngine>();
    #[cfg(feature = "yrs-engine")]
    section_d::<YrsEngine>();
    for burst in [1, 40] {
        section_g::<LoroEngine>(burst);
        #[cfg(feature = "yrs-engine")]
        section_g::<YrsEngine>(burst);
    }
}
