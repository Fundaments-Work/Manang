//! Conversion from CommonMark/Markdown constructs to Typst markup.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, MetadataBlockKind, Parser, Tag, TagEnd};

use crate::diagnostics::Diagnostic;
use crate::diagrams::{DiagramFence, OpenFence};
use crate::html::parse_options;

/// A diagram fence as a figure, or as the code block it was.
fn push_fence(
    typst: &mut String,
    diagnostics: &mut Vec<Diagnostic>,
    fence: &DiagramFence,
    figure: Option<FenceFigure>,
) {
    match figure {
        Some(FenceFigure::Image(path)) => {
            typst.push_str(&format!(
                "#figure(image(\"{}\"))\n\n",
                escape_typst_string(&path)
            ));
        }
        other => {
            if let Some(FenceFigure::Keep(why)) = other {
                diagnostics.push(Diagnostic::warning(why, Some(fence.line)));
            }
            let ticks = longest_backtick_run(&fence.source).max(2) + 1;
            let fence_marks = "`".repeat(ticks);
            typst.push_str(&fence_marks);
            typst.push_str(&fence.language);
            typst.push('\n');
            typst.push_str(&fence.source);
            if !fence.source.ends_with('\n') {
                typst.push('\n');
            }
            typst.push_str(&fence_marks);
            typst.push_str("\n\n");
        }
    }
}

fn longest_backtick_run(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

fn escape_typst_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_typst_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '#' => out.push_str("\\#"),
            '$' => out.push_str("\\$"),
            '@' => out.push_str("\\@"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            _ => out.push(ch),
        }
    }
    out
}

/// What a diagram fence becomes in Typst.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FenceFigure {
    /// `#figure(image("<path>"))`: the rendered picture's path as Typst
    /// resolves it (`/...` is relative to the project root).
    Image(String),
    /// Keep the fence as a code block and report why as a warning.
    Keep(String),
}

/// Converts markdown source text into Typst markup, logging diagnostics for
/// unsupported constructs. Diagram fences stay code blocks; see
/// [`markdown_to_typst_with`] to turn them into figures.
pub fn markdown_to_typst(source: &str) -> (String, Vec<Diagnostic>) {
    markdown_to_typst_with(source, 0, &mut |_| None)
}

/// [`markdown_to_typst`] for text that starts after `line_offset` lines of
/// its file, asking `figure` what each diagram fence becomes (`None` keeps it
/// as a code block silently).
pub fn markdown_to_typst_with(
    source: &str,
    line_offset: usize,
    figure: &mut dyn FnMut(&DiagramFence) -> Option<FenceFigure>,
) -> (String, Vec<Diagnostic>) {
    let parser = Parser::new_ext(source, parse_options()).into_offset_iter();
    let mut typst = String::new();
    let mut diagnostics = Vec::new();
    let mut diagram: Option<OpenFence> = None;

    let mut in_code_block = false;
    let mut in_metadata_block = false;
    let mut in_table = false;
    let mut in_table_head = false;
    let mut list_depth: usize = 0;
    let mut ordered_stack: Vec<bool> = Vec::new();

    for (event, range) in parser {
        let line = line_offset + source[..range.start].chars().filter(|&c| c == '\n').count() + 1;

        if let Some(fence) = diagram.as_mut() {
            match event {
                Event::Text(text) => fence.source.push_str(&text),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(fence) = diagram.take() {
                        let fence = fence.finish();
                        push_fence(&mut typst, &mut diagnostics, &fence, figure(&fence));
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
            Event::Start(Tag::MetadataBlock(kind)) => {
                in_metadata_block = true;
                let kind_str = match kind {
                    MetadataBlockKind::YamlStyle => "YAML",
                    MetadataBlockKind::PlusesStyle => "TOML",
                };
                diagnostics.push(Diagnostic::info(
                    format!("{kind_str} frontmatter was extracted and omitted from Typst body"),
                    Some(line),
                ));
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                in_metadata_block = false;
            }
            _ if in_metadata_block => {
                // Ignore metadata inside Typst document body
            }

            Event::Start(Tag::Heading { level, .. }) => {
                let eq_count = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                for _ in 0..eq_count {
                    typst.push('=');
                }
                typst.push(' ');
            }
            Event::End(TagEnd::Heading(_)) => {
                typst.push_str("\n\n");
            }

            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if !in_table {
                    typst.push_str("\n\n");
                }
            }

            Event::Start(Tag::Emphasis) => typst.push('_'),
            Event::End(TagEnd::Emphasis) => typst.push('_'),

            Event::Start(Tag::Strong) => typst.push('*'),
            Event::End(TagEnd::Strong) => typst.push('*'),

            Event::Start(Tag::Strikethrough) => typst.push_str("#strike["),
            Event::End(TagEnd::Strikethrough) => typst.push(']'),

            Event::Start(Tag::BlockQuote(_)) => {
                typst.push_str("#quote(block: true)[\n");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                typst.push_str("]\n\n");
            }

            Event::Start(Tag::List(first_num)) => {
                list_depth += 1;
                ordered_stack.push(first_num.is_some());
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                ordered_stack.pop();
                if list_depth == 0 {
                    typst.push('\n');
                }
            }

            Event::Start(Tag::Item) => {
                let is_ordered = ordered_stack.last().copied().unwrap_or(false);
                let indent = "  ".repeat(list_depth.saturating_sub(1));
                typst.push_str(&indent);
                if is_ordered {
                    typst.push_str("+ ");
                } else {
                    typst.push_str("- ");
                }
            }
            Event::End(TagEnd::Item) => {
                typst.push('\n');
            }

            Event::TaskListMarker(checked) => {
                if checked {
                    typst.push_str("[x] ");
                } else {
                    typst.push_str("[ ] ");
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                typst.push_str("```");
                if let CodeBlockKind::Fenced(lang) = kind {
                    typst.push_str(&lang);
                }
                typst.push('\n');
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if !typst.ends_with('\n') {
                    typst.push('\n');
                }
                typst.push_str("```\n\n");
            }

            Event::Code(code) => {
                typst.push('`');
                typst.push_str(&code);
                typst.push('`');
            }

            Event::InlineMath(math) => {
                typst.push('$');
                typst.push_str(math.trim());
                typst.push('$');
            }

            Event::DisplayMath(math) => {
                typst.push_str("$ ");
                typst.push_str(math.trim());
                typst.push_str(" $\n\n");
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                typst.push_str(&format!("#link(\"{}\")[", escape_typst_string(&dest_url)));
            }
            Event::End(TagEnd::Link) => {
                typst.push(']');
            }

            Event::Start(Tag::Image { dest_url, .. }) => {
                typst.push_str(&format!("#image(\"{}\")", escape_typst_string(&dest_url)));
            }
            Event::End(TagEnd::Image) => {}

            Event::Start(Tag::Table(alignments)) => {
                in_table = true;
                let columns = alignments.len();
                typst.push_str(&format!("#table(\n  columns: {columns},\n"));
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                typst.push_str(")\n\n");
            }

            Event::Start(Tag::TableHead) => {
                in_table_head = true;
            }
            Event::End(TagEnd::TableHead) => {
                in_table_head = false;
            }

            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {}

            Event::Start(Tag::TableCell) => {
                typst.push_str("  [");
                if in_table_head {
                    typst.push('*');
                }
            }
            Event::End(TagEnd::TableCell) => {
                if in_table_head {
                    typst.push('*');
                }
                typst.push_str("],\n");
            }

            Event::FootnoteReference(name) => {
                typst.push_str(&format!("#footnote[{name}]"));
            }

            Event::Rule => {
                typst.push_str("#line(length: 100%)\n\n");
            }

            Event::SoftBreak => {
                if in_code_block {
                    typst.push('\n');
                } else {
                    typst.push(' ');
                }
            }

            Event::HardBreak => {
                typst.push_str(" \\\n");
            }

            Event::Text(text) => {
                if in_code_block {
                    typst.push_str(&text);
                } else {
                    typst.push_str(&escape_typst_text(&text));
                }
            }

            Event::Html(raw) | Event::InlineHtml(raw) => {
                diagnostics.push(Diagnostic::warning(
                    format!("raw HTML `{raw}` is unsupported in Typst and was omitted"),
                    Some(line),
                ));
            }

            other => {
                diagnostics.push(Diagnostic::info(
                    format!("unsupported markdown construct `{other:?}` was omitted"),
                    Some(line),
                ));
            }
        }
    }

    (typst.trim().to_string(), diagnostics)
}
