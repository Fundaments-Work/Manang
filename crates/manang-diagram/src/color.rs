//! Keeps every colour in a rendered diagram on the design's tonal ramp.
//!
//! Mermaid emits colours of its own: CSS defaults for families such as
//! Sankey or C4, presentation-attribute fallbacks, and colours it derives
//! from the theme variables. The design allows design tokens only, so each
//! colour that is not already a token is replaced by the ramp token of the
//! nearest perceptual lightness (depth is tonal; hue is dropped).
//!
//! Mermaid's hard-coded literals (hex and named colours) are written for a
//! light page. On the dark theme their lightness is inverted before
//! matching, so dark text stays readable on dark surfaces. Functional
//! colours (`rgb()`, `hsl()`) are what Mermaid derives from our own dark
//! variables and are matched as they are.

use std::borrow::Cow;

use crate::theme::Palette;

/// Which foreign colours were written for a light page and so have their
/// lightness inverted on a dark palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invert {
    /// Hex and named literals (Mermaid's hard-coded defaults); functional
    /// colours are Mermaid's derivations from the theme and kept as is.
    Literals,
    /// Every foreign colour (engines with no theme at all, e.g. Pikchr).
    All,
}

/// A parsed sRGB colour and whether it was written as a literal.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Parsed {
    rgb: [f64; 3],
    literal: bool,
}

const NAMED: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("lightgray", [211, 211, 211]),
    ("lightgrey", [211, 211, 211]),
    ("darkgray", [169, 169, 169]),
    ("darkgrey", [169, 169, 169]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("silver", [192, 192, 192]),
    ("gainsboro", [220, 220, 220]),
    ("whitesmoke", [245, 245, 245]),
    ("red", [255, 0, 0]),
    ("green", [0, 128, 0]),
    ("blue", [0, 0, 255]),
    ("yellow", [255, 255, 0]),
    ("orange", [255, 165, 0]),
    ("purple", [128, 0, 128]),
    ("navy", [0, 0, 128]),
    ("teal", [0, 128, 128]),
    ("maroon", [128, 0, 0]),
    ("olive", [128, 128, 0]),
    ("lime", [0, 255, 0]),
    ("aqua", [0, 255, 255]),
    ("cyan", [0, 255, 255]),
    ("fuchsia", [255, 0, 255]),
    ("magenta", [255, 0, 255]),
    ("lightblue", [173, 216, 230]),
    ("lightgreen", [144, 238, 144]),
    ("lightyellow", [255, 255, 224]),
    ("pink", [255, 192, 203]),
    ("brown", [165, 42, 42]),
    ("darkred", [139, 0, 0]),
    ("darkgreen", [0, 100, 0]),
    ("darkblue", [0, 0, 139]),
    ("steelblue", [70, 130, 180]),
    ("ivory", [255, 255, 240]),
    ("beige", [245, 245, 220]),
    ("linen", [250, 240, 230]),
];

fn hex_digit(b: u8) -> Option<f64> {
    (b as char).to_digit(16).map(f64::from)
}

fn parse_hex(hex: &str) -> Option<[f64; 3]> {
    let b = hex.as_bytes();
    match b.len() {
        3 | 4 => {
            let c = |i: usize| hex_digit(b[i]).map(|d| d * 17.0);
            Some([c(0)?, c(1)?, c(2)?])
        }
        6 | 8 => {
            let c = |i: usize| Some(hex_digit(b[i])? * 16.0 + hex_digit(b[i + 1])?);
            Some([c(0)?, c(2)?, c(4)?])
        }
        _ => None,
    }
}

fn channel(arg: &str, scale: f64) -> Option<f64> {
    let arg = arg.trim();
    if let Some(pct) = arg.strip_suffix('%') {
        pct.trim().parse::<f64>().ok().map(|v| v / 100.0 * scale)
    } else {
        arg.strip_suffix("deg").unwrap_or(arg).parse::<f64>().ok()
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let h = h.rem_euclid(360.0) / 360.0;
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        v * 255.0
    };
    [hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0)]
}

/// Parses `rgb(..)`, `rgba(..)`, `hsl(..)`, `hsla(..)` (comma or space
/// separated, optional `/ alpha`).
fn parse_function(name: &str, args: &str) -> Option<[f64; 3]> {
    let main = args.split('/').next()?;
    let parts: Vec<&str> = main
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    if name.starts_with("rgb") {
        Some([
            channel(parts[0], 255.0)?,
            channel(parts[1], 255.0)?,
            channel(parts[2], 255.0)?,
        ])
    } else {
        Some(hsl_to_rgb(
            channel(parts[0], 360.0)?,
            channel(parts[1], 1.0)?,
            channel(parts[2], 1.0)?,
        ))
    }
}

/// Perceptual lightness (CIE L*, scaled to 0..1) of an sRGB colour.
fn lightness(rgb: [f64; 3]) -> f64 {
    let lin = |c: f64| {
        let c = (c / 255.0).clamp(0.0, 1.0);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let y = 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2]);
    let l = if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    };
    l / 100.0
}

/// The tonal ramp of a palette: the tokens a foreign colour may become.
fn ramp(p: &Palette) -> [&'static str; 7] {
    [
        p.base,
        p.surface,
        p.surface_hi,
        p.surface_muted,
        p.line,
        p.ink_60,
        p.ink,
    ]
}

fn is_token(p: &Palette, hex: &str) -> bool {
    let tokens = [
        p.base,
        p.surface,
        p.surface_hi,
        p.surface_muted,
        p.line,
        p.ink,
        p.ink_60,
        p.accent,
        p.accent_ink,
        p.secondary,
    ];
    tokens
        .iter()
        .chain(p.background.iter())
        .any(|t| t.eq_ignore_ascii_case(hex))
}

fn nearest(p: &Palette, color: Parsed, invert: Invert) -> &'static str {
    let mut target = lightness(color.rgb);
    if p.dark && (color.literal || invert == Invert::All) {
        target = 1.0 - target;
    }
    let mut best = p.ink;
    let mut best_d = f64::INFINITY;
    for token in ramp(p) {
        let Some(rgb) = token.strip_prefix('#').and_then(parse_hex) else {
            continue;
        };
        let d = (lightness(rgb) - target).abs();
        if d < best_d {
            best = token;
            best_d = d;
        }
    }
    best
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Finds the colour token starting at `i`, returning its end and value.
fn token_at(text: &str, i: usize) -> Option<(usize, Parsed)> {
    let bytes = text.as_bytes();
    let prev = i.checked_sub(1).map(|j| bytes[j]);
    if prev.is_some_and(|b| is_ident(b) || b == b'.' || b == b'#') {
        return None;
    }
    let rest = text.get(i..)?;
    if let Some(hex) = rest.strip_prefix('#') {
        let len = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
        // A value ends the declaration or attribute; anything else (`{`,
        // `.`, a combinator) means an id selector such as `#add {`.
        let after = hex.get(len..).unwrap_or_default().trim_start();
        if !(after.is_empty() || after.starts_with([';', '}', ')', '!', '"', '\''])) {
            return None;
        }
        let rgb = parse_hex(hex.get(..len)?)?;
        return Some((i + 1 + len, Parsed { rgb, literal: true }));
    }
    let word_len = rest.bytes().take_while(|&b| is_ident(b)).count();
    let word = rest.get(..word_len)?;
    let lower = word.to_ascii_lowercase();
    if matches!(lower.as_str(), "rgb" | "rgba" | "hsl" | "hsla")
        && rest.as_bytes().get(word_len) == Some(&b'(')
    {
        let close = rest.find(')')?;
        let rgb = parse_function(&lower, rest.get(word_len + 1..close)?)?;
        return Some((
            i + close + 1,
            Parsed {
                rgb,
                literal: false,
            },
        ));
    }
    let (_, rgb) = NAMED.iter().find(|(name, _)| *name == lower)?;
    Some((
        i + word_len,
        Parsed {
            rgb: rgb.map(f64::from),
            literal: true,
        },
    ))
}

/// Replaces every colour in `text` that is not a palette token.
pub fn clamp<'t>(text: &'t str, p: &Palette, invert: Invert) -> Cow<'t, str> {
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let b = bytes[i];
        if (b == b'#' || b.is_ascii_alphabetic())
            && let Some((end, color)) = token_at(text, i)
        {
            let found = text.get(i..end).unwrap_or_default();
            if !is_token(p, found) {
                out.push_str(text.get(copied..i).unwrap_or_default());
                out.push_str(nearest(p, color, invert));
                copied = end;
            }
            i = end;
            continue;
        }
        i += 1;
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(text.get(copied..).unwrap_or_default());
    Cow::Owned(out)
}

/// Drops CSS declarations whose value uses `var(--…)`: usvg and Typst do
/// not resolve custom properties and would read the declaration verbatim.
pub fn strip_css_vars(css: &str) -> Cow<'_, str> {
    if !css.contains("var(--") {
        return Cow::Borrowed(css);
    }
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("var(--") {
        let start = rest
            .get(..at)
            .and_then(|s| s.rfind([';', '{']))
            .map_or(0, |j| j + 1);
        let tail = rest.get(at..).unwrap_or_default();
        let end_rel = tail.find([';', '}']).unwrap_or(tail.len());
        // Keep a `}` that closes the block; consume a `;`.
        let end = at + end_rel + usize::from(tail.as_bytes().get(end_rel) == Some(&b';'));
        out.push_str(rest.get(..start).unwrap_or_default());
        rest = rest.get(end..).unwrap_or_default();
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DARK, PRINT};

    #[test]
    fn tokens_are_kept_and_others_mapped() {
        assert_eq!(
            clamp("fill:#1c1c20;stroke:#3d3d42", &DARK, Invert::Literals),
            "fill:#1c1c20;stroke:#3d3d42"
        );
        // Black text written for a light page becomes ink on dark.
        assert_eq!(clamp("fill:black", &DARK, Invert::Literals), "fill:#e8e6e1");
        assert_eq!(clamp("fill:#000", &PRINT, Invert::Literals), "fill:#1c1c1c");
        assert_eq!(clamp("fill:#fff", &PRINT, Invert::Literals), "fill:#ffffff");
        // Derived dark colours stay dark.
        assert_eq!(
            clamp("hsl(240, 6%, 2%)", &DARK, Invert::Literals),
            "#141416"
        );
    }

    #[test]
    fn selectors_ids_and_words_are_not_colours() {
        let css = "#mng-abc .red{white-space:pre;marker-end:url(#arrow)} #add{x:1} #bad .c{}";
        assert_eq!(clamp(css, &DARK, Invert::Literals), css);
    }

    #[test]
    fn css_variables_are_dropped() {
        assert_eq!(
            strip_css_vars(".a{font-family:x;font-family:var(--f);fill:red}.b{c:var(--d)}"),
            ".a{font-family:x;fill:red}.b{}"
        );
    }
}
