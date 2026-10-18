//! Fundaments.work palettes for rendered diagrams.
//!
//! Every colour here is a design token from `src/app.css` (or the light
//! accent fixed by ADR 0002). No other hex code may appear in a diagram.

use std::str::FromStr;

use crate::error::DiagramError;

/// Text font for diagram labels.
pub const FONT_SANS: &str = "DM Sans, sans-serif";
/// Font for monospaced diagrams (ASCII art).
pub const FONT_MONO: &str = "DM Mono, monospace";

/// Which surroundings the SVG is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// The dark workspace: transparent background, light ink.
    Dark,
    /// The light workspace: transparent background, dark ink.
    Light,
    /// Paper: white background, black ink. For committed files and PDF.
    Print,
}

impl Theme {
    /// Every theme, in a fixed order.
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::Print];

    /// The id used on the command boundary.
    pub fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Print => "print",
        }
    }

    /// The colours this theme draws with.
    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Dark => &DARK,
            Self::Light => &LIGHT,
            Self::Print => &PRINT,
        }
    }
}

impl FromStr for Theme {
    type Err = DiagramError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            "print" => Ok(Self::Print),
            other => Err(DiagramError::UnknownTheme(other.to_owned())),
        }
    }
}

/// The colours of one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Painted page background, or `None` for transparent.
    pub background: Option<&'static str>,
    /// The workspace base the diagram sits on (used where Mermaid needs an
    /// opaque "background" colour, e.g. behind edge labels).
    pub base: &'static str,
    /// Node fill: one tonal step above the base.
    pub surface: &'static str,
    /// Second tonal step (clusters, alternating rows).
    pub surface_hi: &'static str,
    /// Third tonal step.
    pub surface_muted: &'static str,
    /// 1px hairlines and node borders.
    pub line: &'static str,
    /// Primary text.
    pub ink: &'static str,
    /// Secondary text.
    pub ink_60: &'static str,
    /// The one accent per diagram (`classDef accent`).
    pub accent: &'static str,
    /// Text set on the accent.
    pub accent_ink: &'static str,
    /// Secondary accent (Sage where it has contrast, ink otherwise).
    pub secondary: &'static str,
    /// Whether this is a dark palette.
    pub dark: bool,
}

/// Dark workspace tokens (`:root` in app.css).
pub const DARK: Palette = Palette {
    background: None,
    base: "#141416",
    surface: "#1c1c20",
    surface_hi: "#28282c",
    surface_muted: "#353539",
    line: "#3d3d42",
    ink: "#e8e6e1",
    ink_60: "#9a9a9a",
    accent: "#c6f135",
    accent_ink: "#141416",
    secondary: "#b3d17a",
    dark: true,
};

/// Light workspace tokens (`.theme-light` in app.css). The accent is the
/// 35% lime / 65% ink mix of ADR 0002; Sage has no contrast on paper, so the
/// secondary falls back to ink as `--mng-math` does.
pub const LIGHT: Palette = Palette {
    background: None,
    base: "#faf9f7",
    surface: "#f4f3f0",
    surface_hi: "#ffffff",
    surface_muted: "#e8e7e2",
    line: "#d4d3ce",
    ink: "#1c1c1c",
    ink_60: "#6b6b6b",
    accent: "#586725",
    accent_ink: "#faf9f7",
    secondary: "#1c1c1c",
    dark: false,
};

/// Paper: white page, ink lines, so the figure prints without a backdrop.
pub const PRINT: Palette = Palette {
    background: Some("#ffffff"),
    base: "#ffffff",
    surface: "#ffffff",
    surface_hi: "#f4f3f0",
    surface_muted: "#e8e7e2",
    line: "#1c1c1c",
    ink: "#1c1c1c",
    ink_60: "#6b6b6b",
    accent: "#586725",
    accent_ink: "#faf9f7",
    secondary: "#6b6b6b",
    dark: false,
};
