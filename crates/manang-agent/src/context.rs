//! Context chips: the only things sent to a provider besides the user's own message.
//!
//! Each chip is resolved to text, capped, token-estimated and redacted before anything is
//! assembled, so `agent.preview-request` and the real request carry identical bytes.

use crate::error::AgentError;
use crate::text::{clip_chars, estimate_tokens};
use manang_agent_core::{RedactionConfig, RedactionSpan, redact};
use serde::{Deserialize, Serialize};

/// What a chip holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChipKind {
    Selection,
    File,
    Diagnostics,
    Outline,
    BibliographyKeys,
    Figure,
    Pinned,
}

impl ChipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::File => "file",
            Self::Diagnostics => "diagnostics",
            Self::Outline => "outline",
            Self::BibliographyKeys => "bibliography-keys",
            Self::Figure => "figure",
            Self::Pinned => "pinned",
        }
    }

    /// Most characters of the chip that are ever sent.
    fn cap(self) -> usize {
        match self {
            Self::Selection => 40_000,
            Self::File => 80_000,
            Self::Diagnostics => 8_000,
            Self::Outline => 6_000,
            Self::BibliographyKeys => 8_000,
            Self::Figure => 12_000,
            Self::Pinned => 20_000,
        }
    }
}

/// One problem from the editor, with the lines around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub column: Option<u32>,
    pub severity: String,
    pub message: String,
    /// The lines around the problem, as shown in the editor.
    #[serde(default)]
    pub context: Option<String>,
}

/// A chip as the UI sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChipInput {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: ChipKind,
    #[serde(default)]
    pub label: Option<String>,
    /// Project-relative, `/`-separated.
    #[serde(default)]
    pub path: Option<String>,
    /// UTF-16 offsets into the file, for a selection.
    #[serde(default)]
    pub range: Option<[usize; 2]>,
    /// The text. Absent for chips the host can resolve itself (figure, outline, bibliography).
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    pub pinned: bool,
}

/// A chip ready to send.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedChip {
    pub id: String,
    pub kind: ChipKind,
    pub label: String,
    pub path: Option<String>,
    pub range: Option<[usize; 2]>,
    pub pinned: bool,
    /// Estimated tokens of what is sent.
    pub tokens: u64,
    pub truncated: bool,
    /// What is sent (after the cap and redaction).
    pub text: String,
    /// Present only when redaction changed something: the text before redaction, for the diff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    pub redactions: Vec<RedactionSpan>,
    /// The unredacted, uncapped text the editor holds, for tools (never serialised).
    #[serde(skip)]
    pub source: String,
}

/// Turns an input chip with known `text` into a resolved one.
pub fn resolve_chip(
    input: &ChipInput,
    index: usize,
    text: &str,
    config: &RedactionConfig,
) -> ResolvedChip {
    let (clipped, truncated) = clip_chars(text, input.kind.cap());
    let redaction = redact(&clipped, config);
    let redacted = !redaction.spans.is_empty();
    let label = input.label.clone().unwrap_or_else(|| default_label(input));
    ResolvedChip {
        id: input
            .id
            .clone()
            .unwrap_or_else(|| format!("{}-{index}", input.kind.as_str())),
        kind: input.kind,
        label,
        path: input.path.clone(),
        range: input.range,
        pinned: input.pinned,
        tokens: estimate_tokens(&redaction.text),
        truncated,
        original: redacted.then(|| clipped.clone()),
        redactions: redaction.spans,
        text: redaction.text,
        source: text.to_owned(),
    }
}

fn default_label(input: &ChipInput) -> String {
    let file = input.path.as_deref().unwrap_or("document");
    match input.kind {
        ChipKind::Selection => format!("Selection in {file}"),
        ChipKind::File => file.to_owned(),
        ChipKind::Diagnostics => "Problems".to_owned(),
        ChipKind::Outline => format!("Outline of {file}"),
        ChipKind::BibliographyKeys => "Bibliography keys".to_owned(),
        ChipKind::Figure => format!("Figure {file}"),
        ChipKind::Pinned => "Pinned note".to_owned(),
    }
}

/// The text of a diagnostics chip.
pub fn diagnostics_text(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diagnostics.iter().take(40) {
        let at = match d.column {
            Some(c) => format!("{}:{}:{}", d.path, d.line, c),
            None => format!("{}:{}", d.path, d.line),
        };
        out.push_str(&format!("{} at {at}: {}\n", d.severity, d.message));
        if let Some(context) = &d.context {
            out.push_str(context);
            if !context.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out
}

/// A tag that appears inside data must not be able to close its wrapper.
fn neutralise(text: &str) -> String {
    text.replace("</chip>", "<\\/chip>")
        .replace("</manang_context>", "<\\/manang_context>")
        .replace("</user_request>", "<\\/user_request>")
}

fn attr(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '"' | '<' | '>' | '\n' | '\r'))
        .collect()
}

/// The user message: delimited context (data) first, then the request.
pub fn render_user_message(chips: &[ResolvedChip], request: &str) -> String {
    let mut out = String::new();
    if !chips.is_empty() {
        out.push_str("<manang_context>\n");
        for chip in chips {
            out.push_str(&format!("<chip kind=\"{}\"", chip.kind.as_str()));
            if let Some(path) = &chip.path {
                out.push_str(&format!(" path=\"{}\"", attr(path)));
            }
            if let Some([from, to]) = chip.range {
                out.push_str(&format!(" range=\"{from}-{to}\""));
            }
            if chip.truncated {
                out.push_str(" truncated=\"true\"");
            }
            out.push_str(">\n");
            out.push_str(&neutralise(&chip.text));
            if !chip.text.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("</chip>\n");
        }
        out.push_str("</manang_context>\n\n");
    }
    out.push_str("<user_request>\n");
    out.push_str(&neutralise(request));
    out.push_str("\n</user_request>");
    out
}

/// Total estimated tokens, refused when above `limit`.
pub fn check_total(chips: &[ResolvedChip], extra: u64, limit: u64) -> Result<u64, AgentError> {
    let total: u64 = chips.iter().map(|c| c.tokens).sum::<u64>() + extra;
    if total > limit {
        let biggest = chips.iter().max_by_key(|c| c.tokens);
        let hint = biggest.map_or(String::new(), |c| {
            format!(" Remove or shorten \"{}\" ({} tokens).", c.label, c.tokens)
        });
        return Err(AgentError::new(
            "context_too_large",
            format!("the attached context is about {total} tokens; the limit is {limit}.{hint}"),
        ));
    }
    Ok(total)
}
