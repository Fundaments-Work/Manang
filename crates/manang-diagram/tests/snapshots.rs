//! SVG snapshots: one fixture per diagram family, in the `dark` and `print`
//! themes. Snapshots hold the exact bytes; nothing is normalised, so any
//! nondeterminism shows up as a diff.

mod fixtures;

use fixtures::FIXTURES;
use std::borrow::Cow;

use manang_diagram::{Theme, color, plugin};

#[test]
fn every_family_renders_and_matches_its_snapshot() {
    let diagrams = plugin();
    let mut failures = Vec::new();
    for &(name, language, source) in FIXTURES {
        for theme in [Theme::Dark, Theme::Print] {
            match diagrams.render(language, source, theme) {
                Ok(svg) => {
                    let again = diagrams.render(language, source, theme);
                    assert_eq!(
                        again.as_ref(),
                        Ok(&svg),
                        "{name}/{theme:?} is not deterministic"
                    );
                    insta::assert_snapshot!(format!("{name}_{}", theme.id()), svg);
                }
                Err(e) => failures.push(format!("{name}/{}: {e}", theme.id())),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "failed to render:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_fixture_checks_clean() {
    let diagrams = plugin();
    for &(name, language, source) in FIXTURES {
        let diagnostics = diagrams.check(language, source).expect("known language");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.severity != manang_diagram::Severity::Error),
            "{name}: {diagnostics:?}"
        );
    }
}

#[test]
fn design_rules_hold_in_every_snapshot() {
    let diagrams = plugin();
    for &(name, language, source) in FIXTURES {
        for theme in Theme::ALL {
            let Ok(svg) = diagrams.render(language, source, theme) else {
                continue;
            };
            assert!(!svg.contains("<foreignObject"), "{name}: HTML labels");
            assert!(!svg.contains("var(--"), "{name}: CSS variables");
            assert!(!svg.contains("<filter"), "{name}: filters (shadows)");
            // No fixture label spells a colour, so any colour the clamp
            // would still change is a colour outside the design tokens.
            let clamped = color::clamp(&svg, theme.palette(), color::Invert::All);
            if let Cow::Owned(changed) = clamped {
                let at = svg
                    .bytes()
                    .zip(changed.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(0);
                panic!(
                    "{name}/{theme:?}: colour outside the palette near `{}`",
                    &svg[at.saturating_sub(60)..(at + 20).min(svg.len())]
                );
            }
            for rect in svg.split("<rect ").skip(1) {
                let tag = rect.split('>').next().unwrap_or_default();
                for rx in tag.split(" rx=\"").skip(1) {
                    assert!(
                        rx.starts_with("0\""),
                        "{name}/{theme:?}: rounded <rect {tag}>"
                    );
                }
            }
        }
    }
}
