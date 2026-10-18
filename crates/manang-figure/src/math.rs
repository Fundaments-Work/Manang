//! Formula rendering: Typst and LaTeX math to vector SVG (ADR 0012,
//! amendment 2026-10-18).
//!
//! [`render_math`] is a pure function of `(language, source, size)`. The SVG
//! it returns has no text and no font dependency: glyphs are outline paths,
//! every paint is `currentColor` (so the node's colour is one CSS `color` or
//! SVG `color` attribute away), and the root carries a `viewBox` of
//! `0 0 width height` in scene units at the requested size.
//!
//! * **LaTeX** is rendered in process by RaTeX (MIT; pure Rust; KaTeX fonts
//!   embedded in the binary). RaTeX would scan the system's fonts for CJK and
//!   emoji glyphs, which would make the output depend on the machine, so
//!   sources with such characters are declined with an error (the same rule
//!   `manang-diagram` applies to Mermaid math).
//! * **Typst** is rendered by the Typst compiler, which lives in
//!   `manang-typst` (too heavy for this crate and for the browser build), so
//!   the caller passes it in as a function: the plugin routes it through
//!   `typst.math-to-svg` with `Context::invoke`.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use serde::{Deserialize, Serialize};

use crate::scene::MathLanguage;

/// Largest formula source accepted, in bytes.
pub const MAX_MATH_SOURCE: usize = 16 * 1024;

/// A rendered formula.
#[derive(Debug, Clone, PartialEq)]
pub struct MathSvg {
    /// The SVG document (paths only, `currentColor`).
    pub svg: String,
    /// Width in scene units at the requested size.
    pub width: f64,
    /// Height in scene units at the requested size.
    pub height: f64,
}

/// Why a formula did not render, with a position inside the formula.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathError {
    /// What is wrong.
    pub message: String,
    /// 1-based line inside the formula source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// 1-based column (characters) inside that line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    /// Length in characters of the offending text, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    /// Hints from the compiler, when it gave any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl MathError {
    /// An error without a position.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
            length: None,
            hints: Vec::new(),
        }
    }

    /// Reads the `[math_error] {json}` form back (it travels inside other
    /// plugins' error strings); `None` when `text` carries none.
    pub fn from_tagged(text: &str) -> Option<Self> {
        let json = text.split_once("[math_error] ")?.1;
        let end = json.rfind('}')?;
        serde_json::from_str(&json[..=end]).ok()
    }

    /// An error at a character offset in `source`.
    pub fn at(message: impl Into<String>, source: &str, offset: usize, length: usize) -> Self {
        let (line, column) = line_column(source, offset);
        Self {
            message: message.into(),
            line: Some(line),
            column: Some(column),
            length: (length > 0).then_some(length),
            hints: Vec::new(),
        }
    }

    /// The error as plugin errors carry it: `[math_error] {json}`.
    pub fn to_tagged(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json) => format!("[math_error] {json}"),
            Err(_) => format!("[math_error] {{\"message\":{:?}}}", self.message),
        }
    }
}

impl std::fmt::Display for MathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "{} (line {l}, column {c})", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for MathError {}

/// 1-based line and column (in characters) of character offset `offset`.
pub fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column) = (1, 1);
    for (i, c) in source.chars().enumerate() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn check_source(source: &str, size: f64) -> Result<(), MathError> {
    if source.trim().is_empty() {
        return Err(MathError::new("the formula is empty"));
    }
    if source.len() > MAX_MATH_SOURCE {
        return Err(MathError::new(format!(
            "the formula is {} bytes; the limit is {MAX_MATH_SOURCE}",
            source.len()
        )));
    }
    if !(size.is_finite() && size > 0.0 && size <= 1024.0) {
        return Err(MathError::new("the size must be between 0 and 1024"));
    }
    Ok(())
}

/// Renders `source` in `language` at `size`. `typst` renders Typst math
/// (see the module docs); it is not called for LaTeX.
pub fn render_math(
    language: MathLanguage,
    source: &str,
    size: f64,
    typst: impl FnOnce(&str, f64) -> Result<MathSvg, MathError>,
) -> Result<MathSvg, MathError> {
    check_source(source, size)?;
    match language {
        MathLanguage::Latex => render_latex(source, size),
        MathLanguage::Typst => typst(source, size),
    }
}

/// Characters RaTeX would look up in system fonts (CJK, emoji, symbols).
fn needs_system_font(c: char) -> bool {
    let c = u32::from(c);
    (0x2600..=0x27BF).contains(&c) || c >= 0x2E80
}

/// Em values are rounded like the rest of the figure so the SVG is stable.
fn num(value: f64) -> String {
    crate::geometry::format_coord(value)
}

/// Renders LaTeX math with RaTeX at `size`.
pub fn render_latex(source: &str, size: f64) -> Result<MathSvg, MathError> {
    check_source(source, size)?;
    if let Some((offset, c)) = source
        .chars()
        .enumerate()
        .find(|(_, c)| needs_system_font(*c))
    {
        return Err(MathError::at(
            format!(
                "`{c}` is not in the bundled math fonts; Manang renders formulas without system fonts, so the formula cannot use it"
            ),
            source,
            offset,
            1,
        ));
    }
    let ast = ratex_parser::parse(source).map_err(|e| match e.loc {
        Some(loc) => MathError::at(
            e.message.clone(),
            source,
            loc.start,
            loc.end.saturating_sub(loc.start),
        ),
        None => MathError::new(e.message.clone()),
    })?;
    let options = ratex_layout::LayoutOptions::default()
        .with_style(ratex_types::MathStyle::Display)
        .with_color(ratex_types::Color::BLACK);
    let layout = ratex_layout::layout(&ast, &options);
    ratex_layout::validate_layout(&layout).map_err(MathError::new)?;
    let list = ratex_layout::to_display_list(&layout);
    let width = (list.width.max(0.0) * size * 10.0).ceil() / 10.0;
    let height = (list.total_height().max(0.0) * size * 10.0).ceil() / 10.0;
    if width <= 0.0 || height <= 0.0 {
        return Err(MathError::new("the formula has no visible content"));
    }
    let raw = ratex_svg::render_to_svg_with_color_syntax(
        &list,
        &ratex_svg::SvgOptions {
            font_size: size,
            padding: 0.0,
            stroke_width: 0.04 * size,
            embed_glyphs: true,
            font_dir: String::new(),
        },
        ratex_svg::SvgColorSyntax::Rgb,
    );
    let svg = recolour_svg(&raw, is_black, "currentColor", Some((width, height)))
        .map_err(|e| MathError::new(format!("the renderer produced invalid SVG: {e}")))?;
    Ok(MathSvg { svg, width, height })
}

fn is_black(value: &str) -> bool {
    let compact: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        compact.as_str(),
        "rgb(0,0,0)" | "rgba(0,0,0,1)" | "#000" | "#000000" | "black"
    )
}

/// Rewrites paint attributes (`fill`, `stroke`) whose value satisfies
/// `matches` to `to`, and with `size` sets the root's `viewBox`, `width` and
/// `height`. It tokenises the document with `quick-xml`; text content is
/// never touched.
pub fn recolour_svg(
    svg: &str,
    matches: impl Fn(&str) -> bool,
    to: &str,
    size: Option<(f64, f64)>,
) -> Result<String, String> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(svg.len()));
    let mut root_seen = false;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let (start, empty) = match &event {
            Event::Eof => break,
            Event::Start(s) => (Some(s), false),
            Event::Empty(s) => (Some(s), true),
            _ => (None, false),
        };
        let Some(start) = start else {
            writer.write_event(event).map_err(|e| e.to_string())?;
            continue;
        };
        let is_root = !root_seen && start.name().as_ref() == "svg";
        root_seen = true;
        let mut out = BytesStart::new(start.name().as_ref().to_owned());
        for attr in start.attributes() {
            let attr = attr.map_err(|e| e.to_string())?;
            let key = attr.key.as_ref();
            if is_root && size.is_some() && matches!(key, "width" | "height" | "viewBox") {
                continue;
            }
            let value = attr
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|e| e.to_string())?;
            if (key == "fill" || key == "stroke") && matches(&value) {
                out.push_attribute((key, to));
            } else {
                out.push_attribute((key, value.as_ref()));
            }
        }
        if is_root && let Some((w, h)) = size {
            out.push_attribute(("viewBox", format!("0 0 {} {}", num(w), num(h)).as_str()));
            out.push_attribute(("width", num(w).as_str()));
            out.push_attribute(("height", num(h).as_str()));
        }
        let rewritten = if empty {
            Event::Empty(out)
        } else {
            Event::Start(out)
        };
        writer.write_event(rewritten).map_err(|e| e.to_string())?;
    }
    String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())
}

/// The `width` and `height` of an SVG's root, from its `viewBox`.
pub fn svg_size(svg: &str) -> Option<(f64, f64)> {
    let mut reader = Reader::from_str(svg);
    loop {
        match reader.read_event().ok()? {
            Event::Start(s) | Event::Empty(s) if s.name().as_ref() == "svg" => {
                let attr = s
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "viewBox")?;
                let value = attr
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()?;
                let nums: Vec<f64> = value
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|p| !p.is_empty())
                    .filter_map(|p| p.parse().ok())
                    .collect();
                return match nums.as_slice() {
                    [_, _, w, h] => Some((*w, *h)),
                    _ => None,
                };
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latex_renders_paths_in_currentcolor() {
        let m = render_latex(r"\int_0^1 x^2\,dx = \frac13", 24.0).expect("renders");
        assert!(m.width > 20.0 && m.height > 10.0, "{m:?}");
        assert!(m.svg.contains("currentColor"), "{}", m.svg);
        assert!(!m.svg.contains("<text"), "no text, only outlines");
        assert!(!m.svg.contains("rgb("), "{}", m.svg);
        assert_eq!(svg_size(&m.svg), Some((m.width, m.height)));
        // Deterministic.
        assert_eq!(
            m,
            render_latex(r"\int_0^1 x^2\,dx = \frac13", 24.0).unwrap()
        );
    }

    #[test]
    fn latex_errors_carry_a_position() {
        let err = render_latex("x^2 +\n\\frac{1}", 20.0).expect_err("unbalanced");
        assert!(err.line.is_some(), "{err:?}");
        assert!(err.to_tagged().starts_with("[math_error] {"));
        let err = render_latex(r"\nosuchcommand", 20.0).expect_err("unknown");
        assert!(err.message.to_lowercase().contains("undefined") || !err.message.is_empty());
        assert!(render_latex("", 20.0).is_err());
    }

    #[test]
    fn latex_declines_what_needs_system_fonts() {
        let err = render_latex("x = 日本", 20.0).expect_err("cjk");
        assert_eq!((err.line, err.column), (Some(1), Some(5)));
    }

    #[test]
    fn size_scales_the_box() {
        let a = render_latex("x^2", 16.0).expect("renders");
        let b = render_latex("x^2", 32.0).expect("renders");
        assert!((b.width / a.width - 2.0).abs() < 0.05);
    }

    #[test]
    fn line_column_counts_characters() {
        assert_eq!(line_column("ab\ncd", 0), (1, 1));
        assert_eq!(line_column("ab\ncd", 4), (2, 2));
    }

    #[test]
    fn recolour_edits_attributes_only() {
        let svg =
            "<svg width=\"1\"><path fill=\"#010203\" d=\"M0 0\"/><g stroke=\"#010203\"/></svg>";
        let out = recolour_svg(svg, |v| v == "#010203", "currentColor", Some((3.0, 4.0))).unwrap();
        assert!(out.contains("fill=\"currentColor\"") && out.contains("stroke=\"currentColor\""));
        assert!(out.contains("viewBox=\"0 0 3 4\"") && !out.contains("width=\"1\""));
    }
}
