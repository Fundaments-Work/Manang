//! Document formatting.
//!
//! * Typst: `typstyle-core`, wrapped to guarantee idempotence.
//! * Markdown: a conservative whitespace normaliser; prose and math are never
//!   touched.
//! * LaTeX: reported as unsupported. Without a full TeX parser every
//!   reformatter risks changing meaning (verbatim environments, `\catcode`
//!   tricks), so none is attempted.

pub mod markdown;
pub mod typst;

use serde::Serialize;
use serde_json::Value;

use crate::error::WritingError;
use crate::syntax::Syntax;
use crate::text::{CharRange, TextIndex};

pub use markdown::MarkdownOptions;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Formatted {
    pub formatted: String,
    pub changed: bool,
    /// Set when formatting was declined and the input returned as is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RangeFormatted {
    /// The whole text after applying the edit.
    pub formatted: String,
    pub changed: bool,
    /// Character range of the original text that `replacement` replaces.
    pub range: CharRange,
    pub replacement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

fn concrete(language: Syntax, source: &str) -> Result<Syntax, WritingError> {
    match language.resolve(source) {
        s @ (Syntax::Typst | Syntax::Markdown | Syntax::Latex) => Ok(s),
        _ => Err(WritingError::bad_arg(
            "language",
            "expected typst, markdown or latex",
        )),
    }
}

fn latex_unsupported() -> WritingError {
    WritingError::Unsupported(
        "LaTeX formatting is not available; use latexindent or your editor's TeX tooling"
            .to_owned(),
    )
}

pub fn format_document(
    source: &str,
    language: Syntax,
    options: Option<&Value>,
) -> Result<Formatted, WritingError> {
    match concrete(language, source)? {
        Syntax::Typst => {
            let config = typst::config_from_json(options)?;
            let out = typst::format_document(source, &config)?;
            Ok(Formatted {
                changed: out.formatted != source,
                formatted: out.formatted,
                skipped: out.skipped,
            })
        }
        Syntax::Markdown => {
            let opts = MarkdownOptions::from_json(options)?;
            let formatted = markdown::format_document(source, &opts);
            Ok(Formatted {
                changed: formatted != source,
                formatted,
                skipped: None,
            })
        }
        _ => Err(latex_unsupported()),
    }
}

pub fn format_range(
    source: &str,
    from: usize,
    to: usize,
    language: Syntax,
    options: Option<&Value>,
) -> Result<RangeFormatted, WritingError> {
    let index = TextIndex::new(source);
    if from > to || to > index.char_len() {
        return Err(WritingError::BadRange {
            from,
            to,
            len: index.char_len(),
        });
    }
    match concrete(language, source)? {
        Syntax::Typst => {
            let config = typst::config_from_json(options)?;
            let out = typst::format_range(source, from, to, &config)?;
            Ok(RangeFormatted {
                changed: out.formatted != source,
                formatted: out.formatted,
                range: out.range,
                replacement: out.replacement,
                skipped: out.skipped,
            })
        }
        Syntax::Markdown => {
            let opts = MarkdownOptions::from_json(options)?;
            let (bytes, replacement) = markdown::format_range(
                source,
                index.char_to_byte(from),
                index.char_to_byte(to),
                &opts,
            );
            let mut formatted = String::with_capacity(source.len());
            formatted.push_str(&source[..bytes.start]);
            formatted.push_str(&replacement);
            formatted.push_str(&source[bytes.end..]);
            Ok(RangeFormatted {
                changed: formatted != source,
                formatted,
                range: index.char_range(bytes),
                replacement,
                skipped: None,
            })
        }
        _ => Err(latex_unsupported()),
    }
}
