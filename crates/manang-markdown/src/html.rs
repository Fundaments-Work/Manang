//! Sanitized HTML rendering with math preservation and XSS mitigation.

use pulldown_cmark::{CowStr, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::diagrams::{DiagramFence, OpenFence};

/// Configuration options for markdown HTML rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderOptions {
    /// Whether to permit raw HTML tags through an explicit allowlist.
    /// Default is `false` (all raw HTML is escaped as text).
    #[serde(default)]
    pub allow_raw_html: bool,

    /// Permitted HTML tags when `allow_raw_html` is `true`.
    #[serde(default = "default_allowed_tags")]
    pub allowed_html_tags: Vec<String>,
}

fn default_allowed_tags() -> Vec<String> {
    vec![
        "b".to_string(),
        "i".to_string(),
        "em".to_string(),
        "strong".to_string(),
        "span".to_string(),
        "sub".to_string(),
        "sup".to_string(),
        "kbd".to_string(),
        "br".to_string(),
        "p".to_string(),
        "code".to_string(),
    ]
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            allow_raw_html: false,
            allowed_html_tags: default_allowed_tags(),
        }
    }
}

/// Escapes XML and HTML special characters for safe inclusion in markup.
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Returns true if a URL contains unsafe protocols such as javascript:, vbscript:, or data:.
pub fn is_unsafe_url(url: &str) -> bool {
    let clean: String = url
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect();
    let lower = clean.to_ascii_lowercase();

    lower.starts_with("javascript:") || lower.starts_with("vbscript:") || lower.starts_with("data:")
}

/// Inspects a raw HTML snippet and returns either the allowed safe HTML or the escaped representation.
fn sanitize_raw_html_tag(
    raw: &str,
    allowed_tags: &[String],
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with('<') || !trimmed.ends_with('>') {
        return escape_html(raw);
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let is_closing = inner.starts_with('/');
    let tag_content = if is_closing { &inner[1..] } else { inner };

    let mut parts = tag_content.split_whitespace();
    let Some(tag_name) = parts.next() else {
        return escape_html(raw);
    };

    let tag_name_clean = tag_name.trim_end_matches('/').to_ascii_lowercase();

    if !allowed_tags
        .iter()
        .any(|t| t.eq_ignore_ascii_case(&tag_name_clean))
    {
        diagnostics.push(Diagnostic::warning(
            format!("disallowed raw HTML tag `<{tag_name_clean}>` was escaped"),
            Some(line),
        ));
        return escape_html(raw);
    }

    // Inspect attributes for event handlers or unsafe URLs
    let lower_inner = tag_content.to_ascii_lowercase();
    if lower_inner.contains("javascript:") || lower_inner.contains("data:") {
        diagnostics.push(Diagnostic::warning(
            format!("raw HTML tag `<{tag_name_clean}>` contained unsafe attribute and was escaped"),
            Some(line),
        ));
        return escape_html(raw);
    }

    let has_event_handler = parts.any(|part| {
        let p = part.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
        let p_lower = p.to_ascii_lowercase();
        p_lower.starts_with("on")
            && p_lower[2..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
    });

    if has_event_handler {
        diagnostics.push(Diagnostic::warning(
            format!("raw HTML tag `<{tag_name_clean}>` contained event handler attribute and was escaped"),
            Some(line),
        ));
        return escape_html(raw);
    }

    raw.to_string()
}

/// The parser options shared by every pass over a document.
pub(crate) fn parse_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
}

/// The empty element that stands for a diagram fence in the preview. Every
/// attribute value is generated here (a hex key, a fixed language id, a
/// number), never copied from the document, and raw HTML in the document is
/// escaped, so a document cannot forge one.
pub fn diagram_placeholder(fence: &DiagramFence) -> String {
    format!(
        "<figure class=\"mng-diagram\" data-diagram=\"{}\" data-language=\"{}\" data-line=\"{}\"></figure>\n",
        fence.key, fence.language, fence.line
    )
}

/// Renders markdown source into sanitized HTML with preserved math formulas.
/// Diagram fences become [`diagram_placeholder`]s.
pub fn render_to_html(source: &str, options: &RenderOptions) -> (String, Vec<Diagnostic>) {
    render_to_html_at(source, options, 0)
}

/// [`render_to_html`] for text that starts after `line_offset` lines of its
/// file (the frontmatter), so diagnostics and placeholders name file lines.
pub fn render_to_html_at(
    source: &str,
    options: &RenderOptions,
    line_offset: usize,
) -> (String, Vec<Diagnostic>) {
    let parser = Parser::new_ext(source, parse_options()).into_offset_iter();
    let mut diagnostics = Vec::new();
    let mut sanitized_events: Vec<Event<'static>> = Vec::new();

    let mut in_metadata_block = false;
    let mut diagram: Option<OpenFence> = None;
    let mut last_offset = 0;
    let mut current_line = line_offset + 1;

    for (event, range) in parser {
        if range.start > last_offset {
            let slice = &source.as_bytes()[last_offset..range.start.min(source.len())];
            current_line += slice.iter().filter(|&&b| b == b'\n').count();
            last_offset = range.start;
        }
        let line = current_line;

        if let Some(fence) = diagram.as_mut() {
            match event {
                Event::Text(text) => fence.source.push_str(&text),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(fence) = diagram.take() {
                        let html = diagram_placeholder(&fence.finish());
                        sanitized_events.push(Event::Html(CowStr::Boxed(html.into_boxed_str())));
                    }
                }
                _ => {}
            }
            continue;
        }
        if !in_metadata_block {
            diagram = OpenFence::start(&event, source, &range, line_offset);
            if diagram.is_some() {
                continue;
            }
        }

        match event {
            Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle))
            | Event::Start(Tag::MetadataBlock(MetadataBlockKind::PlusesStyle)) => {
                in_metadata_block = true;
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                in_metadata_block = false;
            }
            _ if in_metadata_block => {
                // Skip metadata content in HTML output
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let safe_dest = if is_unsafe_url(&dest_url) {
                    diagnostics.push(Diagnostic::warning(
                        format!("sanitized unsafe link destination `{dest_url}` to `about:blank`"),
                        Some(line),
                    ));
                    CowStr::Boxed("about:blank".to_string().into_boxed_str())
                } else {
                    dest_url.into_static()
                };
                sanitized_events.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: safe_dest,
                    title: title.into_static(),
                    id: id.into_static(),
                }));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let safe_dest = if is_unsafe_url(&dest_url) {
                    diagnostics.push(Diagnostic::warning(
                        format!("sanitized unsafe image source `{dest_url}` to `about:blank`"),
                        Some(line),
                    ));
                    CowStr::Boxed("about:blank".to_string().into_boxed_str())
                } else {
                    dest_url.into_static()
                };
                sanitized_events.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: safe_dest,
                    title: title.into_static(),
                    id: id.into_static(),
                }));
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                if options.allow_raw_html {
                    let safe = sanitize_raw_html_tag(
                        &raw,
                        &options.allowed_html_tags,
                        &mut diagnostics,
                        line,
                    );
                    sanitized_events.push(Event::Html(CowStr::Boxed(safe.into_boxed_str())));
                } else {
                    diagnostics.push(Diagnostic::warning(
                        "raw HTML tag escaped for XSS mitigation (enable allow_raw_html with allowlist to permit)",
                        Some(line),
                    ));
                    let escaped = escape_html(&raw);
                    sanitized_events.push(Event::Html(CowStr::Boxed(escaped.into_boxed_str())));
                }
            }
            Event::InlineMath(math) => {
                let escaped_math = escape_html(&math);
                let html = format!("<span class=\"math math-inline\">${escaped_math}$</span>");
                sanitized_events.push(Event::Html(CowStr::Boxed(html.into_boxed_str())));
            }
            Event::DisplayMath(math) => {
                let escaped_math = escape_html(&math);
                let html = format!("<div class=\"math math-display\">$${escaped_math}$$</div>\n");
                sanitized_events.push(Event::Html(CowStr::Boxed(html.into_boxed_str())));
            }
            other => {
                sanitized_events.push(other.into_static());
            }
        }
    }

    let mut html_out = String::new();
    pulldown_cmark::html::push_html(&mut html_out, sanitized_events.into_iter());

    (html_out, diagnostics)
}
