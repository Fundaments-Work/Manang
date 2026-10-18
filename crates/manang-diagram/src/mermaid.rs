//! Mermaid through `merman`: headless, deterministic, no JavaScript.
//!
//! Theming goes through Mermaid's own `theme: base` + `themeVariables`
//! config, so the colours are baked into the SVG as plain values that usvg
//! and Typst can read. Square corners and the absence of shadows are
//! enforced by an XML-level postprocessor ([`crate::svgxml`]).

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use merman::svg::{
    MathRenderer, RatexMathRenderer, RenderError as BackendError, RenderResult,
    SvgPostprocessContext, SvgPostprocessor, TextMetrics, TextStyle, WrapMode,
};
use merman::{
    Engine, MermaidConfig, OperationControl, ParseOptions, RenderError, RenderOutput,
    RenderRequest, Renderer, SvgEnvironment, SvgRequest, TerminalDiagnostic,
};
use serde_json::{Value, json};

use crate::color::Invert;
use crate::diagnostics::{Diagnostic, Severity};
use crate::engine::{DiagramEngine, LanguageInfo};
use crate::error::DiagramError;
use crate::svgxml::{self, Rewrite};
use crate::theme::{FONT_SANS, Palette, Theme};

/// The `merman` version this crate is pinned to (`=0.9.0` in Cargo.toml).
pub const MERMAN_VERSION: &str = "0.9.0";

static INFO: LanguageInfo = LanguageInfo {
    id: "mermaid",
    name: "Mermaid",
    extensions: &[".mmd", ".mermaid"],
    fences: &["mermaid"],
    engine: "merman 0.9.0",
};

/// The class a diagram uses to opt into the theme's accent colour.
const ACCENT_CLASS: &str = "accent";

/// Mermaid diagrams rendered by `merman`.
#[derive(Default)]
pub struct MermaidEngine {
    renderers: [OnceLock<Renderer>; 3],
}

impl std::fmt::Debug for MermaidEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MermaidEngine").finish_non_exhaustive()
    }
}

impl MermaidEngine {
    /// Creates the engine. Per-theme renderers are built on first use so the
    /// app does not pay for them at launch.
    pub fn new() -> Self {
        Self::default()
    }

    fn renderer(&self, theme: Theme) -> &Renderer {
        let slot = match theme {
            Theme::Dark => &self.renderers[0],
            Theme::Light => &self.renderers[1],
            Theme::Print => &self.renderers[2],
        };
        slot.get_or_init(|| {
            Renderer::new().with_engine(
                Engine::new().with_site_config(MermaidConfig::from_value(site_config(theme))),
            )
        })
    }

    /// The source handed to merman: the user's text, plus the theme's
    /// `accent` class when a diagram that supports `classDef` uses it without
    /// defining it. Appending keeps every user line number unchanged.
    fn prepared<'a>(&self, source: &'a str, theme: Theme) -> Cow<'a, str> {
        if !uses_accent(source) || source.contains("classDef accent") {
            return Cow::Borrowed(source);
        }
        let Ok(meta) = self.renderer(theme).engine().parse_metadata_sync(source) else {
            return Cow::Borrowed(source);
        };
        let kind = meta.diagram_type.as_str();
        if !["flowchart", "graph", "state", "class"]
            .iter()
            .any(|family| kind.starts_with(family))
        {
            return Cow::Borrowed(source);
        }
        let p = theme.palette();
        let sep = if source.ends_with('\n') { "" } else { "\n" };
        Cow::Owned(format!(
            "{source}{sep}classDef {ACCENT_CLASS} fill:{},stroke:{},color:{}\n",
            p.accent, p.accent, p.accent_ink
        ))
    }
}

impl DiagramEngine for MermaidEngine {
    fn info(&self) -> &'static LanguageInfo {
        &INFO
    }

    fn check(&self, source: &str) -> Vec<Diagnostic> {
        let engine = self.renderer(Theme::Dark).engine();
        let mut out = match engine.parse_diagram_sync(source, ParseOptions::strict()) {
            Ok(Some(_)) => Vec::new(),
            Ok(None) => vec![no_diagram(source)],
            Err(error) => vec![parse_diagnostic(source, &TerminalDiagnostic::from(error))],
        };
        if accent_uses(source) > 1 {
            out.push(Diagnostic::whole_source(
                source,
                Severity::Warning,
                "more than one element uses the accent class; the design allows one accent per diagram",
                Some("manang.diagram.accent".to_owned()),
            ));
        }
        out
    }

    fn render(&self, source: &str, theme: Theme) -> Result<String, DiagramError> {
        let prepared = self.prepared(source, theme);
        let request = svg_request(theme, &svg_id(source, theme));
        let output = self
            .renderer(theme)
            .render(RenderRequest::svg(
                &prepared,
                OperationControl::new(),
                request,
            ))
            .map_err(|error| render_error(source, error))?;
        match output {
            RenderOutput::Svg(Some(svg)) => Ok(svg.into_parts().0),
            _ => Err(no_diagram(source).to_error()),
        }
    }
}

/// A short, stable, document-unique id so two diagrams inlined in one page
/// do not share marker ids or scoped CSS.
fn svg_id(source: &str, theme: Theme) -> String {
    let hash = blake3::hash(format!("{}\0{source}", theme.id()).as_bytes());
    let hex = hash.to_hex();
    format!("mng-{}", hex.get(..12).unwrap_or_default())
}

fn svg_request(theme: Theme, id: &str) -> SvgRequest {
    let mut request = SvgRequest {
        environment: SvgEnvironment::deterministic()
            .with_math_renderer(Arc::new(LocalMath(RatexMathRenderer))),
        ..SvgRequest::default()
    };
    request.options.diagram_id = Some(id.to_owned());
    request.pipeline = Some(merman::svg::SvgPipeline::resvg_safe().with_postprocessor(
        DesignPostprocessor {
            palette: theme.palette(),
        },
    ));
    request
}

/// Mermaid site config for one theme: `base` theme, native SVG labels (no
/// `<foreignObject>`, which usvg and Typst cannot draw), and the palette.
fn site_config(theme: Theme) -> Value {
    let p = theme.palette();
    json!({
        "theme": "base",
        "look": "classic",
        "darkMode": p.dark,
        "fontFamily": FONT_SANS,
        "htmlLabels": false,
        "flowchart": { "htmlLabels": false },
        "themeVariables": theme_variables(p),
    })
}

fn theme_variables(p: &Palette) -> Value {
    // Categorical fills are tonal steps, never new hues.
    let tones = [p.surface, p.surface_hi, p.surface_muted, p.base];
    let flat: &[(&str, &str)] = &[
        ("fontFamily", FONT_SANS),
        ("fontSize", "14px"),
        ("background", p.base),
        ("primaryColor", p.surface),
        ("primaryTextColor", p.ink),
        ("primaryBorderColor", p.line),
        ("secondaryColor", p.surface_hi),
        ("secondaryTextColor", p.ink),
        ("secondaryBorderColor", p.line),
        ("tertiaryColor", p.surface_muted),
        ("tertiaryTextColor", p.ink),
        ("tertiaryBorderColor", p.line),
        ("mainBkg", p.surface),
        ("nodeBorder", p.line),
        ("nodeTextColor", p.ink),
        ("textColor", p.ink),
        ("titleColor", p.ink),
        ("lineColor", p.ink_60),
        ("defaultLinkColor", p.ink_60),
        ("arrowheadColor", p.ink_60),
        ("clusterBkg", p.base),
        ("clusterBorder", p.line),
        ("edgeLabelBackground", p.base),
        ("labelBackground", p.base),
        ("labelTextColor", p.ink),
        ("labelColor", p.ink),
        ("errorBkgColor", p.surface),
        ("errorTextColor", p.ink),
        // Sequence
        ("actorBkg", p.surface),
        ("actorBorder", p.line),
        ("actorTextColor", p.ink),
        ("actorLineColor", p.line),
        ("signalColor", p.ink_60),
        ("signalTextColor", p.ink),
        ("labelBoxBkgColor", p.surface),
        ("labelBoxBorderColor", p.line),
        ("loopTextColor", p.ink),
        ("noteBkgColor", p.surface_hi),
        ("noteTextColor", p.ink),
        ("noteBorderColor", p.line),
        ("activationBkgColor", p.surface_hi),
        ("activationBorderColor", p.line),
        ("sequenceNumberColor", p.accent_ink),
        // State / class / ER
        ("altBackground", p.surface_hi),
        ("compositeBackground", p.base),
        ("compositeTitleBackground", p.surface),
        ("innerEndBackground", p.ink),
        ("specialStateColor", p.ink),
        ("attributeBackgroundColorOdd", p.surface),
        ("attributeBackgroundColorEven", p.surface_hi),
        ("relationColor", p.ink_60),
        ("relationLabelColor", p.ink),
        ("relationLabelBackground", p.base),
        // Gantt
        ("sectionBkgColor", p.surface),
        ("altSectionBkgColor", p.base),
        ("sectionBkgColor2", p.surface_hi),
        ("excludeBkgColor", p.surface_muted),
        ("taskBorderColor", p.line),
        ("taskBkgColor", p.surface_hi),
        ("taskTextColor", p.ink),
        ("taskTextLightColor", p.ink),
        ("taskTextOutsideColor", p.ink),
        ("taskTextClickableColor", p.ink),
        ("taskTextDarkColor", p.ink),
        ("activeTaskBorderColor", p.secondary),
        ("activeTaskBkgColor", p.surface_muted),
        ("doneTaskBkgColor", p.surface_muted),
        ("doneTaskBorderColor", p.line),
        ("critBorderColor", p.accent),
        ("critBkgColor", p.surface_muted),
        ("gridColor", p.line),
        ("todayLineColor", p.accent),
        ("vertLineColor", p.line),
        // Pie
        ("pieTitleTextColor", p.ink),
        ("pieSectionTextColor", p.ink),
        ("pieLegendTextColor", p.ink),
        ("pieStrokeColor", p.line),
        ("pieStrokeWidth", "1px"),
        ("pieOuterStrokeColor", p.line),
        ("pieOuterStrokeWidth", "1px"),
        ("pieOpacity", "1"),
        // Quadrant
        ("quadrant1Fill", p.surface),
        ("quadrant2Fill", p.surface_hi),
        ("quadrant3Fill", p.surface_muted),
        ("quadrant4Fill", p.base),
        ("quadrant1TextFill", p.ink),
        ("quadrant2TextFill", p.ink),
        ("quadrant3TextFill", p.ink),
        ("quadrant4TextFill", p.ink),
        ("quadrantPointFill", p.secondary),
        ("quadrantPointTextFill", p.ink),
        ("quadrantXAxisTextFill", p.ink),
        ("quadrantYAxisTextFill", p.ink),
        ("quadrantInternalBorderStrokeFill", p.line),
        ("quadrantExternalBorderStrokeFill", p.line),
        ("quadrantTitleFill", p.ink),
        // Requirement / C4 / git
        ("requirementBackground", p.surface),
        ("requirementBorderColor", p.line),
        ("requirementTextColor", p.ink),
        ("personBorder", p.line),
        ("personBkg", p.surface),
        ("commitLabelColor", p.ink),
        ("commitLabelBackground", p.surface),
        ("tagLabelColor", p.ink),
        ("tagLabelBackground", p.surface),
        ("tagLabelBorder", p.line),
    ];
    let mut vars = json!({
    "darkMode": p.dark,
    "xyChart": {
        "backgroundColor": "transparent",
        "titleColor": p.ink,
        "xAxisLabelColor": p.ink,
        "xAxisTitleColor": p.ink,
        "xAxisTickColor": p.line,
        "xAxisLineColor": p.line,
        "yAxisLabelColor": p.ink,
        "yAxisTitleColor": p.ink,
        "yAxisTickColor": p.line,
        "yAxisLineColor": p.line,
        "plotColorPalette": format!("{}, {}, {}, {}", p.secondary, p.ink_60, p.line, p.ink),
    },
    "radar": {
        "axisColor": p.line,
        "graticuleColor": p.line,
        "curveOpacity": 0.3,
    },
    });
    if let Some(map) = vars.as_object_mut() {
        for &(key, value) in flat {
            map.insert(key.to_owned(), json!(value));
        }
        // Scales used by mindmap, timeline, kanban, journey sections and
        // git branches, and the twelve pie slices: cycle through the tones.
        for i in 0..12 {
            let tone = tones[i % tones.len()];
            map.insert(format!("cScale{i}"), json!(tone));
            map.insert(format!("cScaleLabel{i}"), json!(p.ink));
            map.insert(format!("cScaleInv{i}"), json!(p.ink));
            map.insert(format!("cScalePeer{i}"), json!(p.line));
            map.insert(format!("pie{}", i + 1), json!(tone));
            map.insert(format!("fillType{i}"), json!(tone));
        }
        for i in 0..8 {
            map.insert(
                format!("git{i}"),
                json!(if i == 0 { p.secondary } else { p.ink_60 }),
            );
            map.insert(format!("gitBranchLabel{i}"), json!(p.ink));
            map.insert(format!("gitInv{i}"), json!(p.ink));
        }
    }
    vars
}

/// Whether the source mentions the accent class at all.
fn uses_accent(source: &str) -> bool {
    accent_uses(source) > 0
}

/// How many elements the source assigns the accent class to, through
/// `:::accent` or `class a,b accent`.
fn accent_uses(source: &str) -> usize {
    let inline = source.matches(":::accent").count();
    let statements: usize = source
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("class "))
        .filter_map(|rest| {
            let mut words = rest.split_whitespace();
            let ids = words.next()?;
            (words.next() == Some(ACCENT_CLASS)).then(|| ids.split(',').count())
        })
        .sum();
    inline + statements
}

fn no_diagram(source: &str) -> Diagnostic {
    Diagnostic::whole_source(
        source,
        Severity::Error,
        "no Mermaid diagram detected",
        Some("merman.parse.no_diagram_detected".to_owned()),
    )
}

fn parse_diagnostic(source: &str, diag: &TerminalDiagnostic) -> Diagnostic {
    let details = diag.terminal_diagnostic_details();
    let message = diag.terminal_safe_message();
    let code = Some(details.code);
    let span = details
        .span
        .map(|span| (span.start, span.end))
        .or_else(|| trailing_offset(&message).map(|at| (at, at)));
    match span {
        Some((start, end)) => {
            Diagnostic::from_byte_span(source, start, end, Severity::Error, message, code)
        }
        None => Diagnostic::whole_source(source, Severity::Error, message, code),
    }
}

/// Some merman lexer errors carry no span but end in `at <byte offset>`
/// (e.g. "Unexpected character at 33"); recover the location from it.
fn trailing_offset(message: &str) -> Option<usize> {
    let (_, digits) = message.trim_end().rsplit_once(" at ")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn render_error(source: &str, error: RenderError) -> DiagramError {
    match error {
        RenderError::Parse(diag) => parse_diagnostic(source, &diag).to_error(),
        RenderError::NoDiagram => no_diagram(source).to_error(),
        other => DiagramError::Render(other.to_string()),
    }
}

/// RaTeX math with only its bundled KaTeX fonts. RaTeX falls back to
/// scanning system fonts for CJK and emoji glyphs, which would make output
/// depend on the machine and touch the filesystem; such formulas are
/// declined instead.
#[derive(Debug)]
struct LocalMath(RatexMathRenderer);

/// Characters RaTeX would look up in system fonts.
fn needs_system_font(text: &str) -> bool {
    text.chars().any(|c| {
        let c = u32::from(c);
        (0x2600..=0x27BF).contains(&c) || c >= 0x2E80
    })
}

impl MathRenderer for LocalMath {
    fn render_html_label(&self, text: &str, config: &MermaidConfig) -> Option<String> {
        if needs_system_font(text) {
            return None;
        }
        self.0.render_html_label(text, config)
    }

    fn render_sequence_html_label(&self, text: &str, config: &MermaidConfig) -> Option<String> {
        if needs_system_font(text) {
            return None;
        }
        self.0.render_sequence_html_label(text, config)
    }

    fn measure_html_label(
        &self,
        text: &str,
        config: &MermaidConfig,
        style: &TextStyle,
        max_width_px: Option<f64>,
        wrap_mode: WrapMode,
    ) -> Option<TextMetrics> {
        if needs_system_font(text) {
            return None;
        }
        self.0
            .measure_html_label(text, config, style, max_width_px, wrap_mode)
    }

    fn measure_sequence_html_label(
        &self,
        text: &str,
        config: &MermaidConfig,
    ) -> Option<TextMetrics> {
        if needs_system_font(text) {
            return None;
        }
        self.0.measure_sequence_html_label(text, config)
    }
}

/// Square corners, no shadows, palette-only colours and the print
/// background, applied to the SVG tree before merman's resvg-safe finalisation.
struct DesignPostprocessor {
    palette: &'static Palette,
}

impl SvgPostprocessor for DesignPostprocessor {
    fn name(&self) -> &'static str {
        "manang-design"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        _ctx: &SvgPostprocessContext<'_>,
    ) -> RenderResult<Cow<'a, str>> {
        let plan = Rewrite {
            background: self.palette.background,
            square: true,
            palette: Some((self.palette, Invert::Literals)),
            text_font: None,
        };
        svgxml::rewrite(&svg, &plan)
            .map(Cow::Owned)
            .map_err(|message| BackendError::SvgPostprocess {
                pass: "manang-design".to_owned(),
                message,
            })
    }
}
