//! Pikchr through `pikru`, a pure-Rust port of the C reference.
//!
//! Pikchr has no theming: it draws black on nothing, for a light page. Its
//! colours are mapped onto the palette's tonal ramp by the XML rewriter
//! (lightness inverted on the dark theme), and text gets the design font.

use pest::Parser;
use pest::error::InputLocation;
use pikru::{PikchrParser, RenderOptions, Rule};

use crate::color::Invert;
use crate::diagnostics::{Diagnostic, Severity};
use crate::engine::{DiagramEngine, LanguageInfo};
use crate::error::DiagramError;
use crate::svgxml::{self, Rewrite};
use crate::theme::{FONT_SANS, Theme};

static INFO: LanguageInfo = LanguageInfo {
    id: "pikchr",
    name: "Pikchr",
    extensions: &[".pikchr"],
    fences: &["pikchr"],
    engine: "pikru 1.0.1",
};

/// Pikchr diagrams rendered by `pikru`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PikchrEngine;

impl PikchrEngine {
    /// Creates the engine.
    pub fn new() -> Self {
        Self
    }
}

/// Syntax errors carry a byte location from the grammar; report it.
fn syntax_diagnostic(source: &str) -> Option<Diagnostic> {
    let error = PikchrParser::parse(Rule::program, source).err()?;
    let (start, end) = match error.location {
        InputLocation::Pos(at) => (at, at),
        InputLocation::Span((start, end)) => (start, end),
    };
    Some(Diagnostic::from_byte_span(
        source,
        start,
        end,
        Severity::Error,
        error.variant.message().into_owned(),
        Some("pikchr.parse".to_owned()),
    ))
}

/// Errors after parsing (undefined names, bad macros) carry no location.
fn render_raw(source: &str) -> Result<String, Diagnostic> {
    if let Some(diag) = syntax_diagnostic(source) {
        return Err(diag);
    }
    pikru::pikchr_with_options(source, &RenderOptions::default()).map_err(|report| {
        Diagnostic::whole_source(
            source,
            Severity::Error,
            report.to_string(),
            Some("pikchr.render".to_owned()),
        )
    })
}

impl DiagramEngine for PikchrEngine {
    fn info(&self) -> &'static LanguageInfo {
        &INFO
    }

    fn check(&self, source: &str) -> Vec<Diagnostic> {
        render_raw(source).err().into_iter().collect()
    }

    fn render(&self, source: &str, theme: Theme) -> Result<String, DiagramError> {
        let svg = render_raw(source).map_err(|d| d.to_error())?;
        let p = theme.palette();
        svgxml::rewrite(
            &svg,
            &Rewrite {
                background: p.background,
                square: true,
                palette: Some((p, Invert::All)),
                text_font: Some(FONT_SANS),
            },
        )
        .map_err(DiagramError::Render)
    }
}
