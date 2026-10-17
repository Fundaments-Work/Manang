//! Latency of spell check and prose lint on a 10k-word document.
//! Budget (task SLA): spell check < 20 ms after warm-up.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_writing::format::{MarkdownOptions, markdown};
use manang_writing::lint::{LintConfig, lint};
use manang_writing::mask::prose;
use manang_writing::spell::{self, Dictionaries, UserDictionary};
use manang_writing::{Syntax, stats};

const SENTENCES: &[&str] = &[
    "The quick brown fox jumps over the lazy dog while the committee reviewed the proposal.",
    "Results were obtained by averaging over several independent runs of the experiment.",
    "We observe that the estimator is very stable under small perturbations of the input $x_i$.",
    "A wellknwon result states that $ sum_(i=1)^n i = (n(n+1))/2 $ holds for every positive integer.",
    "The measurements, which were taken at room temperature, agree with the theoretical prediction.",
    "See @smith2020 and #cite(<jones2019>) for a longer discussion of the underlying assumptions.",
    "This is is a deliberate mistake, and the the repetition is intentional for the benchmark.",
    "Consider the following lemma, whose proof is straightforward but somewhat tedious to write out.",
];

/// A Typst document of roughly `words` words.
fn document(words: usize) -> String {
    let mut text = String::from("#set page(paper: \"a4\")\n= Introduction\n");
    let mut count = 0;
    let mut i = 0;
    while count < words {
        let sentence = SENTENCES[i % SENTENCES.len()];
        text.push_str(sentence);
        text.push(' ');
        count += sentence.split_whitespace().count();
        i += 1;
        if i % 6 == 0 {
            text.push_str("\n\n== Section\n");
        }
    }
    text
}

fn bench(c: &mut Criterion) {
    let dir = std::env::temp_dir().join("manang-writing-bench");
    let dictionaries = Dictionaries::new(dir.join("dictionaries"));
    let user = UserDictionary::new(dir.join("user.txt"));
    let (_, dict) = dictionaries.get(None).expect("bundled dictionary loads");
    let doc = document(10_000);
    let masked = prose(&doc, Syntax::Typst, &[]);
    // Warm up caches and lazily initialised state.
    let _ = spell::check(&dict, &user, &masked, 5);

    c.bench_function("dictionary_load_en_us", |b| {
        b.iter(|| {
            let fresh = Dictionaries::new(dir.join("dictionaries"));
            black_box(fresh.get(None).expect("loads"));
        });
    });
    c.bench_function("mask_typst_10k_words", |b| {
        b.iter(|| black_box(prose(black_box(&doc), Syntax::Typst, &[])));
    });
    // The latency budget applies to checking; suggestions are fetched on
    // demand (`writing.spell.suggest`) or capped, and benchmarked apart.
    c.bench_function("spell_check_10k_words_masked", |b| {
        b.iter(|| black_box(spell::check(&dict, &user, black_box(&masked), 0).expect("check")));
    });
    c.bench_function("spell_check_10k_words_end_to_end", |b| {
        b.iter(|| {
            let masked = prose(black_box(&doc), Syntax::Typst, &[]);
            black_box(spell::check(&dict, &user, &masked, 0).expect("check"))
        });
    });
    c.bench_function("spell_check_10k_words_with_5_suggestions", |b| {
        b.iter(|| black_box(spell::check(&dict, &user, black_box(&masked), 5).expect("check")));
    });
    c.bench_function("spell_suggest_one_word", |b| {
        b.iter(|| black_box(spell::suggest(&dict, black_box("wellknwon"), 5)));
    });
    let config = LintConfig::default();
    c.bench_function("lint_prose_10k_words_masked", |b| {
        b.iter(|| black_box(lint(black_box(&masked), &config, None)));
    });
    c.bench_function("lint_prose_10k_words_end_to_end", |b| {
        b.iter(|| {
            let masked = prose(black_box(&doc), Syntax::Typst, &[]);
            black_box(lint(&masked, &config, None))
        });
    });
    c.bench_function("stats_10k_words", |b| {
        b.iter(|| black_box(stats::count(black_box(&doc), Syntax::Typst)));
    });
    let md = doc
        .replace("#set page(paper: \"a4\")\n", "")
        .replace("= ", "# ");
    c.bench_function("markdown_normalise_10k_words", |b| {
        b.iter(|| {
            black_box(markdown::format_document(
                black_box(&md),
                &MarkdownOptions::default(),
            ))
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
