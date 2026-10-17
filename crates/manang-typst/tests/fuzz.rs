//! Arbitrary source text must never panic the compiler or its diagnostics.

use std::sync::OnceLock;

use manang_typst::{CompileRequest, Compiler, CompilerConfig};
use proptest::prelude::*;

fn compiler() -> &'static Compiler {
    static C: OnceLock<Compiler> = OnceLock::new();
    C.get_or_init(|| {
        Compiler::new(CompilerConfig {
            threads: 2,
            ..CompilerConfig::default()
        })
        .expect("pool starts")
    })
}

fn compile(text: String) {
    let req = CompileRequest {
        main: "main.typ".to_owned(),
        root: Some(std::env::temp_dir().join("manang-typst-proptest-root")),
        sources: [("main.typ".to_owned(), text.clone())].into(),
    };
    let report = compiler()
        .compile(&req)
        .expect("a request never fails on content");
    for d in &report.diagnostics {
        if let Some(range) = d.range {
            assert!(range.start <= range.end);
            assert!(range.end <= text.chars().count(), "{d:?}");
        }
    }
    // The document, when there is one, can always be rendered.
    if report.ok && report.pages > 0 {
        compiler().render(0).expect("render");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn arbitrary_text_never_panics(text in any::<String>()) {
        compile(text);
    }

    #[test]
    fn markup_shaped_text_never_panics(
        text in proptest::collection::vec(
            prop_oneof![
                Just("#"), Just("="), Just("$"), Just("*"), Just("_"), Just("`"),
                Just("#let x = "), Just("("), Just(")"), Just("["), Just("]"),
                Just("{"), Just("}"), Just("\""), Just("\\"), Just("@"), Just("<"),
                Just("#import \"x\""), Just("#for i in range(3) "), Just("😀"),
                Just("é"), Just("\n"), Just(" "), Just("a"), Just("1"), Just(".")
            ],
            0..60,
        ).prop_map(|parts| parts.concat())
    ) {
        compile(text);
    }
}
