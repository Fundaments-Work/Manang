//! Markdown rules.

use crate::engine::{Ctx, Draft};
use crate::types::DiagnosticIn;

pub(crate) fn suggest(_ctx: &Ctx<'_>, _diagnostics: &[DiagnosticIn]) -> Vec<Draft> {
    Vec::new()
}
