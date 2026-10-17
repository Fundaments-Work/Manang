//! Hot paths: layered resolution of every setting, key-press resolution and a
//! comment-preserving TOML edit. The UI calls the first two on every
//! keystroke or settings render, so both must stay far below the 8 ms
//! keystroke budget (they measure in microseconds).

use std::collections::{BTreeMap, HashSet};
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_settings::keybindings::KeybindingSet;
use manang_settings::keys::{Platform, normalize};
use manang_settings::{Registry, layers, store};
use serde_json::json;

fn bench(c: &mut Criterion) {
    let registry = Registry::with_core();
    let user: BTreeMap<_, _> = [
        ("editor.font-size".to_owned(), json!(16)),
        ("appearance.theme".to_owned(), json!("light")),
    ]
    .into();
    let project: BTreeMap<_, _> = [("editor.tab-size".to_owned(), json!(4))].into();
    c.bench_function("resolve_all_core", |b| {
        b.iter(|| layers::resolve_all(black_box(&registry), black_box(&user), black_box(&project)));
    });

    let keys = KeybindingSet::with_core(Platform::Linux);
    let ctx: HashSet<String> = ["editorFocus".to_owned()].into();
    c.bench_function("resolve_keypress", |b| {
        b.iter(|| keys.resolve(black_box(&ctx), black_box("Ctrl+S")));
    });
    c.bench_function("normalize_shortcut", |b| {
        b.iter(|| normalize(black_box("shift+ctrl+p")));
    });

    let text =
        "# notes\n[editor]\nfont-size = 12 # px\ntab-size = 4\n[files]\nautosave = \"off\"\n";
    c.bench_function("toml_set_preserving_comments", |b| {
        b.iter(|| {
            let mut doc: toml_edit::DocumentMut = text.parse().unwrap_or_default();
            store::set_value(&mut doc, "editor.font-size", &json!(18)).ok();
            black_box(doc.to_string())
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
