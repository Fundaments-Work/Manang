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
    fn natural_size_counts_lines() {
        let (w, h) = natural_size("ab\nabcd", 10.0, FontFamily::Mono);
        assert!((w - 24.0).abs() < 1e-9);
        assert!((h - 25.0).abs() < 1e-9);
    }
}
