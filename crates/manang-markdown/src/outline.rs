//! Markdown heading and document outline extraction.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

/// Document heading entry for document outlines and table of contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    /// Heading text content.
    pub title: String,
    /// Heading level (1 to 6).
    pub level: u32,
    /// Explicit heading identifier (e.g. from `{#intro}`) if specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 1-based line number in source text.
    pub line: usize,
    /// Character offset in source text.
    pub offset: usize,
}

/// Extracts headings from markdown source text with positions and IDs.
pub fn extract_outline(source: &str) -> Vec<Heading> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;

    let parser = Parser::new_ext(source, options).into_offset_iter();
    let mut headings = Vec::new();

    let mut in_heading = false;
    let mut current_level = 1;
    let mut current_id = None;
    let mut current_title = String::new();
    let mut current_offset = 0;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, id, .. }) => {
                in_heading = true;
                current_level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current_id = id.map(|s| s.to_string());
                current_title.clear();
                current_offset = range.start;
            }
            Event::End(TagEnd::Heading(_)) => {
                if in_heading {
                    let line = source[..current_offset]
                        .chars()
                        .filter(|&c| c == '\n')
                        .count()
                        + 1;
                    headings.push(Heading {
                        title: current_title.trim().to_string(),
                        level: current_level,
                        id: current_id.take(),
                        line,
                        offset: current_offset,
                    });
                    in_heading = false;
                }
            }
            Event::Text(t) | Event::Code(t) if in_heading => {
                current_title.push_str(&t);
            }
            _ => {}
        }
    }

    headings
}
