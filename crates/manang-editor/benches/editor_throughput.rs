use criterion::{Criterion, black_box, criterion_group, criterion_main};
use manang_editor::Editor;

fn generate_5mb_text() -> String {
    let paragraph = "In scientific authoring, high-performance interactive text buffers require sub-millisecond \
latency. When typing in Typst or LaTeX documents, keystrokes must update the document representation \
without inducing garbage-collection pauses or redundant allocations.\n\
Under unitary coupling to a heat reservoir at inverse temperature beta, the irreversible entropy \
production rate satisfies the lower bound Delta S_irr >= integral_0^t (delta Q) / T.\n";

    let mut doc = String::with_capacity(5 * 1024 * 1024);
    while doc.len() < 5 * 1024 * 1024 {
        doc.push_str(paragraph);
    }
    doc
}

fn bench_editor_throughput(c: &mut Criterion) {
    let text_5mb = generate_5mb_text();
    assert!(text_5mb.len() >= 5 * 1024 * 1024);

    let mut group = c.benchmark_group("editor_throughput_5mb");

    group.bench_function("insert_middle_5mb", |b| {
        let editor = Editor::new();
        let id = editor.create();
        editor.insert(Some(id), 0, &text_5mb).expect("seed 5mb");
        let mid = editor.info(Some(id)).unwrap().chars / 2;

        b.iter(|| {
            editor
                .insert(Some(id), black_box(mid), black_box(" $E = mc^2$ "))
                .unwrap();
        });
    });

    group.bench_function("delete_middle_5mb", |b| {
        let editor = Editor::new();
        let id = editor.create();
        editor.insert(Some(id), 0, &text_5mb).expect("seed 5mb");
        let mid = editor.info(Some(id)).unwrap().chars / 2;

        b.iter(|| {
            editor
                .delete(Some(id), black_box(mid), black_box(mid + 10))
                .unwrap();
            editor
                .insert(Some(id), black_box(mid), black_box("0123456789"))
                .unwrap();
        });
    });

    group.bench_function("undo_middle_5mb", |b| {
        let editor = Editor::new();
        let id = editor.create();
        editor.insert(Some(id), 0, &text_5mb).expect("seed 5mb");
        let mid = editor.info(Some(id)).unwrap().chars / 2;

        b.iter(|| {
            editor
                .insert(Some(id), black_box(mid), black_box("edit to undo"))
                .unwrap();
            editor.undo(Some(id)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_editor_throughput);
criterion_main!(benches);
