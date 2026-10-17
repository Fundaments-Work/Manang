//! Diagnostics in the shape `manang-typst` emits, so the editor renders both
//! with one code path.

use serde::Serialize;

use crate::text::{CharRange, TextIndex};

/// Upper bound on diagnostics per request, so a pathological text cannot
/// flood the IPC channel (same cap as `manang-typst`).
pub const MAX_DIAGNOSTICS: usize = 500;

/// `error` and `warning` match `manang-typst`. `info` is added for style
/// advice that should never be presented as a defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagSeverity {
    Error,
    Warning,
    Info,
}

impl DiagSeverity {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "info" => Some(Self::Info),
            _ => None,
        }
    }
}

/// Field-for-field the `manang-typst` diagnostic, plus `code` (the rule id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: DiagSeverity,
    pub message: String,
    pub file: Option<String>,
    /// 1-based line of the start of the range.
    pub line: Option<usize>,
    /// 1-based column, counted in characters.
    pub column: Option<usize>,
    /// Character offsets into the text.
    pub range: Option<CharRange>,
    pub hints: Vec<String>,
    pub trace: Vec<String>,
    /// Stable rule id, e.g. `repeated-word` or `spelling`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl Diagnostic {
    pub(crate) fn new(
        index: &TextIndex<'_>,
        file: Option<&str>,
        code: &str,
        severity: DiagSeverity,
        range: CharRange,
        message: String,
        hints: Vec<String>,
    ) -> Self {
        let (line, column) = index.line_col(range.start);
        Self {
            severity,
            message,
            file: file.map(str::to_owned),
            line: Some(line),
            column: Some(column),
            range: Some(range),
            hints,
            trace: Vec::new(),
            code: Some(code.to_owned()),
        }
    }
}
