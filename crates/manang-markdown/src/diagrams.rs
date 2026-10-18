//! Diagram fences: ```` ```mermaid ````, ```` ```pikchr ````, ```` ```bob ````
//! and ```` ```svgbob ```` code blocks, which the preview shows as pictures
//! and the Typst conversion turns into figures.
//!
//! This crate does not render them: the HTML gets a placeholder element the
//! frontend fills with the diagram plugin's SVG (as an `<img>`, never inline
//! markup), and the Typst conversion asks a caller-supplied function for the
//! rendered file.

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::html::parse_options;

/// Fence info strings and the `diagram.languages` id each one selects. Kept
/// in step with the diagram plugin's `fences` (checked by a kernel test).
const FENCES: [(&str, &str); 4] = [
    ("mermaid", "mermaid"),
    ("pikchr", "pikchr"),
    ("bob", "svgbob"),
    ("svgbob", "svgbob"),
];

/// The diagram language a fence's info string selects (its first word,
/// ASCII case-insensitive), or None for ordinary code.
pub fn fence_language(info: &str) -> Option<&'static str> {
    let word = info.split_whitespace().next()?;
    FENCES
        .iter()
        .find(|(fence, _)| fence.eq_ignore_ascii_case(word))
        .map(|(_, language)| *language)
}

/// One diagram fence of a Markdown document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagramFence {
    /// `diagram.languages` id: `mermaid`, `pikchr` or `svgbob`.
    pub language: String,
    /// The text between the fences.
    pub source: String,
    /// 1-based line of the opening fence. A diagnostic at line `n` of
    /// `source` is at line `line + n` of the document.
    pub line: usize,
    /// 1-based line of the closing fence (the last line when unclosed).
    pub end_line: usize,
    /// Names the placeholder in the preview HTML: a hash of language and
    /// source, so an unchanged fence keeps its key (and its picture).
    pub key: String,
}

/// FNV-1a over the language and source: stable across runs and platforms.
/// Only identifies a fence within the preview; the rendered-file cache uses
/// the diagram plugin's own key.
pub fn fence_key(language: &str, source: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in language
        .bytes()
        .chain(std::iter::once(0))
        .chain(source.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 1-based line of byte `offset` in `text`.
pub(crate) fn line_at(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Collects one fence's text while the parser walks through it.
#[derive(Debug)]
pub(crate) struct OpenFence {
    pub language: &'static str,
    pub source: String,
    pub line: usize,
    pub end_line: usize,
}

impl OpenFence {
    /// Starts a fence when `event` opens a diagram code block. `range` is
    /// the block's byte range in `text`; `line_offset` lines precede `text`.
    pub(crate) fn start(
        event: &Event<'_>,
        text: &str,
        range: &std::ops::Range<usize>,
        line_offset: usize,
    ) -> Option<Self> {
        let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event else {
            return None;
        };
        let language = fence_language(info)?;
        Some(Self {
            language,
            source: String::new(),
            line: line_offset + line_at(text, range.start),
            end_line: line_offset + line_at(text, range.end.saturating_sub(1).max(range.start)),
        })
    }

    pub(crate) fn finish(self) -> DiagramFence {
        let key = fence_key(self.language, &self.source);
        DiagramFence {
            language: self.language.to_owned(),
            source: self.source,
            line: self.line,
            end_line: self.end_line,
            key,
        }
    }
}

/// Every diagram fence of `source` (a whole document, frontmatter included),
/// in document order.
pub fn diagram_fences(source: &str) -> Vec<DiagramFence> {
    let mut fences = Vec::new();
    let mut open: Option<OpenFence> = None;
    for (event, range) in Parser::new_ext(source, parse_options()).into_offset_iter() {
        if let Some(fence) = open.as_mut() {
            match event {
                Event::Text(text) => fence.source.push_str(&text),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(fence) = open.take() {
                        fences.push(fence.finish());
                    }
                }
                _ => {}
            }
            continue;
        }
        open = OpenFence::start(&event, source, &range, 0);
    }
    fences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_come_from_the_first_word_of_the_info_string() {
        assert_eq!(fence_language("mermaid"), Some("mermaid"));
        assert_eq!(fence_language("Mermaid {.wide}"), Some("mermaid"));
        assert_eq!(fence_language("bob"), Some("svgbob"));
        assert_eq!(fence_language("svgbob"), Some("svgbob"));
        assert_eq!(fence_language("pikchr"), Some("pikchr"));
        assert_eq!(fence_language("rust"), None);
        assert_eq!(fence_language(""), None);
    }

    #[test]
    fn fences_report_lines_and_sources() {
        let doc = "---\ntitle: x\n---\n# T\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\n```rust\nfn main() {}\n```\n\n~~~bob\n+-+\n~~~\n";
        let fences = diagram_fences(doc);
        assert_eq!(fences.len(), 2);
        assert_eq!(fences[0].language, "mermaid");
        assert_eq!(fences[0].source, "flowchart TD\n  A --> B\n");
        assert_eq!((fences[0].line, fences[0].end_line), (6, 9));
        assert_eq!(fences[1].language, "svgbob");
        assert_eq!((fences[1].line, fences[1].end_line), (15, 17));
        assert_eq!(fences[1].key, fence_key("svgbob", "+-+\n"));
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        let fences = diagram_fences("text\n\n```pikchr\nbox\narrow\n");
        assert_eq!(fences.len(), 1);
        assert_eq!(fences[0].line, 3);
        assert_eq!(fences[0].source, "box\narrow\n");
    }

    #[test]
    fn keys_depend_on_language_and_source_only() {
        assert_eq!(fence_key("mermaid", "a"), fence_key("mermaid", "a"));
        assert_ne!(fence_key("mermaid", "a"), fence_key("mermaid", "b"));
        assert_ne!(fence_key("mermaid", "a"), fence_key("pikchr", "a"));
        assert_eq!(fence_key("mermaid", "a").len(), 16);
    }
}
