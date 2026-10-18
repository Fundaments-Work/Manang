//! `unknown variable` for a one-letter slip of a keyword or known function.

use std::collections::BTreeSet;

use crate::engine::{ByteEdit, Ctx, Draft};
use crate::types::Confidence;

use super::packages::all_symbols;
use super::{Placed, unknown_variable};

const RULE: &str = "typst.misspelled-name";

/// Keywords usable after `#`.
pub const KEYWORDS: &[&str] = &["set", "show", "let", "import", "include", "context"];

/// Functions and values of the standard library that a person types by hand.
/// A unit test checks each against the compiler's own library.
pub const FUNCTIONS: &[&str] = &[
    "align",
    "bibliography",
    "block",
    "box",
    "circle",
    "cite",
    "colbreak",
    "columns",
    "counter",
    "curve",
    "document",
    "ellipse",
    "emph",
    "enum",
    "figure",
    "footnote",
    "grid",
    "heading",
    "highlight",
    "hide",
    "image",
    "label",
    "layout",
    "line",
    "linebreak",
    "link",
    "list",
    "lorem",
    "measure",
    "move",
    "numbering",
    "outline",
    "overline",
    "pad",
    "page",
    "pagebreak",
    "par",
    "parbreak",
    "path",
    "place",
    "polygon",
    "quote",
    "raw",
    "rect",
    "repeat",
    "rotate",
    "scale",
    "smallcaps",
    "smartquote",
    "square",
    "stack",
    "state",
    "strike",
    "strong",
    "sub",
    "super",
    "table",
    "terms",
    "text",
    "underline",
    "upper",
    "lower",
    "query",
    "regex",
    "datetime",
    "duration",
    "range",
    "assert",
    "panic",
    "repr",
    "type",
    "eval",
    "read",
    "json",
    "yaml",
    "csv",
    "rgb",
    "luma",
    "cmyk",
    "gradient",
    "stroke",
    "length",
    "angle",
    "ratio",
    "fraction",
    "relative",
    "content",
    "symbol",
    "version",
    "bytes",
    "calc",
    "math",
    "sym",
    "emoji",
    "float",
    "int",
    "str",
    "bool",
    "array",
    "dictionary",
    "arguments",
    "function",
    "module",
    "selector",
    "frac",
    "sqrt",
    "binom",
    "cases",
    "vec",
    "mat",
    "abs",
    "norm",
    "floor",
    "ceil",
    "round",
    "lr",
    "attach",
    "limits",
    "scripts",
    "cancel",
    "display",
    "inline",
    "equation",
    "title",
];

/// Names the document defines itself: `#let name`, `#let name(...)`, `let name`.
pub(super) fn defined_names(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("let ") {
        let before_ok = at == 0
            || !rest.as_bytes()[at - 1].is_ascii_alphanumeric() && rest.as_bytes()[at - 1] != b'-';
        let after = &rest[at + 4..];
        if before_ok {
            let name: String = after
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
        rest = after;
    }
    out
}

pub(super) fn misspellings(ctx: &Ctx<'_>, placed: &[Placed<'_>]) -> Vec<Draft> {
    let defined = defined_names(ctx.text);
    let mut out = Vec::new();
    for p in placed {
        let Some(name) = unknown_variable(&p.diagnostic.message) else {
            continue;
        };
        if ctx.text.get(p.from..p.to) != Some(name)
            || defined.contains(name)
            || !super::packages::providers(name).is_empty()
        {
            continue;
        }
        // The `#` of `#sett` is not part of the name; keep it.
        let Some(fix) = single_close_match(name, &defined) else {
            continue;
        };
        let keyword = KEYWORDS.contains(&fix.as_str());
        out.push(Draft {
            id: format!("typst.rename:{name}->{fix}:{}", p.from),
            label: format!("Change {name} to {fix}"),
            why: format!(
                "`{name}` is not defined; `{fix}` is the only {} one letter away.",
                if keyword { "keyword" } else { "known name" }
            ),
            edits: vec![ByteEdit::replace(p.from, p.to, fix.clone())],
            confidence: Confidence::Likely,
            rule: RULE,
            diagnostics: vec![p.index],
            finding: None,
        });
    }
    out
}

/// The one candidate within one edit of `name`, if exactly one exists.
fn single_close_match(name: &str, defined: &BTreeSet<String>) -> Option<String> {
    if name.chars().count() < 3 {
        return None;
    }
    let mut found: Option<&str> = None;
    let mut candidates: Vec<&str> = defined.iter().map(String::as_str).collect();
    for fixed in KEYWORDS.iter().chain(FUNCTIONS) {
        candidates.push(fixed);
    }
    for symbol in all_symbols() {
        candidates.push(symbol);
    }
    for candidate in candidates {
        if candidate.chars().count() < 3 || !within_one_edit(name, candidate) {
            continue;
        }
        match found {
            Some(prev) if prev != candidate => return None,
            _ => found = Some(candidate),
        }
    }
    found.map(str::to_owned)
}

/// Optimal-string-alignment distance of exactly 1: one substitution,
/// insertion, deletion or swap of two neighbours.
pub fn within_one_edit(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a == b {
        return false;
    }
    match a.len().abs_diff(b.len()) {
        0 => {
            let diffs: Vec<usize> = (0..a.len()).filter(|i| a[*i] != b[*i]).collect();
            match diffs.as_slice() {
                [_] => true,
                [i, j] => *j == *i + 1 && a[*i] == b[*j] && a[*j] == b[*i],
                _ => false,
            }
        }
        1 => {
            let (short, long) = if a.len() < b.len() {
                (&a, &b)
            } else {
                (&b, &a)
            };
            let skip = (0..short.len())
                .find(|i| short[*i] != long[*i])
                .unwrap_or(short.len());
            short[skip..] == long[skip + 1..]
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_edit() {
        assert!(within_one_edit("sett", "set"));
        assert!(within_one_edit("shwo", "show"));
        assert!(within_one_edit("headng", "heading"));
        assert!(within_one_edit("heeading", "heading"));
        assert!(within_one_edit("haeding", "heading"));
        assert!(!within_one_edit("heading", "heading"));
        assert!(!within_one_edit("hading", "heeding"));
        assert!(within_one_edit("abcd", "abdc"));
        assert!(!within_one_edit("ab", "abcd"));
    }

    #[test]
    fn unique_matches_only() {
        let none = BTreeSet::new();
        assert_eq!(single_close_match("sett", &none).as_deref(), Some("set"));
        assert_eq!(single_close_match("stlye", &none), None, "two letters off");
        // `pax` is one letter from both `par` and `pad`: no guess.
        assert_eq!(single_close_match("pax", &none), None);
        assert_eq!(single_close_match("se", &none), None, "too short");
        let mine: BTreeSet<String> = ["myfunc".to_owned()].into();
        assert_eq!(
            single_close_match("myfnc", &mine).as_deref(),
            Some("myfunc")
        );
    }
}
