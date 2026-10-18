//! The request and response shapes of `heal.suggest`.

use serde::{Deserialize, Serialize};

/// The engine whose diagnostics are being healed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Typst,
    Latex,
    Markdown,
}

impl Engine {
    /// Parses `typst`, `latex` or `markdown` (case-insensitive).
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "typst" => Some(Self::Typst),
            "latex" | "tex" => Some(Self::Latex),
            "markdown" | "md" => Some(Self::Markdown),
            _ => None,
        }
    }
}

/// A half-open range of Unicode scalar values, as the compilers report them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScalarRange {
    pub start: usize,
    pub end: usize,
}

/// A compile diagnostic, in the shape the compile commands return. Every
/// field but `message` is optional: LaTeX gives a line, Typst a range.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticIn {
    #[serde(default)]
    pub severity: Option<String>,
    pub message: String,
    /// 1-based line.
    #[serde(default)]
    pub line: Option<usize>,
    /// 1-based column, in Unicode scalar values.
    #[serde(default)]
    pub column: Option<usize>,
    /// Scalar-value offsets into the text.
    #[serde(default)]
    pub range: Option<ScalarRange>,
    #[serde(default)]
    pub hints: Vec<String>,
    /// A single hint (the LaTeX shape).
    #[serde(default)]
    pub hint: Option<String>,
}

impl DiagnosticIn {
    /// A diagnostic with only a message and a line, mostly for tests.
    pub fn at_line(message: impl Into<String>, line: usize) -> Self {
        Self {
            severity: Some("error".to_owned()),
            message: message.into(),
            line: Some(line),
            ..Self::default()
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity.as_deref().is_none_or(|s| s == "error")
    }
}

/// How sure a fix is.
///
/// `certain` fixes have exactly one reading and cannot change the meaning of
/// the document; only they are applied by Fix all and on save. `likely` fixes
/// pick the most plausible of several readings (where to close a brace) and
/// are always the user's call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Certain,
    Likely,
}

/// One replacement in UTF-16 code units of the original text, the unit the
/// editor uses. `from == to` is an insertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    /// Stable for the same fix, e.g. `latex.package:graphicx`.
    pub id: String,
    /// What the button says, e.g. `Add \usepackage{graphicx}`.
    pub label: String,
    /// One sentence on why.
    pub why: String,
    pub edits: Vec<Edit>,
    pub confidence: Confidence,
    /// The rule that produced it, e.g. `latex.missing-package`.
    pub rule: String,
    /// Indexes into the request's `diagnostics` that this fix clears.
    pub diagnostics: Vec<usize>,
    /// Index into `findings` for fixes of a problem the compiler does not
    /// report (a Markdown fence left open).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding: Option<usize>,
}

/// A problem found by scanning the text, not reported by a compiler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub message: String,
    pub severity: String,
    /// UTF-16 offsets of the marked text.
    pub from: usize,
    pub to: usize,
    /// 1-based line of `from`.
    pub line: usize,
    pub rule: String,
}

/// The combined certain fixes that do not overlap: one transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixAll {
    pub label: String,
    /// Ids of the fixes it contains.
    pub fixes: Vec<String>,
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub fixes: Vec<Fix>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_all: Option<FixAll>,
    pub findings: Vec<Finding>,
}
