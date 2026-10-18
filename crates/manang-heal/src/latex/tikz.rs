//! `\usetikzlibrary`: TikZ features that need a library nobody loaded.
//!
//! The pgf errors name the key or arrow tip ("I do not know the key
//! '/tikz/right=of a'"), but not the library, and TikZ stops at the first one.
//! The rule reads the document for every feature that needs a library and
//! adds all the missing ones in one line.

use crate::engine::Draft;
use crate::types::{Confidence, DiagnosticIn};

use super::Env;

const RULE: &str = "latex.tikz-library";

struct Feature {
    library: &'static str,
    /// Words that, in the error message, tie it to this library.
    in_message: &'static [&'static str],
    detect: fn(&str) -> bool,
}

const FEATURES: &[Feature] = &[
    Feature {
        library: "positioning",
        in_message: &["above", "below", "left", "right", " of"],
        detect: positioning,
    },
    Feature {
        library: "arrows.meta",
        in_message: &["Stealth", "Latex", "tip", "To", "Triangle", "arrow"],
        detect: arrows_meta,
    },
    Feature {
        library: "calc",
        in_message: &["Giving up", "calc", "Math"],
        detect: |t| t.contains("($") || t.contains(" let \\p") || t.contains("[let \\p"),
    },
    Feature {
        library: "fit",
        in_message: &["fit"],
        detect: |t| t.contains("fit=(") || t.contains("fit = ("),
    },
    Feature {
        library: "shapes.geometric",
        in_message: &[
            "shape",
            "diamond",
            "ellipse",
            "trapezium",
            "star",
            "cylinder",
        ],
        detect: |t| {
            shape_is(
                t,
                &[
                    "diamond",
                    "ellipse",
                    "trapezium",
                    "regular polygon",
                    "star",
                    "semicircle",
                    "isosceles triangle",
                    "kite",
                    "dart",
                    "circular sector",
                    "cylinder",
                ],
            )
        },
    },
    Feature {
        library: "shapes.multipart",
        in_message: &["shape", "rectangle split", "split"],
        detect: |t| t.contains("rectangle split") || t.contains("circle split"),
    },
    Feature {
        library: "shapes.symbols",
        in_message: &["shape", "cloud"],
        detect: |t| shape_is(t, &["cloud", "signal", "starburst", "magnifying glass"]),
    },
    Feature {
        library: "shapes.misc",
        in_message: &["shape", "rounded", "cross out", "strike out"],
        detect: |t| {
            t.contains("rounded rectangle") || t.contains("cross out") || t.contains("strike out")
        },
    },
    Feature {
        library: "decorations.pathreplacing",
        in_message: &["decoration", "brace", "decorate"],
        detect: |t| decorated(t, &["brace", "ticks", "border"]),
    },
    Feature {
        library: "decorations.pathmorphing",
        in_message: &["decoration", "snake", "zigzag", "coil", "decorate"],
        detect: |t| {
            decorated(
                t,
                &["snake", "zigzag", "coil", "bumps", "saw", "random steps"],
            )
        },
    },
    Feature {
        library: "decorations.markings",
        in_message: &["decoration", "markings", "decorate"],
        detect: |t| decorated(t, &["markings"]),
    },
    Feature {
        library: "decorations.text",
        in_message: &["decoration", "text along path", "decorate"],
        detect: |t| decorated(t, &["text along path"]),
    },
    Feature {
        library: "patterns",
        in_message: &["pattern"],
        detect: |t| t.contains("pattern=") || t.contains("pattern ="),
    },
    Feature {
        library: "backgrounds",
        in_message: &["background", "layer"],
        detect: |t| t.contains("on background layer") || t.contains("show background"),
    },
    Feature {
        library: "matrix",
        in_message: &["matrix"],
        detect: |t| t.contains("matrix of nodes") || t.contains("matrix of math nodes"),
    },
    Feature {
        library: "shadows",
        in_message: &["shadow"],
        detect: |t| t.contains("drop shadow") || t.contains("general shadow"),
    },
    Feature {
        library: "mindmap",
        in_message: &["mindmap", "concept"],
        detect: |t| t.contains("mindmap"),
    },
    Feature {
        library: "intersections",
        in_message: &["intersection"],
        detect: |t| t.contains("name intersections"),
    },
    Feature {
        library: "through",
        in_message: &["through"],
        detect: |t| t.contains("circle through") || t.contains("through="),
    },
    Feature {
        library: "angles",
        in_message: &["angle", "pic"],
        detect: |t| t.contains("{angle") || t.contains("{angle ="),
    },
    Feature {
        library: "spy",
        in_message: &["spy"],
        detect: |t| t.contains("spy on") || t.contains("spy using"),
    },
    Feature {
        library: "3d",
        in_message: &["canvas", "3d"],
        detect: |t| t.contains("canvas is "),
    },
];

pub(super) fn missing_library(env: &Env<'_>, index: usize, d: &DiagnosticIn) -> Vec<Draft> {
    let lower = d.message.to_ascii_lowercase();
    let pgf_error = lower.contains("pgf") || lower.contains("tikz") || lower.contains("giving up");
    if !pgf_error || !(env.pre.loaded("tikz") || env.pre.loaded("pgfplots")) {
        return Vec::new();
    }
    let Some(pos) = env.pre.last_library_end.or(env.pre.package_insertion()) else {
        return Vec::new();
    };
    let mut missing: Vec<&Feature> = FEATURES
        .iter()
        .filter(|f| !env.pre.tikz_libraries.contains(f.library) && (f.detect)(&env.masked))
        .collect();
    // `shapes.*` and `decorations.*` libraries load the same support; one
    // already named by a prefix is enough only when exact, so keep them all.
    missing.sort_by_key(|f| f.library);
    if missing.is_empty() {
        return Vec::new();
    }
    let certain = missing
        .iter()
        .all(|f| f.in_message.iter().any(|w| d.message.contains(w)));
    let libraries: Vec<&str> = missing.iter().map(|f| f.library).collect();
    let joined = libraries.join(",");
    let line = format!("\\usetikzlibrary{{{joined}}}");
    let why = format!(
        "This document uses TikZ features from the {} librar{}, which {} not loaded.",
        libraries
            .iter()
            .map(|l| format!("`{l}`"))
            .collect::<Vec<_>>()
            .join(", "),
        if libraries.len() == 1 { "y" } else { "ies" },
        if libraries.len() == 1 { "is" } else { "are" },
    );
    vec![Draft {
        id: format!("latex.tikzlibrary:{joined}"),
        label: format!("Add {line}"),
        why,
        edits: vec![env.insert_lines_after(pos, &[line])],
        confidence: if certain {
            Confidence::Certain
        } else {
            Confidence::Likely
        },
        rule: RULE,
        diagnostics: vec![index],
        finding: None,
    }]
}

/// `above=of a`, `right = 2cm of b`, `below left=1cm and 2cm of c`.
fn positioning(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find("of") {
        let at = from + p;
        from = at + 2;
        let word_start = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        let word_end = !bytes.get(at + 2).is_some_and(u8::is_ascii_alphanumeric);
        if !word_start || !word_end {
            continue;
        }
        let mut head = text[..at].trim_end();
        // An optional distance: `2cm`, `1cm and 2cm`.
        let distance_end = head.rfind('=').map_or(0, |eq| eq + 1);
        let dist = &head[distance_end..];
        if !dist.chars().all(|c| {
            c.is_ascii_digit() || c.is_ascii_whitespace() || c == '.' || c.is_ascii_alphabetic()
        }) {
            continue;
        }
        head = head[..distance_end].trim_end();
        let Some(before_eq) = head.strip_suffix('=') else {
            continue;
        };
        let before_eq = before_eq.trim_end();
        if ["above", "below", "left", "right"]
            .iter()
            .any(|dir| before_eq.ends_with(dir))
        {
            return true;
        }
    }
    false
}

fn arrows_meta(text: &str) -> bool {
    [
        "Stealth",
        "{Latex",
        "=Latex",
        "-Latex",
        "{To",
        "=To",
        "-To",
        "Straight Barb",
        "{Triangle",
        "{Hooks",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

/// `shape=diamond` or `shape = star`.
fn shape_is(text: &str, shapes: &[&str]) -> bool {
    let mut from = 0;
    while let Some(p) = text[from..].find("shape") {
        let at = from + p + 5;
        from = at;
        let rest = text[at..].trim_start();
        if let Some(value) = rest.strip_prefix('=') {
            let value = value.trim_start();
            let end = value.find([',', ']', '}']).unwrap_or(value.len());
            if shapes.contains(&value[..end].trim()) {
                return true;
            }
        }
    }
    false
}

/// A `decoration={...}` option naming one of `words`.
fn decorated(text: &str, words: &[&str]) -> bool {
    let mut from = 0;
    while let Some(p) = text[from..].find("decoration") {
        let at = from + p + "decoration".len();
        from = at;
        let window_end = (at + 80).min(text.len());
        let mut end = window_end;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let window = &text[at..end];
        let rest = window.trim_start();
        if let Some(value) = rest.strip_prefix('=')
            && words
                .iter()
                .any(|w| value.trim_start().trim_start_matches('{').starts_with(w))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positioning_forms() {
        assert!(positioning("\\node[right=of a] {}"));
        assert!(positioning("\\node[below left = 1cm of a] {}"));
        assert!(positioning("\\node[above=1cm and 2cm of a] {}"));
        assert!(!positioning("\\node[right] {kind of nice}"));
        assert!(!positioning("a = b of c"));
    }

    #[test]
    fn other_detectors() {
        assert!(arrows_meta("\\draw[-{Stealth[length=3mm]}] (0,0)--(1,1);"));
        assert!(!arrows_meta("\\draw[->, >=stealth] (0,0)--(1,1);"));
        assert!(shape_is("\\node[shape=diamond, draw]", &["diamond"]));
        assert!(!shape_is("\\node[shape=circle]", &["diamond"]));
        assert!(decorated(
            "decorate, decoration={brace, amplitude=3pt}",
            &["brace"]
        ));
        assert!(!decorated("decoration={snake}", &["brace"]));
    }
}
