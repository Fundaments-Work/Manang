//! Themes: the brand's design tokens as data, plus guarded user themes.
//!
//! The built-in `dark` and `light` maps copy the custom properties in
//! `src/app.css` (the source of truth; `--mng-accent` in the light theme is
//! kept as the same `color-mix` expression). Brand colours are not
//! user-editable: a user theme (`<config_dir>/themes/<id>.toml`) names a
//! `base`, may override only the neutral tokens in [`OVERRIDABLE`] with plain
//! hex colours, and is rejected unless every text pairing reaches WCAG 4.5:1
//! and every UI pairing 3:1. The accent and the ink set on it are locked.
//!
//! ```toml
//! name = "Soft dark"
//! base = "dark"
//! [tokens]
//! "--mng-bg" = "#18181b"
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::error::SettingsError;
use crate::registry::valid_dotted_id;
use crate::store;

/// Minimum contrast for text against its background.
pub const TEXT_CONTRAST: f64 = 4.5;
/// Minimum contrast for UI components against their background.
pub const UI_CONTRAST: f64 = 3.0;

/// Tokens a user theme may override.
pub const OVERRIDABLE: &[&str] = &[
    "--mng-bg",
    "--mng-surface",
    "--mng-surface-hi",
    "--mng-surface-muted",
    "--mng-recessed",
    "--mng-line",
    "--mng-line-subtle",
    "--mng-text",
    "--mng-text-60",
    "--mng-text-muted",
    "--mng-math",
];

/// Brand tokens that no user theme can change.
pub const LOCKED: &[&str] = &["--mng-accent", "--mng-accent-ink"];

const DARK: &[(&str, &str)] = &[
    ("--mng-bg", "#141416"),
    ("--mng-surface", "#1c1c20"),
    ("--mng-surface-hi", "#28282c"),
    ("--mng-surface-muted", "#353539"),
    ("--mng-recessed", "#0d0f0a"),
    ("--mng-line", "#3d3d42"),
    ("--mng-line-subtle", "#323236"),
    ("--mng-text", "#e8e6e1"),
    ("--mng-text-60", "#9a9a9a"),
    ("--mng-text-muted", "#7a7a7a"),
    ("--mng-accent", "#c6f135"),
    ("--mng-accent-ink", "#141416"),
    ("--mng-math", "#b3d17a"),
];

const LIGHT: &[(&str, &str)] = &[
    ("--mng-bg", "#faf9f7"),
    ("--mng-surface", "#f4f3f0"),
    ("--mng-surface-hi", "#ffffff"),
    ("--mng-surface-muted", "#e8e7e2"),
    ("--mng-recessed", "#f4f3f0"),
    ("--mng-line", "#d4d3ce"),
    ("--mng-line-subtle", "#e2e1dd"),
    ("--mng-text", "#1c1c1c"),
    ("--mng-text-60", "#6b6b6b"),
    ("--mng-text-muted", "#9c9c9c"),
    ("--mng-accent", "color-mix(in srgb, #c6f135 35%, #1c1c1c)"),
    ("--mng-accent-ink", "#faf9f7"),
    ("--mng-math", "#1c1c1c"),
];

/// `(foreground, backgrounds, required ratio)` pairings that must hold.
const PAIRINGS: &[(&str, &[&str], f64)] = &[
    (
        "--mng-text",
        &[
            "--mng-bg",
            "--mng-surface",
            "--mng-surface-hi",
            "--mng-surface-muted",
            "--mng-recessed",
        ],
        TEXT_CONTRAST,
    ),
    (
        "--mng-text-60",
        &[
            "--mng-bg",
            "--mng-surface",
            "--mng-surface-hi",
            "--mng-recessed",
        ],
        TEXT_CONTRAST,
    ),
    ("--mng-math", &["--mng-bg", "--mng-surface"], TEXT_CONTRAST),
    ("--mng-accent-ink", &["--mng-accent"], TEXT_CONTRAST),
    ("--mng-accent", &["--mng-bg", "--mng-surface"], UI_CONTRAST),
];

/// Whether a theme is for a dark or a light UI (drives `color-scheme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Dark,
    Light,
}

/// A complete theme: every token resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub scheme: Scheme,
    pub builtin: bool,
    /// Built-in theme this one extends; `None` for the built-ins themselves.
    pub base: Option<String>,
    /// CSS custom property to value, complete.
    pub tokens: BTreeMap<String, String>,
    /// The subset a user theme changed.
    pub overrides: BTreeMap<String, String>,
}

fn builtin(id: &str, name: &str, scheme: Scheme, table: &[(&str, &str)]) -> Theme {
    Theme {
        id: id.to_owned(),
        name: name.to_owned(),
        scheme,
        builtin: true,
        base: None,
        tokens: table
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        overrides: BTreeMap::new(),
    }
}

/// The built-in `dark` and `light` themes.
pub fn builtin_themes() -> Vec<Theme> {
    vec![
        builtin("dark", "Dark", Scheme::Dark, DARK),
        builtin("light", "Light", Scheme::Light, LIGHT),
    ]
}

// ---------------------------------------------------------------------------
// Colour maths
// ---------------------------------------------------------------------------

/// An sRGB colour with channels in `0.0..=255.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub f64, pub f64, pub f64);

fn parse_hex(text: &str) -> Option<Rgb> {
    let hex = text.strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |s: &str| u8::from_str_radix(s, 16).ok().map(f64::from);
    match hex.len() {
        3 => {
            let d = |i: usize| channel(&hex[i..=i]).map(|v| v * 17.0);
            Some(Rgb(d(0)?, d(1)?, d(2)?))
        }
        6 => Some(Rgb(
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        )),
        _ => None,
    }
}

/// Parse `#rgb`, `#rrggbb` or `color-mix(in srgb, <hex> <n>%, <hex>)`.
pub fn parse_color(text: &str) -> Option<Rgb> {
    let text = text.trim();
    if let Some(rest) = text
        .strip_prefix("color-mix(in srgb,")
        .and_then(|r| r.strip_suffix(')'))
    {
        let (first, second) = rest.split_once(',')?;
        let (hex, pct) = first.trim().split_once(char::is_whitespace)?;
        let pct: f64 = pct.trim().strip_suffix('%')?.parse().ok()?;
        if !(0.0..=100.0).contains(&pct) {
            return None;
        }
        let (a, b) = (parse_hex(hex.trim())?, parse_hex(second.trim())?);
        let w = pct / 100.0;
        return Some(Rgb(
            a.0 * w + b.0 * (1.0 - w),
            a.1 * w + b.1 * (1.0 - w),
            a.2 * w + b.2 * (1.0 - w),
        ));
    }
    parse_hex(text)
}

fn luminance(c: Rgb) -> f64 {
    let lin = |v: f64| {
        let s = v / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

/// WCAG 2.x contrast ratio, from 1 to 21.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Every pairing in `tokens` that falls short, as readable messages.
pub fn contrast_failures(tokens: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    for (fg, backgrounds, need) in PAIRINGS {
        for bg in *backgrounds {
            let (Some(f), Some(b)) = (
                tokens.get(*fg).and_then(|v| parse_color(v)),
                tokens.get(*bg).and_then(|v| parse_color(v)),
            ) else {
                continue;
            };
            let ratio = contrast_ratio(f, b);
            if ratio < *need {
                let kind = if *need >= TEXT_CONTRAST { "text" } else { "UI" };
                out.push(format!(
                    "`{fg}` on `{bg}` has contrast {ratio:.2}:1; {kind} needs at least {need}:1"
                ));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// User themes
// ---------------------------------------------------------------------------

fn bad(id: &str, message: impl Into<String>) -> SettingsError {
    SettingsError::BadTheme {
        id: id.to_owned(),
        message: message.into(),
    }
}

/// Parse and validate a user theme file's text.
pub fn parse_user_theme(id: &str, text: &str) -> Result<Theme, SettingsError> {
    if !valid_dotted_id(&format!("theme.{id}")) || builtin_themes().iter().any(|t| t.id == id) {
        return Err(bad(
            id,
            "the id must be lowercase kebab-case and not a built-in name",
        ));
    }
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| bad(id, format!("not valid TOML: {}", e.message())))?;
    let json = store::doc_to_json(&doc);
    let base_id = json
        .get("base")
        .and_then(Value::as_str)
        .ok_or_else(|| bad(id, "`base` is required and must be \"dark\" or \"light\""))?;
    let base = builtin_themes()
        .into_iter()
        .find(|t| t.id == base_id)
        .ok_or_else(|| {
            bad(
                id,
                format!("unknown base `{base_id}`; use \"dark\" or \"light\""),
            )
        })?;
    let name = match json.get("name") {
        None => id.to_owned(),
        Some(Value::String(s)) if !s.trim().is_empty() && s.len() <= 80 => s.clone(),
        Some(_) => return Err(bad(id, "`name` must be a non-empty string up to 80 bytes")),
    };
    let mut overrides = BTreeMap::new();
    match json.get("tokens") {
        None => {}
        Some(Value::Object(map)) => {
            for (token, value) in map {
                if LOCKED.contains(&token.as_str()) {
                    return Err(bad(
                        id,
                        format!("`{token}` is a brand colour and cannot be overridden"),
                    ));
                }
                if !OVERRIDABLE.contains(&token.as_str()) {
                    return Err(bad(
                        id,
                        format!(
                            "unknown token `{token}`; allowed tokens are {}",
                            OVERRIDABLE.join(", ")
                        ),
                    ));
                }
                let color = value
                    .as_str()
                    .filter(|v| v.starts_with('#') && parse_hex(v).is_some())
                    .ok_or_else(|| {
                        bad(
                            id,
                            format!("`{token}` must be a hex colour like \"#1a1a1c\""),
                        )
                    })?;
                overrides.insert(token.clone(), color.to_owned());
            }
        }
        Some(_) => return Err(bad(id, "`tokens` must be a table")),
    }
    if let Some(unknown) = json.as_object().and_then(|o| {
        o.keys()
            .find(|k| !["name", "base", "tokens"].contains(&k.as_str()))
    }) {
        return Err(bad(id, format!("unknown key `{unknown}`")));
    }
    let mut tokens = base.tokens.clone();
    tokens.extend(overrides.clone());
    let failures = contrast_failures(&tokens);
    if !failures.is_empty() {
        return Err(bad(id, failures.join("; ")));
    }
    Ok(Theme {
        id: id.to_owned(),
        name,
        scheme: base.scheme,
        builtin: false,
        base: Some(base.id),
        tokens,
        overrides,
    })
}

/// Built-in themes plus the valid ones in a user themes folder.
#[derive(Debug, Clone)]
pub struct ThemeStore {
    dir: PathBuf,
}

impl ThemeStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// All usable themes (built-ins first) and a warning per rejected file.
    pub fn list(&self) -> (Vec<Theme>, Vec<String>) {
        let mut themes = builtin_themes();
        let mut warnings = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return (themes, warnings);
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml") && p.is_file())
            .collect();
        files.sort();
        for path in files {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let parsed =
                store::read_text(&path).and_then(|t| parse_user_theme(id, &t.unwrap_or_default()));
            match parsed {
                Ok(theme) => themes.push(theme),
                Err(e) => warnings.push(format!("themes/{id}.toml: {e}")),
            }
        }
        (themes, warnings)
    }

    pub fn get(&self, id: &str) -> Result<Theme, SettingsError> {
        if let Some(t) = builtin_themes().into_iter().find(|t| t.id == id) {
            return Ok(t);
        }
        // Only look at a file when the id cannot escape the folder.
        if !valid_dotted_id(&format!("theme.{id}")) {
            return Err(SettingsError::UnknownTheme(id.to_owned()));
        }
        let path = self.dir.join(format!("{id}.toml"));
        match store::read_text(&path)? {
            Some(text) => parse_user_theme(id, &text),
            None => Err(SettingsError::UnknownTheme(id.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_cover_the_same_tokens_and_pass_contrast() {
        let themes = builtin_themes();
        let keys: Vec<Vec<&String>> = themes.iter().map(|t| t.tokens.keys().collect()).collect();
        assert_eq!(keys[0], keys[1]);
        for t in &themes {
            assert_eq!(
                contrast_failures(&t.tokens),
                Vec::<String>::new(),
                "{}",
                t.id
            );
        }
    }

    #[test]
    fn overridable_plus_locked_is_every_token() {
        let mut all: Vec<&str> = OVERRIDABLE.iter().chain(LOCKED).copied().collect();
        all.sort_unstable();
        let mut have: Vec<&str> = DARK.iter().map(|(k, _)| *k).collect();
        have.sort_unstable();
        assert_eq!(all, have);
    }

    #[test]
    fn known_contrast_values() {
        let black = Rgb(0.0, 0.0, 0.0);
        let white = Rgb(255.0, 255.0, 255.0);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-9);
        // Independently computed WCAG ratios (the brand page rounds to 14.16 / 1.24).
        let lime = parse_color("#c6f135").expect("lime");
        let carbon = parse_color("#141416").expect("carbon");
        assert!((contrast_ratio(lime, carbon) - 14.05).abs() < 0.01);
        // ...and 1.24:1 on paper.
        let paper = parse_color("#faf9f7").expect("paper");
        assert!((contrast_ratio(lime, paper) - 1.24).abs() < 0.05);
    }

    #[test]
    fn color_mix_matches_the_css() {
        let c = parse_color("color-mix(in srgb, #c6f135 35%, #1c1c1c)").expect("mix");
        assert!((c.0 - 87.5).abs() < 0.01 && (c.1 - 102.55).abs() < 0.01);
        assert_eq!(parse_color("#fff"), Some(Rgb(255.0, 255.0, 255.0)));
        assert_eq!(parse_color("red"), None);
        assert_eq!(parse_color("#12345"), None);
    }

    #[test]
    fn user_theme_accepts_safe_overrides() {
        let t = parse_user_theme(
            "soft",
            "name = \"Soft\"\nbase = \"dark\"\n[tokens]\n\"--mng-bg\" = \"#18181b\"\n",
        )
        .expect("valid");
        assert_eq!(t.tokens["--mng-bg"], "#18181b");
        assert_eq!(t.tokens["--mng-accent"], "#c6f135");
        assert_eq!(t.scheme, Scheme::Dark);
    }

    #[test]
    fn user_theme_rejections_snapshot() {
        let cases = [
            (
                "a",
                "base = \"dark\"\n[tokens]\n\"--mng-accent\" = \"#ff0000\"\n",
            ),
            (
                "b",
                "base = \"dark\"\n[tokens]\n\"--mng-radius\" = \"#ff0000\"\n",
            ),
            ("c", "base = \"dark\"\n[tokens]\n\"--mng-bg\" = \"red\"\n"),
            (
                "d",
                "base = \"dark\"\n[tokens]\n\"--mng-bg\" = \"#e8e6e1\"\n",
            ),
            (
                "e",
                "base = \"light\"\n[tokens]\n\"--mng-text\" = \"#cccccc\"\n",
            ),
            ("f", "base = \"sepia\"\n"),
            ("g", "name = \"x\"\n"),
            ("h", "base = \"dark\"\nextra = 1\n"),
            ("dark", "base = \"dark\"\n"),
            ("i", "base = = \n"),
        ];
        let out: Vec<String> = cases
            .iter()
            .map(|(id, text)| match parse_user_theme(id, text) {
                Ok(_) => format!("{id}: accepted"),
                Err(e) => e.to_string(),
            })
            .collect();
        insta::assert_debug_snapshot!(out);
    }
}
