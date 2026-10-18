//! One formula to vector SVG, through Typst itself (`typst.math-to-svg`).
//!
//! The formula becomes the only content of an in-memory, one-line document
//! with an automatic page size and no margin, so the page is the formula's
//! bounding box. The world has exactly one file, no packages, no clock and
//! only the embedded fonts (never the system's), so the same formula gives
//! the same bytes on every machine. Typst's SVG export draws glyphs as
//! outlines, so the result has no font dependency. Its text paint is a
//! placeholder colour that an XML-aware pass then replaces (default
//! `currentColor`); the root is normalised to `viewBox="0 0 w h"` in scene
//! units (Typst points = CSS pixels here).

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use serde::Serialize;
use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World, WorldExt};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;
use typst_svg::SvgOptions;

use crate::error::TypstError;

/// Largest formula source accepted, in bytes.
pub const MAX_MATH_SOURCE: usize = 16 * 1024;

const MAIN_PATH: &str = "/manang-math/main.typ";

/// Not a colour anyone draws with: marks the text paint for replacement.
const PLACEHOLDER: &str = "#010203";

/// A rendered formula.
#[derive(Debug, Clone, PartialEq)]
pub struct MathSvg {
    /// The SVG document.
    pub svg: String,
    /// Width in scene units.
    pub width: f64,
    /// Height in scene units.
    pub height: f64,
}

/// A formula error, positioned inside the formula source.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MathDiagnostic {
    /// What is wrong.
    pub message: String,
    /// 1-based line inside the formula.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// 1-based column in characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    /// Length in characters of the offending text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    /// Typst's hints.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl MathDiagnostic {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
            length: None,
            hints: Vec::new(),
        }
    }

    /// `[math_error] {json}`, the form plugin errors carry.
    pub fn to_tagged(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json) => format!("[math_error] {json}"),
            Err(_) => format!("[math_error] {{\"message\":{:?}}}", self.message),
        }
    }
}

fn math_error(diag: MathDiagnostic) -> TypstError {
    TypstError::Math(diag.to_tagged())
}

fn file_id() -> Result<FileId, TypstError> {
    let vpath = VirtualPath::new(MAIN_PATH).map_err(|e| TypstError::BadPath {
        path: MAIN_PATH.to_owned(),
        reason: e.to_string(),
    })?;
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

struct MathWorld<'a> {
    library: &'a LazyHash<Library>,
    fonts: &'a FontStore,
    main: Source,
}

impl World for MathWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotSource)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            Ok(Bytes::from_string(self.main.text().to_owned()))
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    /// No clock: the SVG must be byte-identical for the same formula.
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn char_offset(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].chars().count()
}

/// Where inside the formula a diagnostic points, if it points there.
fn locate(
    world: &MathWorld<'_>,
    diag: &SourceDiagnostic,
    formula_start: usize,
    formula: &str,
) -> MathDiagnostic {
    let mut out = MathDiagnostic::plain(diag.message.to_string());
    out.hints = diag.hints.iter().map(|h| h.v.to_string()).collect();
    if let Some(range) = world.range(diag.span)
        && range.start >= formula_start
        && range.start <= formula_start + formula.len()
    {
        let inner = range.start - formula_start;
        let offset = char_offset(formula, inner);
        let (line, column) = line_column(formula, offset);
        out.line = Some(line);
        out.column = Some(column);
        let end = range.end.min(formula_start + formula.len());
        let length = char_offset(formula, end.saturating_sub(formula_start)).saturating_sub(offset);
        out.length = (length > 0).then_some(length);
    }
    out
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
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

fn join(errors: impl Iterator<Item = String>) -> String {
    errors.collect::<Vec<_>>().join("; ")
}

/// Renders `source` (Typst math) at `size` scene units. `color` replaces the
/// text paint: `currentColor` or a `#rrggbb` literal.
pub(crate) fn convert(
    library: &LazyHash<Library>,
    fonts: &FontStore,
    source: &str,
    size: f64,
    color: &str,
) -> Result<MathSvg, TypstError> {
    if source.trim().is_empty() {
        return Err(math_error(MathDiagnostic::plain("the formula is empty")));
    }
    if source.len() > MAX_MATH_SOURCE {
        return Err(math_error(MathDiagnostic::plain(format!(
            "the formula is {} bytes; the limit is {MAX_MATH_SOURCE}",
            source.len()
        ))));
    }
    if !(size.is_finite() && size > 0.0 && size <= 1024.0) {
        return Err(math_error(MathDiagnostic::plain(
            "the size must be between 0 and 1024",
        )));
    }
    if !valid_colour(color) {
        return Err(TypstError::Svg(format!(
            "`color` must be currentColor or #rrggbb, not `{color}`"
        )));
    }
    let prefix = format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n\
         #set text(size: {size}pt, fill: rgb(\"{PLACEHOLDER}\"))\n$ "
    );
    let text = format!("{prefix}{source} $\n");
    let world = MathWorld {
        library,
        fonts,
        main: Source::new(file_id()?, text),
    };
    let compiled = typst::compile::<PagedDocument>(&world);
    let document = match compiled.output {
        Ok(document) => document,
        Err(errors) => {
            let first = errors
                .iter()
                .find(|d| d.severity == Severity::Error)
                .or_else(|| errors.first());
            let mut diag = match first {
                Some(d) => locate(&world, d, prefix.len(), source),
                None => MathDiagnostic::plain("the formula did not compile"),
            };
            if errors.len() > 1 {
                diag.hints.push(format!(
                    "{} more error(s): {}",
                    errors.len() - 1,
                    join(errors.iter().skip(1).take(3).map(|e| e.message.to_string()))
                ));
            }
            return Err(math_error(diag));
        }
    };
    let Some(page) = document.pages().first() else {
        return Err(math_error(MathDiagnostic::plain(
            "the formula produced no output",
        )));
    };
    let raw = typst_svg::svg(page, &SvgOptions::default());
    let size = page.frame.size();
    let (width, height) = (ceil1(size.x.to_pt()), ceil1(size.y.to_pt()));
    if width <= 0.0 || height <= 0.0 {
        return Err(math_error(MathDiagnostic::plain(
            "the formula has no visible content",
        )));
    }
    let svg = recolour(&raw, color, width, height)
        .map_err(|e| TypstError::Svg(format!("the renderer produced invalid SVG: {e}")))?;
    Ok(MathSvg { svg, width, height })
}

fn ceil1(v: f64) -> f64 {
    (v * 10.0).ceil() / 10.0
}

fn valid_colour(color: &str) -> bool {
    color == "currentColor"
        || (color.len() == 7
            && color.starts_with('#')
            && color[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

fn num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// Replaces the placeholder paint with `color` in `fill` and `stroke`
/// attributes and sets the root's `viewBox`, `width` and `height`, by
/// tokenising the document with `quick-xml`; text content is not touched.
fn recolour(svg: &str, color: &str, width: f64, height: f64) -> Result<String, String> {
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
            if is_root && matches!(key, "width" | "height" | "viewBox") {
                continue;
            }
            // SVG 2 `href` needs no namespace declaration, so the formula can
            // be nested in any document without declaring `xlink` there.
            if key == "xmlns:xlink" || key == "xmlns:h5" {
                continue;
            }
            let key = if key == "xlink:href" { "href" } else { key };
            let value = attr
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|e| e.to_string())?;
            if (key == "fill" || key == "stroke") && value.eq_ignore_ascii_case(PLACEHOLDER) {
                out.push_attribute((key, color));
            } else {
                out.push_attribute((key, value.as_ref()));
            }
        }
        if is_root {
            out.push_attribute((
                "viewBox",
                format!("0 0 {} {}", num(width), num(height)).as_str(),
            ));
            out.push_attribute(("width", num(width).as_str()));
            out.push_attribute(("height", num(height).as_str()));
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
