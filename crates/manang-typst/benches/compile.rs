//! Cold and incremental compile latency. Budget: incremental < 50 ms for a
//! small document (AGENTS.md section 4.2).

use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use manang_typst::{CompileRequest, Compiler, CompilerConfig};

fn doc(tag: usize) -> String {
    let mut text = String::from(
        "#set page(paper: \"a4\")\n#set heading(numbering: \"1.\")\n\
         #set text(font: \"New Computer Modern\", size: 11pt)\n= Introduction\n",
    );
    for section in 0..6 {
        text.push_str(&format!("== Section {section}\n"));
        for para in 0..4 {
            text.push_str(&format!(
                "Paragraph {para} of section {section} with math $a^2 + b^2 = c^2$ and a list:\n\
                 - first item\n- second item\n\n"
            ));
        }
    }
    text.push_str(&format!("Edit marker: {tag}.\n"));
    text
}

fn request(dir: &std::path::Path, tag: usize) -> CompileRequest {
    CompileRequest {
        main: "main.typ".to_owned(),
        root: Some(dir.to_path_buf()),
        sources: BTreeMap::from([("main.typ".to_owned(), doc(tag))]),
    }
}

fn compiler() -> Compiler {
    Compiler::new(CompilerConfig::default()).expect("pool starts")
}

fn bench(c: &mut Criterion) {
    let dir = std::env::temp_dir().join("manang-typst-bench");

    c.bench_function("compile/cold", |b| {
        b.iter_batched(
            || {
                // A fresh session and empty memo tables: nothing to reuse.
                typst::comemo::evict(0);
                (compiler(), request(&dir, 0))
            },
            |(compiler, req)| black_box(compiler.compile(&req).expect("compile")),
            BatchSize::PerIteration,
        );
    });

    c.bench_function("compile/incremental_edit", |b| {
        let compiler = compiler();
        compiler.compile(&request(&dir, 0)).expect("warm up");
        let mut tag = 0;
        b.iter(|| {
            tag += 1;
            black_box(compiler.compile(&request(&dir, tag)).expect("compile"))
        });
    });

    c.bench_function("compile/unchanged", |b| {
        let compiler = compiler();
        let req = request(&dir, 0);
        compiler.compile(&req).expect("warm up");
        b.iter(|| black_box(compiler.compile(&req).expect("compile")));
    });

    c.bench_function("render/svg_page_uncached", |b| {
        let compiler = compiler();
        b.iter_batched(
            || compiler.compile(&request(&dir, 0)).expect("compile"),
            |_| black_box(compiler.render(0).expect("render")),
            BatchSize::PerIteration,
        );
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
