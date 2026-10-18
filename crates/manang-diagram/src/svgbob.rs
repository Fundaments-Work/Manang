//! ASCII diagrams through `svgbob`. Any text is a valid diagram, so this
//! engine never reports diagnostics.

use ::svgbob::Settings;

use crate::color::Invert;
use crate::diagnostics::Diagnostic;
use crate::engine::{DiagramEngine, LanguageInfo};
use crate::error::DiagramError;
use crate::svgxml::{self, Rewrite};
use crate::theme::{FONT_MONO, Theme};

static INFO: LanguageInfo = LanguageInfo {
    id: "svgbob",
    name: "ASCII diagram",
    extensions: &[".bob"],
    fences: &["bob", "svgbob"],
    engine: "svgbob 0.7.2",
};

/// ASCII-art diagrams rendered by `svgbob`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SvgbobEngine;

impl SvgbobEngine {
    /// Creates the engine.
    pub fn new() -> Self {
        Self
    }
}

impl DiagramEngine for SvgbobEngine {
    fn info(&self) -> &'static LanguageInfo {
        &INFO
    }

    fn check(&self, _source: &str) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn render(&self, source: &str, theme: Theme) -> Result<String, DiagramError> {
        let p = theme.palette();
        let settings = Settings {
            font_size: 14,
            font_family: FONT_MONO.to_owned(),
            fill_color: p.ink.to_owned(),
            // Open shapes are filled with the background to look hollow.
            background: p.background.unwrap_or(p.base).to_owned(),
            stroke_color: p.ink.to_owned(),
            stroke_width: 1.0,
            include_backdrop: p.background.is_some(),
            ..Settings::default()
        };
        let svg = ::svgbob::to_svg_with_settings(source, &settings);
        let plan = Rewrite {
            background: None,
            square: true,
            palette: Some((p, Invert::All)),
            text_font: None,
        };
        svgxml::rewrite(&svg, &plan).map_err(DiagramError::Render)
    }
}
