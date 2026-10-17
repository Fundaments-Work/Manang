//! Math symbol search.
//!
//! The table lives in `data/symbols.txt` and is parsed once. Each row carries
//! the exact insertion text for Typst and LaTeX, a unicode preview and
//! keywords, so `alpha`, `a`, `forall` and `for all` all find their symbol.

use std::sync::OnceLock;

use serde::Serialize;

use crate::syntax::Syntax;

const TABLE_SOURCE: &str = include_str!("../data/symbols.txt");
const SEPARATOR: char = '\u{a6}';

/// Default and maximum number of hits.
pub const DEFAULT_LIMIT: usize = 20;
pub const MAX_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Symbol {
    /// Short display name (`alpha`, `arrow.r.double`, `fraction`).
    pub name: String,
    /// Exact Typst math-mode insertion text, when Typst has the symbol.
    pub typst: Option<String>,
    /// Exact LaTeX math-mode insertion text, when LaTeX has the symbol.
    pub latex: Option<String>,
    /// Unicode preview.
    pub preview: String,
    /// `U+XXXX` of the preview, for single-character symbols.
    pub unicode: Option<String>,
    pub category: String,
    /// LaTeX package providing the command, when not in the kernel.
    pub requires: Option<String>,
    pub keywords: Vec<String>,
}

fn opt(column: &str) -> Option<String> {
    (column != "-").then(|| column.to_owned())
}

/// Identifier part of a Typst insertion (`arrow.r.double`, `sum` of
/// `sum_(i=1)^n`).
fn typst_ident(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
        .unwrap_or(text.len());
    &text[..end]
}

pub(crate) fn parse_table(source: &str) -> Result<Vec<Symbol>, String> {
    let mut out = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split(SEPARATOR).collect();
        let [typst, latex, preview, category, requires, keywords] = cols[..] else {
            return Err(format!(
                "line {}: expected 6 columns, got {}",
                n + 1,
                cols.len()
            ));
        };
        let keywords: Vec<String> = keywords
            .split(',')
            .map(|k| k.trim().to_lowercase())
            .filter(|k| !k.is_empty())
            .collect();
        let construct = category == "construct";
        let name = if construct {
            keywords
                .first()
                .cloned()
                .unwrap_or_else(|| typst_ident(typst).to_owned())
        } else if typst == "-" || !typst.is_ascii() {
            latex.trim_start_matches('\\').to_owned()
        } else {
            typst.to_owned()
        };
        let mut chars = preview.chars();
        let unicode = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(format!("U+{:04X}", u32::from(c))),
            _ => None,
        };
        out.push(Symbol {
            name,
            typst: opt(typst),
            latex: opt(latex),
            preview: preview.to_owned(),
            unicode,
            category: category.to_owned(),
            requires: opt(requires),
            keywords,
        });
    }
    Ok(out)
}

/// The built-in table.
pub fn table() -> &'static [Symbol] {
    static TABLE: OnceLock<Vec<Symbol>> = OnceLock::new();
    TABLE.get_or_init(|| parse_table(TABLE_SOURCE).unwrap_or_default())
}

/// A search hit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    pub name: String,
    /// The exact text to insert for the requested syntax.
    pub insert: String,
    pub typst: Option<String>,
    pub latex: Option<String>,
    pub preview: String,
    pub unicode: Option<String>,
    pub category: String,
    /// LaTeX package to load (only when the requested syntax is LaTeX).
    pub requires: Option<String>,
    pub keywords: Vec<String>,
    pub score: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathSyntax {
    Typst,
    Latex,
}

impl MathSyntax {
    /// Markdown math is LaTeX.
    pub fn from_syntax(syntax: Syntax) -> Option<Self> {
        match syntax {
            Syntax::Typst => Some(Self::Typst),
            Syntax::Latex | Syntax::Markdown => Some(Self::Latex),
            _ => None,
        }
    }
}

fn word_starts(candidate: &str, token: &str) -> bool {
    candidate
        .split([' ', '.', '_'])
        .any(|w| w.starts_with(token))
}

/// Score of one query token against one candidate string.
fn match_score(candidate: &str, token: &str, keyword: bool) -> u32 {
    let (exact, prefix, word, contains) = if keyword {
        (90, 50, 35, 15)
    } else {
        (100, 60, 40, 20)
    };
    if candidate == token {
        exact
    } else if candidate.starts_with(token) {
        prefix
    } else if word_starts(candidate, token) {
        word
    } else if token.chars().count() >= 2 && candidate.contains(token) {
        contains
    } else {
        0
    }
}

/// Identifier of a LaTeX insertion: `\\rightarrow` becomes `rightarrow`.
fn latex_ident(text: &str) -> String {
    text.trim_start_matches('\\')
        .trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_lowercase()
}

fn score(symbol: &Symbol, syntax: MathSyntax, query: &str, tokens: &[&str]) -> Option<u32> {
    let insert = insert_text(symbol, syntax)?;
    if symbol.preview == query || insert == query {
        return Some(1000);
    }
    let typst_name = symbol
        .typst
        .as_deref()
        .map(|t| typst_ident(t).to_lowercase());
    let latex_name = symbol.latex.as_deref().map(latex_ident);
    let (own, other) = match syntax {
        MathSyntax::Typst => (&typst_name, &latex_name),
        MathSyntax::Latex => (&latex_name, &typst_name),
    };
    let display = symbol.name.to_lowercase();
    let mut total = 0;
    for token in tokens {
        // Names from the requested syntax count in full; the other syntax's
        // names still match (so `\rightarrow` finds `arrow.r` in Typst), at
        // half weight, so `phi` in LaTeX prefers `\phi` over `\varphi`.
        let mut best = own.as_deref().map_or(0, |c| match_score(c, token, false));
        best = best.max(other.as_deref().map_or(0, |c| match_score(c, token, false)) / 2);
        best = best.max(match_score(&display, token, false) / 2);
        for keyword in &symbol.keywords {
            best = best.max(match_score(keyword, token, true));
        }
        if best == 0 {
            return None;
        }
        total += best;
    }
    // Prefer an exact-case hit (`Delta` over `delta` for the query `Delta`).
    if own.as_deref().is_some_and(|c| c == query.to_lowercase())
        && insert.trim_start_matches('\\') == query
    {
        total += 5;
    }
    Some(total)
}

fn insert_text(symbol: &Symbol, syntax: MathSyntax) -> Option<&str> {
    match syntax {
        MathSyntax::Typst => symbol.typst.as_deref(),
        MathSyntax::Latex => symbol.latex.as_deref(),
    }
}

/// Search the table. An empty query lists symbols in table order.
pub fn search(query: &str, syntax: MathSyntax, limit: usize, category: Option<&str>) -> Vec<Hit> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let normalised = query
        .trim()
        .trim_start_matches('\\')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lowered = normalised.to_lowercase();
    let tokens: Vec<&str> = lowered.split(' ').filter(|t| !t.is_empty()).collect();
    let mut scored: Vec<(u32, usize, &Symbol)> = table()
        .iter()
        .enumerate()
        .filter(|(_, s)| category.is_none_or(|c| s.category == c))
        .filter_map(|(i, s)| {
            if tokens.is_empty() {
                insert_text(s, syntax).map(|_| (0, i, s))
            } else {
                score(s, syntax, &normalised, &tokens).map(|sc| (sc, i, s))
            }
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .filter_map(|(score, _, s)| {
            let insert = insert_text(s, syntax)?.to_owned();
            Some(Hit {
                name: s.name.clone(),
                insert,
                typst: s.typst.clone(),
                latex: s.latex.clone(),
                preview: s.preview.clone(),
                unicode: s.unicode.clone(),
                category: s.category.clone(),
                requires: if syntax == MathSyntax::Latex {
                    s.requires.clone()
                } else {
                    None
                },
                keywords: s.keywords.clone(),
                score,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(q: &str, syntax: MathSyntax) -> Vec<String> {
        search(q, syntax, 5, None)
            .into_iter()
            .map(|h| h.name)
            .collect()
    }

    #[test]
    fn table_parses_completely() {
        let rows = TABLE_SOURCE
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .count();
        let parsed = parse_table(TABLE_SOURCE).expect("table parses");
        assert_eq!(parsed.len(), rows);
        assert!(parsed.len() > 200);
    }

    #[test]
    fn names_are_unique_per_syntax() {
        let mut typst = std::collections::HashSet::new();
        let mut latex = std::collections::HashSet::new();
        for s in table() {
            if let Some(t) = &s.typst {
                assert!(typst.insert(t.clone()), "duplicate typst `{t}`");
            }
            if let Some(l) = &s.latex {
                assert!(latex.insert(l.clone()), "duplicate latex `{l}`");
            }
        }
    }

    #[test]
    fn alpha_is_found_by_name_prefix_and_keywords() {
        assert_eq!(names("alpha", MathSyntax::Typst)[0], "alpha");
        assert_eq!(names("a", MathSyntax::Typst)[0], "alpha");
        assert_eq!(names("\\alpha", MathSyntax::Latex)[0], "alpha");
        assert_eq!(names("\u{3b1}", MathSyntax::Latex)[0], "alpha");
    }

    #[test]
    fn forall_by_name_and_phrase() {
        assert_eq!(names("forall", MathSyntax::Typst)[0], "forall");
        assert_eq!(names("for all", MathSyntax::Latex)[0], "forall");
        assert_eq!(
            names("universal quantifier", MathSyntax::Typst)[0],
            "forall"
        );
    }

    #[test]
    fn insertion_text_depends_on_syntax() {
        let typst = &search("rightarrow", MathSyntax::Typst, 1, None)[0];
        assert_eq!(typst.insert, "arrow.r");
        let latex = &search("arrow.r", MathSyntax::Latex, 1, None)[0];
        assert_eq!(latex.insert, "\\rightarrow");
        assert_eq!(latex.unicode.as_deref(), Some("U+2192"));
    }

    #[test]
    fn syntax_without_the_symbol_skips_it() {
        assert!(search("omicron", MathSyntax::Latex, 5, None).is_empty());
        assert_eq!(search("omicron", MathSyntax::Typst, 5, None).len(), 1);
    }

    #[test]
    fn empty_query_browses_and_category_filters() {
        assert_eq!(search("", MathSyntax::Typst, 3, None).len(), 3);
        let greek = search("", MathSyntax::Latex, 200, Some("greek"));
        assert!(greek.iter().all(|h| h.category == "greek"));
        assert!(greek.len() > 30);
    }

    #[test]
    fn requires_only_reported_for_latex() {
        let latex = &search("nexists", MathSyntax::Latex, 1, None)[0];
        assert_eq!(latex.requires.as_deref(), Some("amssymb"));
        let typst = &search("nexists", MathSyntax::Typst, 1, None)[0];
        assert_eq!(typst.requires, None);
    }

    #[test]
    fn limit_is_clamped() {
        assert_eq!(search("", MathSyntax::Typst, 0, None).len(), 1);
        assert!(search("", MathSyntax::Typst, 10_000, None).len() <= MAX_LIMIT);
    }
}
