//! The rule engine: runs an engine's rules, merges equal fixes, converts
//! byte offsets to the editor's UTF-16 and builds Fix all.

use crate::text::{Lines, utf16_of_byte};
use crate::types::{Confidence, DiagnosticIn, Edit, Engine, Finding, Fix, FixAll, Suggestion};
use crate::{latex, markdown, typst};

/// A replacement in bytes of the UTF-8 text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByteEdit {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

impl ByteEdit {
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self {
            from: at,
            to: at,
            insert: text.into(),
        }
    }

    pub fn replace(from: usize, to: usize, text: impl Into<String>) -> Self {
        Self {
            from,
            to,
            insert: text.into(),
        }
    }
}

/// A problem found by a scan, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DraftFinding {
    pub message: String,
    pub severity: &'static str,
    pub from: usize,
    pub to: usize,
    pub rule: &'static str,
}

/// A fix before offsets are converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Draft {
    pub id: String,
    pub label: String,
    pub why: String,
    pub edits: Vec<ByteEdit>,
    pub confidence: Confidence,
    pub rule: &'static str,
    pub diagnostics: Vec<usize>,
    pub finding: Option<DraftFinding>,
}

/// The text under repair plus its line index, passed to every rule.
pub(crate) struct Ctx<'a> {
    pub text: &'a str,
    pub lines: Lines,
}

impl<'a> Ctx<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            lines: Lines::new(text),
        }
    }
}

/// Fixes for `diagnostics` of the document `text`.
///
/// Pure: no clocks, no files. Rules that scan the text themselves
/// (Markdown) also run when `diagnostics` is empty.
pub fn suggest(engine: Engine, text: &str, diagnostics: &[DiagnosticIn]) -> Suggestion {
    let ctx = Ctx::new(text);
    let drafts = match engine {
        Engine::Latex => latex::suggest(&ctx, diagnostics),
        Engine::Typst => typst::suggest(&ctx, diagnostics),
        Engine::Markdown => markdown::suggest(&ctx, diagnostics),
    };
    finish(&ctx, drafts)
}

fn finish(ctx: &Ctx<'_>, drafts: Vec<Draft>) -> Suggestion {
    let drafts = merge(drafts);
    let mut findings: Vec<Finding> = Vec::new();
    let mut fixes: Vec<Fix> = Vec::new();
    for draft in &drafts {
        let finding = draft.finding.as_ref().map(|f| {
            let from = utf16_of_byte(ctx.text, f.from);
            let known = findings
                .iter()
                .position(|g| g.rule == f.rule && g.from == from && g.message == f.message);
            known.unwrap_or_else(|| {
                findings.push(Finding {
                    message: f.message.clone(),
                    severity: f.severity.to_owned(),
                    from,
                    to: utf16_of_byte(ctx.text, f.to),
                    line: ctx.lines.line_of(f.from),
                    rule: f.rule.to_owned(),
                });
                findings.len() - 1
            })
        });
        fixes.push(Fix {
            id: draft.id.clone(),
            label: draft.label.clone(),
            why: draft.why.clone(),
            edits: to_utf16(ctx.text, &draft.edits),
            confidence: draft.confidence,
            rule: draft.rule.to_owned(),
            diagnostics: draft.diagnostics.clone(),
            finding,
        });
    }
    let fix_all = fix_all(&drafts, ctx.text);
    Suggestion {
        fixes,
        fix_all,
        findings,
    }
}

/// Equal ids are one fix clearing several diagnostics. Certain fixes come
/// first, then source order of the first diagnostic.
fn merge(drafts: Vec<Draft>) -> Vec<Draft> {
    let mut out: Vec<Draft> = Vec::new();
    for draft in drafts {
        match out.iter_mut().find(|d| d.id == draft.id) {
            Some(known) => {
                for index in draft.diagnostics {
                    if !known.diagnostics.contains(&index) {
                        known.diagnostics.push(index);
                    }
                }
            }
            None => out.push(draft),
        }
    }
    for draft in &mut out {
        draft.diagnostics.sort_unstable();
    }
    out.sort_by_key(|d| {
        (
            d.confidence,
            d.diagnostics.first().copied().unwrap_or(usize::MAX),
        )
    });
    out
}

fn to_utf16(text: &str, edits: &[ByteEdit]) -> Vec<Edit> {
    let mut out: Vec<Edit> = edits
        .iter()
        .map(|e| Edit {
            from: utf16_of_byte(text, e.from),
            to: utf16_of_byte(text, e.to),
            insert: e.insert.clone(),
        })
        .collect();
    out.sort_by_key(|e| (e.from, e.to));
    out
}

/// Certain fixes whose edits do not overlap, as one set of edits. Two
/// insertions at the same offset are both kept, in fix order.
fn fix_all(drafts: &[Draft], text: &str) -> Option<FixAll> {
    let mut chosen: Vec<&Draft> = Vec::new();
    for draft in drafts
        .iter()
        .filter(|d| d.confidence == Confidence::Certain)
    {
        // Alternatives of one diagnostic (two packages for one command) are
        // never both applied: the first stays.
        let competes = chosen
            .iter()
            .any(|c| c.diagnostics.iter().any(|i| draft.diagnostics.contains(i)));
        if competes || overlaps(&chosen, draft) {
            continue;
        }
        chosen.push(draft);
    }
    if chosen.len() < 2 {
        return None;
    }
    let mut edits: Vec<ByteEdit> = chosen.iter().flat_map(|d| d.edits.clone()).collect();
    edits.sort_by_key(|e| (e.from, e.to));
    Some(FixAll {
        label: format!("Fix all ({})", chosen.len()),
        fixes: chosen.iter().map(|d| d.id.clone()).collect(),
        edits: to_utf16(text, &edits),
    })
}

fn overlaps(chosen: &[&Draft], draft: &Draft) -> bool {
    draft.edits.iter().any(|e| {
        chosen.iter().flat_map(|c| &c.edits).any(|o| {
            // A pure insertion only conflicts with a replacement spanning it.
            let e_ins = e.from == e.to;
            let o_ins = o.from == o.to;
            match (e_ins, o_ins) {
                (true, true) => false,
                (true, false) => e.from > o.from && e.from < o.to,
                (false, true) => o.from > e.from && o.from < e.to,
                (false, false) => e.from < o.to && o.from < e.to,
            }
        })
    })
}

/// Apply `edits` (UTF-16 offsets of `text`, sorted or not) and return the
/// new text. Edits that fall outside the text or overlap are skipped, so a
/// stale fix can never panic. Insertions at one offset keep their order.
pub fn apply_edits(text: &str, edits: &[Edit]) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.from, e.to));
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut cursor = 0;
    for edit in sorted {
        if edit.from < cursor || edit.to < edit.from || edit.to > units.len() {
            continue;
        }
        out.extend_from_slice(&units[cursor..edit.from]);
        out.extend(edit.insert.encode_utf16());
        cursor = edit.to;
    }
    out.extend_from_slice(&units[cursor..]);
    String::from_utf16_lossy(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(id: &str, at: usize, text: &str, diagnostic: usize) -> Draft {
        Draft {
            id: id.to_owned(),
            label: id.to_owned(),
            why: String::new(),
            edits: vec![ByteEdit::insert(at, text)],
            confidence: Confidence::Certain,
            rule: "test",
            diagnostics: vec![diagnostic],
            finding: None,
        }
    }

    #[test]
    fn equal_ids_merge_and_alternatives_do_not_combine() {
        let ctx = Ctx::new("abc");
        let out = finish(
            &ctx,
            vec![
                insert("a", 1, "X", 0),
                insert("a", 1, "X", 1),
                insert("b", 1, "Y", 2),
                insert("c", 1, "Z", 2),
            ],
        );
        assert_eq!(out.fixes.len(), 3);
        assert_eq!(out.fixes[0].diagnostics, vec![0, 1]);
        let all = out.fix_all.expect("fix all");
        assert_eq!(all.fixes, vec!["a", "b"]);
        assert_eq!(all.edits.len(), 2);
    }

    #[test]
    fn overlapping_replacements_are_not_combined() {
        let mut a = insert("a", 0, "", 0);
        a.edits = vec![ByteEdit::replace(0, 3, "x")];
        let mut b = insert("b", 0, "", 1);
        b.edits = vec![ByteEdit::replace(2, 5, "y")];
        assert!(fix_all(&[a, b], "abcdef").is_none());
    }
}
