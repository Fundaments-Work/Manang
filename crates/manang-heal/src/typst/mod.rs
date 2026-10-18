//! Typst rules.
//!
//! Typst reports a scalar range for every diagnostic; the rules read the
//! text at that range and fix only what they can name exactly.

mod delims;
mod imports;
pub mod names;
pub mod packages;

use crate::engine::{Ctx, Draft};
use crate::text::byte_of_scalar;
use crate::types::DiagnosticIn;

/// A diagnostic placed in the text: byte range and the diagnostic's index.
pub(crate) struct Placed<'a> {
    pub index: usize,
    pub diagnostic: &'a DiagnosticIn,
    pub from: usize,
    pub to: usize,
}

pub(crate) fn suggest(ctx: &Ctx<'_>, diagnostics: &[DiagnosticIn]) -> Vec<Draft> {
    let placed: Vec<Placed<'_>> = diagnostics
        .iter()
        .enumerate()
        .filter(|(_, d)| d.is_error())
        .filter_map(|(index, diagnostic)| {
            let range = diagnostic.range?;
            let from = byte_of_scalar(ctx.text, range.start);
            let to = byte_of_scalar(ctx.text, range.end).max(from);
            Some(Placed {
                index,
                diagnostic,
                from,
                to,
            })
        })
        .collect();
    let mut out = Vec::new();
    out.extend(imports::missing_imports(ctx, &placed));
    out.extend(names::misspellings(ctx, &placed));
    out.extend(delims::unclosed(ctx, &placed));
    out.extend(delims::stray_closers(ctx, &placed));
    out
}

/// `unknown variable: NAME` as `NAME`.
pub(crate) fn unknown_variable(message: &str) -> Option<&str> {
    message
        .strip_prefix("unknown variable: ")
        .map(str::trim)
        .filter(|n| !n.is_empty())
}
