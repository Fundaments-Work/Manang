//! Rope-bridge latency on a 1 MB document: the budget for the whole keystroke
//! path is 8 ms, so the bridge must stay far below 1 ms.
//!
//! `cargo bench -p manang-live`. The p99 table comes from
//! `cargo run --release -p manang-live --features yrs-engine --example measure`.

use criterion::{Criterion, criterion_group, criterion_main};
use manang_live::LoroEngine;
use manang_live::TextCrdt;
#[cfg(feature = "yrs-engine")]
use manang_live::YrsEngine;
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{Kind, RoomSecret};
use manang_live::sim::Rng;
use ropey::Rope;

fn document() -> String {
    let words = ["quantum", "of", "the", "résumé", "日本語", "😀", "proof"];
    let mut rng = Rng(1);
    let mut s = String::with_capacity(1 << 20);
    while s.len() < 1 << 20 {
        s.push_str(words[rng.below(words.len() as u64) as usize]);
        s.push(if rng.below(12) == 0 { '\n' } else { ' ' });
    }
    s
}

fn bench_engine<E: TextCrdt>(c: &mut Criterion) {
    let text = document();
    let name = E::name();
    let keys = RoomSecret::generate(&mut Rng(3))
        .and_then(|s| s.derive())
        .expect("keys");

    let mut rope = Rope::from_str(&text);
    let mut doc = LiveDoc::<E>::seeded(10, &text).expect("seed");
    let mut caret = rope.len_chars() / 2;
    c.bench_function(&format!("{name}/local keystroke (rope+crdt+update)"), |b| {
        b.iter(|| {
            doc.local_replace_chars(&mut rope, caret, caret, "a")
                .expect("edit");
            caret += 1;
            doc.take_local_updates().expect("updates")
        });
    });

    let mut ent = Rng(4);
    c.bench_function(&format!("{name}/local keystroke + AEAD seal"), |b| {
        b.iter(|| {
            doc.local_replace_chars(&mut rope, caret, caret, "a")
                .expect("edit");
            caret += 1;
            let updates = doc.take_local_updates().expect("updates");
            keys.seal(Kind::Data, &updates[0], &mut ent).expect("seal")
        });
    });

    // Remote path: a peer produces updates, this replica applies them.
    let mut rope_a = Rope::from_str(&text);
    let mut a = LiveDoc::<E>::seeded(20, &text).expect("seed");
    let mut rope_b = Rope::from_str(&text);
    let mut b_doc = LiveDoc::<E>::seeded(21, &text).expect("seed");
    let mut caret_a = rope_a.len_chars() / 2;
    c.bench_function(&format!("{name}/remote update (import+rope patch)"), |b| {
        b.iter_batched(
            || {
                a.local_replace_chars(&mut rope_a, caret_a, caret_a, "z")
                    .expect("edit");
                caret_a += 1;
                a.take_local_updates().expect("updates").remove(0)
            },
            |update| b_doc.apply_remote(&mut rope_b, &update).expect("apply"),
            criterion::BatchSize::PerIteration,
        );
    });
}

fn benches(c: &mut Criterion) {
    bench_engine::<LoroEngine>(c);
    #[cfg(feature = "yrs-engine")]
    bench_engine::<YrsEngine>(c);
}

criterion_group!(group, benches);
criterion_main!(group);
