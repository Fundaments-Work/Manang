//! Structured diagnostics.
//!
//! Typst reports byte ranges into UTF-8 sources. The editor addresses text by
//! character index (`ropey`), so ranges are converted here once, at the edge.

use serde::Serialize;
use typst::diag::{Severity, SourceDiagnostic};
use typst::syntax::{FileId, VirtualRoot};
use typst::{World, WorldExt};

/// Upper bound on diagnostics reported per compile, so a runaway document
/// cannot flood the IPC channel.
pub const MAX_DIAGNOSTICS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagSeverity {
    Error,
    Warning,
}

/// A half-open range of Unicode scalar values (the editor's indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CharRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: DiagSeverity,
    pub message: String,
    /// Virtual path of the file, e.g. `/main.typ` or `@preview/pkg:0.1.0/lib.typ`.
    pub file: Option<String>,
    /// 1-based line of the start of the range.
    pub line: Option<usize>,
    /// 1-based column, counted in characters.
    pub column: Option<usize>,
    /// Character offsets into the file's text.
    pub range: Option<CharRange>,
    pub hints: Vec<String>,
    /// The call, show-rule and import chain that led here, innermost first.
    pub trace: Vec<String>,
}

pub(crate) fn file_label(id: FileId) -> String {
    let path = id.vpath().get_with_slash();
    match id.root() {
        VirtualRoot::Project => path.to_owned(),
        VirtualRoot::Package(spec) => format!("@{spec}{path}"),
    }
}

fn char_offset(text: &str, byte: usize) -> usize {
    // A range from the parser always sits on a boundary; fall back to the
    // nearest one below rather than panicking if a span is stale.
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].chars().count()
}

pub(crate) fn convert(world: &dyn World, diag: &SourceDiagnostic) -> Diagnostic {
    let severity = match diag.severity {
        Severity::Error => DiagSeverity::Error,
        Severity::Warning => DiagSeverity::Warning,
    };

    let mut out = Diagnostic {
        severity,
        message: diag.message.to_string(),
        file: None,
        line: None,
        column: None,
        range: None,
        hints: diag.hints.iter().map(|h| h.v.to_string()).collect(),
        trace: diag.trace.iter().map(|t| t.v.to_string()).collect(),
    };

    let Some(range) = world.range(diag.span) else {
        return out;
    };
    // The span's file is only reachable through the source it points into.
    let Some(id) = file_of(diag) else {
        return out;
    };
    out.file = Some(file_label(id));
    if let Ok(source) = world.source(id) {
        let text = source.text();
        let start = char_offset(text, range.start);
        let end = char_offset(text, range.end).max(start);
        out.range = Some(CharRange { start, end });
        if let Some((line, column)) = source
            .lines()
            .byte_to_line_column(range.start.min(text.len()))
        {
            out.line = Some(line + 1);
            out.column = Some(column + 1);
        }
    }
    out
}

fn file_of(diag: &SourceDiagnostic) -> Option<FileId> {
    use typst::syntax::DiagSpanKind;
    match diag.span.get() {
        DiagSpanKind::Detached => None,
        DiagSpanKind::Number { id, .. } | DiagSpanKind::Range { id, .. } => Some(id),
    }
}

/// Convert errors then warnings, capped at [`MAX_DIAGNOSTICS`].
pub(crate) fn collect<'a>(
    world: &dyn World,
    errors: impl IntoIterator<Item = &'a SourceDiagnostic>,
    warnings: impl IntoIterator<Item = &'a SourceDiagnostic>,
) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .chain(warnings)
        .take(MAX_DIAGNOSTICS)
        .map(|d| convert(world, d))
        .collect()
}
