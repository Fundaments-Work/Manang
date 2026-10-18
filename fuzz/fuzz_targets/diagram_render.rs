#![no_main]

//! Diagram sources are user text: every engine must check and render any
//! input without panicking, deterministically, with diagnostics inside the
//! source. Run with a cap: `cargo +nightly fuzz run diagram_render --
//! -max_total_time=60`.

use libfuzzer_sys::fuzz_target;
use manang_diagram::{Theme, plugin};

fuzz_target!(|data: &[u8]| {
    let Some((&tag, rest)) = data.split_first() else {
        return;
    };
    let Ok(source) = std::str::from_utf8(rest) else {
        return;
    };
    let language = ["mermaid", "pikchr", "svgbob"][usize::from(tag % 3)];
    let theme = Theme::ALL[usize::from(tag / 3 % 3)];
    let diagrams = plugin();

    if let Ok(diagnostics) = diagrams.check(language, source) {
        let lines = source.split('\n').count();
        for d in diagnostics {
            assert!(d.line >= 1 && d.line <= lines);
            assert!(d.column >= 1);
        }
    }
    let first = diagrams.render(language, source, theme);
    let second = diagrams.render(language, source, theme);
    assert_eq!(first, second);
});
