//! What the PDF preview costs per edit (ADR 0006, 2026-10-18 amendment): an
//! incremental compile followed by `export_pdf` of the new document, for a
//! 10-page and a 50-page document. Budget: Typst incremental recompile
//! < 50 ms for documents under 50 pages (AGENTS.md section 4.2).

use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use manang_typst::{CompileRequest, Compiler, CompilerConfig};

/// About `pages` A4 pages: an outline, numbered headings with labels, prose,
/// math, internal and external links.
fn doc(pages: usize, tag: usize) -> String {
    let mut text = String::from(
        "#set page(paper: \"a4\")\n#set heading(numbering: \"1.\")\n\
         #set text(font: \"New Computer Modern\", size: 11pt)\n\
         #outline()\n",
    );
    for section in 0..pages {
        text.push_str(&format!(
            "= Section {section} <sec-{section}>\n\
             See #link(<sec-0>)[the first section] and \
             #link(\"https://typst.app\")[Typst]. $a^2 + b^2 = c^{section}$.\n\n\
             #lorem(600)\n\n"
        ));
    }
    text.push_str(&format!("Edit marker: {tag}.\n"));
    text
}

fn request(dir: &std::path::Path, pages: usize, tag: usize) -> CompileRequest {
    CompileRequest {
        main: "main.typ".to_owned(),
        root: Some(dir.to_path_buf()),
        sources: BTreeMap::from([("main.typ".to_owned(), doc(pages, tag))]),
    }
}

fn compiler() -> Compiler {
    Compiler::new(CompilerConfig::default()).expect("pool starts")
}

fn bench(c: &mut Criterion) {
    let dir = std::env::temp_dir().join("manang-typst-bench-pdf");
    for pages in [10, 50] {
        c.bench_function(&format!("pdf/{pages}_pages/edit_compile"), |b| {
            let compiler = compiler();
            compiler.compile(&request(&dir, pages, 0)).expect("warm up");
            let mut tag = 0;
            b.iter(|| {
                tag += 1;
                black_box(
                    compiler
                        .compile(&request(&dir, pages, tag))
                        .expect("compile"),
                )
            });
        });

        c.bench_function(&format!("pdf/{pages}_pages/export_after_edit"), |b| {
            let compiler = compiler();
            let mut tag = 0;
            b.iter_batched(
                || {
                    tag += 1;
                    compiler
                        .compile(&request(&dir, pages, tag))
                        .expect("compile")
                },
                |_| black_box(compiler.export_pdf().expect("export")),
                BatchSize::PerIteration,
            );
        });
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = bench
}
criterion_main!(benches);
