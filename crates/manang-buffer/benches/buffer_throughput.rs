use criterion::{Criterion, black_box, criterion_group, criterion_main};
use manang_buffer::DocumentBuffer;

fn generate_5mb_document() -> String {
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

fn bench_buffer_throughput(c: &mut Criterion) {
    let doc_5mb = generate_5mb_document();
    assert!(doc_5mb.len() >= 5 * 1024 * 1024);

    let mut group = c.benchmark_group("buffer_throughput_5mb");

    group.bench_function("insert_middle_5mb", |b| {
        let mut buffer = DocumentBuffer::open(&doc_5mb);
        let mid = buffer.len_chars() / 2;
        b.iter(|| {
            buffer
                .insert(black_box(mid), black_box(" $E = mc^2$ "))
                .unwrap();
        });
    });

    group.bench_function("delete_middle_5mb", |b| {
        let mut buffer = DocumentBuffer::open(&doc_5mb);
        let mid = buffer.len_chars() / 2;
        b.iter(|| {
            // Delete and re-insert to keep buffer approximately 5MB
            let _ = buffer.delete(black_box(mid..mid + 10));
            let _ = buffer.insert(black_box(mid), black_box("0123456789"));
        });
    });

    group.bench_function("line_lookup_5mb", |b| {
        let buffer = DocumentBuffer::open(&doc_5mb);
        let total_chars = buffer.len_chars();
        let target_char = total_chars / 3 * 2;
        b.iter(|| {
            let (line, col) = buffer.char_to_line_col(black_box(target_char)).unwrap();
            let round_trip = buffer
                .line_col_to_char(black_box(line), black_box(col))
                .unwrap();
            black_box(round_trip);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_buffer_throughput);
criterion_main!(benches);
