//! Sanitized HTML rendering with math preservation and XSS mitigation.

use pulldown_cmark::{CowStr, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;

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

/// Returns true if a URL contains unsafe protocols such as javascript:, vbscript:, or data:text/html.
pub fn is_unsafe_url(url: &str) -> bool {
    let clean: String = url
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect();
    let lower = clean.to_ascii_lowercase();

    lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:text/html")
        || lower.starts_with("data:application/javascript")
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
    for attr in [
        "onerror",
        "onload",
        "onclick",
        "onmouseover",
        "onfocus",
        "onblur",
        "javascript:",
    ] {
        if lower_inner.contains(attr) {
            diagnostics.push(Diagnostic::warning(
                format!("raw HTML tag `<{tag_name_clean}>` contained unsafe attribute `{attr}` and was escaped"),
                Some(line),
            ));
            return escape_html(raw);
        }
    }

    raw.to_string()
}

/// Renders markdown source into sanitized HTML with preserved math formulas.
pub fn render_to_html(source: &str, options: &RenderOptions) -> (String, Vec<Diagnostic>) {
    let parse_opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM;

    let parser = Parser::new_ext(source, parse_opts).into_offset_iter();
    let mut diagnostics = Vec::new();
    let mut sanitized_events: Vec<Event<'static>> = Vec::new();

    let mut in_metadata_block = false;

    for (event, range) in parser {
        let line = source[..range.start].chars().filter(|&c| c == '\n').count() + 1;

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
