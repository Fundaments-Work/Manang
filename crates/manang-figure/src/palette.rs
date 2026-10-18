//! Figure colours: named tokens from `palette.json`, resolved per theme.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const PALETTE_JSON: &str = include_str!("palette.json");

/// Which set of token values to draw with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// The app's dark workspace.
    Dark,
    /// The app's light workspace.
    Light,
    /// Files and PDFs: dark ink for paper. The default for exports.
    #[default]
    Print,
}

impl std::str::FromStr for Theme {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            "print" => Ok(Self::Print),
            other => Err(format!("unknown theme `{other}`; use dark, light or print")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenValues {
    dark: String,
    light: String,
    print: String,
}

#[derive(Debug, Deserialize)]
struct PaletteFile {
    tokens: BTreeMap<String, TokenValues>,
    neutral: Vec<String>,
    tint: f64,
}

fn palette() -> Option<&'static PaletteFile> {
    static PALETTE: OnceLock<Option<PaletteFile>> = OnceLock::new();
    PALETTE
        .get_or_init(|| serde_json::from_str(PALETTE_JSON).ok())
        .as_ref()
}

/// Every token name, in a stable order.
pub fn token_names() -> Vec<&'static str> {
    palette()
        .map(|p| p.tokens.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// Whether `name` is a palette token.
pub fn is_token(name: &str) -> bool {
    palette().is_some_and(|p| p.tokens.contains_key(name))
}

fn is_hex(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else {
        return false;
    };
    (digits.len() == 6 || digits.len() == 3) && digits.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether `value` is a colour the renderer accepts: `none`, a token or a
/// `#rgb`/`#rrggbb` literal.
pub fn is_colour(value: &str) -> bool {
    value == "none" || is_token(value) || is_hex(value)
}

/// A colour as the scene stores it: a literal `#rgb` or `#rrggbb` becomes
/// lowercase `#rrggbb`; `none`, tokens and everything else stay as given.
pub fn normalize_colour(value: &str) -> String {
    if !is_hex(value) {
        return value.to_owned();
    }
    let digits = value[1..].to_ascii_lowercase();
    if digits.len() == 3 {
        let mut out = String::with_capacity(7);
        out.push('#');
        for c in digits.chars() {
            out.push(c);
            out.push(c);
        }
        out
    } else {
        format!("#{digits}")
    }
}

/// The red, green and blue of a `#rgb` or `#rrggbb` literal.
pub fn parse_rgb(value: &str) -> Option<(u8, u8, u8)> {
    if !is_hex(value) {
        return None;
    }
    let full = normalize_colour(value);
    let byte = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok();
    Some((byte(1)?, byte(3)?, byte(5)?))
}

/// Whether `value` looks like a hex literal (starts with `#`) but is not a
/// valid one: a typo worth an error rather than a silent fallback to `ink`.
pub fn is_malformed_hex(value: &str) -> bool {
    value.starts_with('#') && !is_hex(value)
}

/// A paint resolved for SVG: the colour and the opacity a fill gets.
#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    /// `none` or `#rrggbb`.
    pub colour: String,
    /// 1.0, or the tint for a hue used as a fill.
    pub alpha: f64,
}

/// The SVG colour for a stroke value: tokens resolve per theme, literals
/// pass through, anything unknown falls back to `ink`.
pub fn stroke(value: &str, theme: Theme) -> String {
    resolve(value, theme).unwrap_or_else(|| resolve("ink", theme).unwrap_or_default())
}

/// The SVG paint for a fill value. A hue token paints as a tint; neutrals,
/// literals and `none` paint as they are.
pub fn fill(value: &str, theme: Theme) -> Paint {
    if value == "none" {
        return Paint {
            colour: "none".to_owned(),
            alpha: 1.0,
        };
    }
    let alpha = match palette() {
        Some(p) if p.tokens.contains_key(value) && !p.neutral.iter().any(|n| n == value) => p.tint,
        _ => 1.0,
    };
    match resolve(value, theme) {
        Some(colour) => Paint { colour, alpha },
        None => Paint {
            colour: "none".to_owned(),
            alpha: 1.0,
        },
    }
}

fn resolve(value: &str, theme: Theme) -> Option<String> {
    if is_hex(value) {
        return Some(value.to_ascii_lowercase());
    }
    let token = palette()?.tokens.get(value)?;
    Some(
        match theme {
            Theme::Dark => &token.dark,
            Theme::Light => &token.light,
            Theme::Print => &token.print,
        }
        .clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_parses_and_every_value_is_hex() {
        let p = palette().expect("palette.json parses");
        assert!(p.tokens.len() >= 12);
        for (name, v) in &p.tokens {
            for value in [&v.dark, &v.light, &v.print] {
                assert!(is_hex(value), "{name}: {value}");
            }
        }
        for n in &p.neutral {
            assert!(p.tokens.contains_key(n), "{n}");
        }
    }

    #[test]
    fn hex_literals_normalise_to_lowercase_six_digits() {
        assert_eq!(normalize_colour("#ABC"), "#aabbcc");
        assert_eq!(normalize_colour("#C6F135"), "#c6f135");
        assert_eq!(normalize_colour("ink"), "ink");
        assert_eq!(normalize_colour("none"), "none");
        assert_eq!(parse_rgb("#c6f135"), Some((0xc6, 0xf1, 0x35)));
        assert_eq!(parse_rgb("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("ink"), None);
        assert!(is_malformed_hex("#12") && is_malformed_hex("#gggggg"));
        assert!(!is_malformed_hex("#123456") && !is_malformed_hex("ink"));
    }

    #[test]
    fn tokens_resolve_per_theme_and_hues_tint() {
        assert_eq!(stroke("ink", Theme::Dark), "#e8e6e1");
        assert_eq!(stroke("ink", Theme::Print), "#1c1c1c");
        assert_eq!(stroke("#ABCDEF", Theme::Print), "#abcdef");
        assert_eq!(stroke("nonsense", Theme::Print), "#1c1c1c");
        assert_eq!(fill("surface", Theme::Dark).alpha, 1.0);
        assert!(fill("blue", Theme::Dark).alpha < 1.0);
        assert_eq!(fill("none", Theme::Dark).colour, "none");
        assert!(is_colour("vermillion") && is_colour("#fff") && !is_colour("red"));
    }
}
