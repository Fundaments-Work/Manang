//! LaTeX rules.
//!
//! A diagnostic is matched by its message (the log parser's own wording);
//! each rule then reads the document, never the log, to decide the edit.

mod balance;
mod packages;
pub mod scan;
pub mod tables;
mod tikz;

use crate::engine::{ByteEdit, Ctx, Draft};
use crate::text::line_ending;
use crate::types::DiagnosticIn;

use scan::Preamble;

/// What a rule can see.
pub(crate) struct Env<'a> {
    pub ctx: &'a Ctx<'a>,
    pub masked: String,
    pub pre: Preamble,
}

impl Env<'_> {
    pub fn text(&self) -> &str {
        self.ctx.text
    }

    pub fn eol(&self) -> &'static str {
        line_ending(self.ctx.text)
    }

    /// An edit that adds `lines` (no line endings) as new lines after the
    /// text ending at byte `pos`, keeping a trailing comment on its line.
    pub fn insert_lines_after(&self, pos: usize, lines: &[String]) -> ByteEdit {
        let eol = self.eol();
        let text = self.text();
        let line_end = text[pos..]
            .find(['\n', '\r'])
            .map_or(text.len(), |p| pos + p);
        let rest_is_blank = self.masked[pos..line_end].trim().is_empty();
        let body: String = lines.iter().map(|l| format!("{eol}{l}")).collect();
        if rest_is_blank {
            ByteEdit::insert(line_end, body)
        } else {
            // Something follows on the same line: split it.
            ByteEdit::insert(pos, format!("{body}{eol}"))
        }
    }
}

pub(crate) fn suggest(ctx: &Ctx<'_>, diagnostics: &[DiagnosticIn]) -> Vec<Draft> {
    let masked = scan::mask(ctx.text);
    let pre = Preamble::scan(&masked);
    let env = Env { ctx, masked, pre };
    let mut out = Vec::new();
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if !diagnostic.is_error() {
            continue;
        }
        out.extend(packages::undefined_command(&env, index, diagnostic));
        out.extend(packages::undefined_environment(&env, index, diagnostic));
        out.extend(tikz::missing_library(&env, index, diagnostic));
        out.extend(balance::unbalanced(&env, index, diagnostic));
    }
    out
}
