//! Deterministic text metrics and wrapping.
//!
//! The app ships DM Sans and DM Mono only as WOFF2 (ADR 0011), so there is
//! no font file to measure with here. These advance widths, in ems, are
//! approximations of DM Sans by character class; DM Mono is 0.6 em
//! throughout. `src/lib/figure/text.ts` holds the same table, so the canvas
//! and the exported SVG break lines at the same words.

use crate::scene::FontFamily;

/// Line height as a multiple of the font size.
pub const LINE_HEIGHT: f64 = 1.25;

/// Advance width of one character in ems.
pub fn advance(c: char, family: FontFamily) -> f64 {
    if family == FontFamily::Mono {
        return 0.6;
    }
    match c {
        ' ' => 0.26,
        'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | 'I' => 0.24,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '{' | '}' | '-' | '"' | '/' | '\\' => 0.34,
        'm' | 'w' => 0.82,
        'M' | 'W' => 0.9,
        'A'..='Z' => 0.66,
        '0'..='9' => 0.58,
        'a'..='z' => 0.54,
        _ if c.is_ascii() => 0.56,
        // CJK and other wide scripts.
        _ if (c as u32) >= 0x2E80 => 1.0,
        _ => 0.6,
    }
}

/// Width of `text` on one line.
pub fn measure(text: &str, size: f64, family: FontFamily) -> f64 {
    text.chars().map(|c| advance(c, family)).sum::<f64>() * size
}

/// Greedy word wrap of `text` to `max_width`. Explicit newlines are kept;
/// a word longer than a line is broken between characters. A non-positive
/// width disables wrapping.
pub fn wrap(text: &str, max_width: f64, size: f64, family: FontFamily) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if max_width <= 0.0 {
            lines.push(paragraph.to_owned());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate, size, family) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // The word alone may still be too long: break it.
            for c in word.chars() {
                let mut next = line.clone();
                next.push(c);
                if !line.is_empty() && measure(&next, size, family) > max_width {
                    lines.push(std::mem::take(&mut line));
                    line.push(c);
                } else {
                    line = next;
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// How far a label's font may shrink (as a fraction of its size) to keep
/// a long word whole before the word is broken.
pub const MIN_LABEL_SCALE: f64 = 0.7;

/// Fits `label` in a `max_width` by `max_height` text box: breaks only at
/// spaces and newlines; a single word wider than the box shrinks the font
/// (down to [`MIN_LABEL_SCALE`], in 0.5 steps) before it is broken between
/// characters; too many lines shrink it the same way. Returns the lines and
/// the font size used. `src/lib/figure/text.ts` mirrors this.
pub fn fit_label(
    label: &str,
    max_width: f64,
    max_height: f64,
    size: f64,
    family: FontFamily,
) -> (Vec<String>, f64) {
    let floor = size * MIN_LABEL_SCALE;
    let mut fitted = size;
    let widest = label
        .split([' ', '\n'])
        .filter(|w| !w.is_empty())
        .map(|w| measure(w, size, family))
        .fold(0.0, f64::max);
    if max_width > 0.0 && widest > max_width {
        fitted = (size * max_width / widest * 2.0).floor() / 2.0;
        fitted = fitted.clamp(floor, size);
    }
    loop {
        let lines = wrap(label, max_width, fitted, family);
        if lines.len() as f64 * fitted * LINE_HEIGHT <= max_height || fitted <= floor {
            return (lines, fitted);
        }
        fitted = (fitted - 0.5).max(floor);
    }
}

/// Width and height of `text` laid out without a width limit.
pub fn natural_size(text: &str, size: f64, family: FontFamily) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|l| measure(l, size, family))
        .fold(0.0, f64::max);
    (width, lines.len() as f64 * size * LINE_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_words_and_breaks_long_words() {
        let lines = wrap("hello wide world", 60.0, 16.0, FontFamily::Sans);
        assert_eq!(lines, vec!["hello", "wide", "world"]);
        let long = wrap("abcdefghijklmnop", 40.0, 16.0, FontFamily::Mono);
        assert!(long.len() > 1);
        assert!(
            long.iter()
                .all(|l| measure(l, 16.0, FontFamily::Mono) <= 40.0)
        );
        assert_eq!(wrap("a\nb", 0.0, 16.0, FontFamily::Sans), vec!["a", "b"]);
        assert_eq!(wrap("", 100.0, 16.0, FontFamily::Sans), vec![""]);
    }

    #[test]
    fn fit_label_keeps_words_whole_by_shrinking_first() {
        // "Validate" is too wide for 60 at 16px, but fits at about 12px.
        let (lines, size) = fit_label("Validate input", 50.0, 100.0, 16.0, FontFamily::Sans);
        assert_eq!(lines, vec!["Validate", "input"]);
        assert!((16.0 * MIN_LABEL_SCALE..16.0).contains(&size));
        // Short words keep the full size.
        let (lines, size) = fit_label("Is it ok", 80.0, 100.0, 16.0, FontFamily::Sans);
        assert_eq!(size, 16.0);
        assert!(lines.len() <= 2);
        // Past the floor, the word is broken.
        let (lines, size) = fit_label("Supercalifragilistic", 40.0, 100.0, 16.0, FontFamily::Sans);
        assert_eq!(size, 16.0 * MIN_LABEL_SCALE);
        assert!(lines.len() > 1);
    }

    #[test]
    fn natural_size_counts_lines() {
        let (w, h) = natural_size("ab\nabcd", 10.0, FontFamily::Mono);
        assert!((w - 24.0).abs() < 1e-9);
        assert!((h - 25.0).abs() < 1e-9);
    }
}
