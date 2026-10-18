//! `edit_text`: exact search-and-replace pairs, located and checked by the host.
//!
//! The model quotes text; the host finds it. Every `old` must occur exactly once in the text the
//! model read (the editor buffer with the session's pending proposals applied). There is no
//! fuzzy fallback: a miss comes back with the closest line, never a guess, so the wrong text can
//! not be changed silently. Offsets exist only in the result, in UTF-16 units (what CodeMirror
//! counts), for the editor.

use crate::text::{byte_to_utf16, utf16_len};

/// One pair as the model wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub old: String,
    pub new: String,
}

/// One pair, found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// Byte range in the base text.
    pub bytes: (usize, usize),
    /// UTF-16 range in the base text.
    pub from: usize,
    pub to: usize,
    pub before: String,
    pub after: String,
}

/// 1-based line of a byte offset.
pub fn line_of(text: &str, byte: usize) -> usize {
    text.get(..byte)
        .map_or(1, |head| head.bytes().filter(|b| *b == b'\n').count() + 1)
}

/// `12| text` prefixes on every non-empty line: the model copied the numbering of `read_file`.
fn has_line_prefixes(text: &str) -> bool {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty()).peekable();
    if lines.peek().is_none() {
        return false;
    }
    lines.all(|line| {
        let digits = line
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .count();
        digits > 0 && line.trim_start()[digits..].starts_with('|')
    })
}

fn bigrams(s: &str) -> Vec<(char, char)> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Dice similarity of two strings over character pairs, 0.0 to 1.0.
fn similarity(a: &str, b: &str) -> f64 {
    let (x, y) = (bigrams(a), bigrams(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let mut pool = y.clone();
    let mut hits = 0usize;
    for pair in &x {
        if let Some(i) = pool.iter().position(|p| p == pair) {
            pool.swap_remove(i);
            hits += 1;
        }
    }
    (2 * hits) as f64 / (x.len() + y.len()) as f64
}

/// The line of `text` most like the first non-empty line of `old`: `(line, its text)`.
pub fn closest_line(text: &str, old: &str) -> Option<(usize, String)> {
    let probe = old.lines().find(|l| !l.trim().is_empty())?.trim();
    let mut best: Option<(usize, f64, &str)> = None;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let score = similarity(probe, line);
        if best.is_none_or(|(_, s, _)| score > s) {
            best = Some((i + 1, score, line));
        }
    }
    best.filter(|(_, score, _)| *score >= 0.5)
        .map(|(line, _, t)| {
            let shown: String = t.trim().chars().take(100).collect();
            (line, shown)
        })
}

fn lines_list(text: &str, hits: &[usize]) -> String {
    let mut lines: Vec<String> = hits
        .iter()
        .take(6)
        .map(|b| line_of(text, *b).to_string())
        .collect();
    if hits.len() > 6 {
        lines.push("...".to_owned());
    }
    lines.join(", ")
}

/// Finds every pair in `base`. Errors are steering text for the model.
pub fn locate(path: &str, base: &str, pairs: &[Pair]) -> Result<Vec<Located>, String> {
    let many = pairs.len() > 1;
    let which = |i: usize| {
        if many {
            format!("edit {} of {}: ", i + 1, pairs.len())
        } else {
            String::new()
        }
    };
    let mut found: Vec<(usize, usize)> = Vec::with_capacity(pairs.len());
    for (i, pair) in pairs.iter().enumerate() {
        if pair.old.is_empty() {
            return Err(format!(
                "{}`old` is empty. Quote the text to replace, or an anchor next to the place to insert.",
                which(i)
            ));
        }
        if pair.old == pair.new {
            return Err(format!(
                "{}`old` and `new` are identical, so nothing would change.",
                which(i)
            ));
        }
        if has_line_prefixes(&pair.old) {
            return Err(format!(
                "{}`old` starts lines with numbers and `|` (as read_file prints them). Quote the text without the numbering.",
                which(i)
            ));
        }
        let hits: Vec<usize> = base
            .match_indices(pair.old.as_str())
            .map(|(b, _)| b)
            .collect();
        match hits.as_slice() {
            [byte] => found.push((*byte, *byte + pair.old.len())),
            [] => {
                let hint = match closest_line(base, &pair.old) {
                    Some((line, text)) => format!(
                        " Closest match at line {line}: \"{text}\". Read lines {}-{} and quote it exactly.",
                        line.saturating_sub(3).max(1),
                        line + 4
                    ),
                    None => {
                        " Read the file around the place you mean and quote it exactly.".to_owned()
                    }
                };
                return Err(format!("{}old text not found in {path}.{hint}", which(i)));
            }
            many_hits => {
                return Err(format!(
                    "{}old text matches {} places in {path} (lines {}). Add surrounding words to make it unique.",
                    which(i),
                    many_hits.len(),
                    lines_list(base, many_hits)
                ));
            }
        }
    }
    let mut order: Vec<usize> = (0..pairs.len()).collect();
    order.sort_by_key(|i| found[*i].0);
    for w in order.windows(2) {
        let (a, b) = (found[w[0]], found[w[1]]);
        if b.0 < a.1 {
            return Err(format!(
                "edits {} and {} overlap at line {}. Merge them into one edit.",
                w[0] + 1,
                w[1] + 1,
                line_of(base, b.0)
            ));
        }
    }
    Ok(order
        .into_iter()
        .map(|i| {
            let (start, end) = found[i];
            Located {
                bytes: (start, end),
                from: byte_to_utf16(base, start),
                to: byte_to_utf16(base, start) + utf16_len(&pairs[i].old),
                before: pairs[i].old.clone(),
                after: pairs[i].new.clone(),
            }
        })
        .collect())
}

/// `base` with the located edits applied (they are sorted and do not overlap).
pub fn apply(base: &str, edits: &[Located]) -> String {
    let mut out = String::with_capacity(base.len());
    let mut at = 0;
    for e in edits {
        out.push_str(&base[at..e.bytes.0]);
        out.push_str(&e.after);
        at = e.bytes.1;
    }
    out.push_str(&base[at..]);
    out
}

/// Applies an earlier proposal's pairs to `text` by content, skipping any that no longer match
/// exactly once. Used to build the pending overlay, so it never fails.
pub fn apply_loosely(text: &str, pairs: &[Pair]) -> String {
    match locate("", text, pairs) {
        Ok(located) => apply(text, &located),
        Err(_) => {
            let mut current = text.to_owned();
            for pair in pairs {
                if let Ok(one) = locate("", &current, std::slice::from_ref(pair)) {
                    current = apply(&current, &one);
                }
            }
            current
        }
    }
}

/// Characters removed and added by a set of edits (for the result line).
pub fn char_delta(edits: &[Located]) -> (usize, usize) {
    edits.iter().fold((0, 0), |(r, a), e| {
        (r + e.before.chars().count(), a + e.after.chars().count())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(old: &str, new: &str) -> Pair {
        Pair {
            old: old.to_owned(),
            new: new.to_owned(),
        }
    }

    const TEXT: &str =
        "= Intro\nWe study the Fourier transform of x.\nThe Fourier transform is linear.\nEnd.\n";

    #[test]
    fn a_unique_quote_is_located_with_utf16_offsets() {
        let located = locate("main.typ", TEXT, &[pair("We study", "We examine")]).expect("found");
        assert_eq!(located[0].from, 8);
        assert_eq!(located[0].to, 16);
        assert_eq!(
            apply(TEXT, &located).lines().nth(1),
            Some("We examine the Fourier transform of x.")
        );
        let text = "α β γ\nsecond β\n";
        let located = locate("a.md", text, &[pair("γ", "c")]).expect("found");
        assert_eq!((located[0].from, located[0].to), (4, 5));
    }

    #[test]
    fn a_miss_names_the_closest_line() {
        let err = locate(
            "main.typ",
            TEXT,
            &[pair("We study the Fourier transfrom of x.", "y")],
        )
        .expect_err("miss");
        assert!(err.starts_with("old text not found in main.typ."), "{err}");
        assert!(err.contains("Closest match at line 2"), "{err}");
        assert!(err.contains("quote it exactly"), "{err}");
        let err = locate("main.typ", TEXT, &[pair("zzzzzzzzzz qqqq", "y")]).expect_err("miss");
        assert!(!err.contains("Closest"), "{err}");
    }

    #[test]
    fn a_repeated_quote_lists_where_it_occurs() {
        let err = locate("main.typ", TEXT, &[pair("Fourier transform", "FT")]).expect_err("two");
        assert!(err.contains("matches 2 places"), "{err}");
        assert!(err.contains("lines 2, 3"), "{err}");
        assert!(err.contains("surrounding words"), "{err}");
    }

    #[test]
    fn pairs_are_sorted_checked_for_overlap_and_numbering() {
        let located = locate(
            "m",
            TEXT,
            &[pair("End.", "Fin."), pair("= Intro", "= Start")],
        )
        .expect("found");
        assert_eq!(located[0].before, "= Intro");
        assert_eq!(
            apply(TEXT, &located),
            "= Start\nWe study the Fourier transform of x.\nThe Fourier transform is linear.\nFin.\n"
        );
        let err = locate(
            "m",
            TEXT,
            &[pair("We study the", "a"), pair("the Fourier", "b")],
        )
        .expect_err("overlap");
        assert!(err.contains("overlap"), "{err}");
        let err = locate("m", TEXT, &[pair("2| We study", "x")]).expect_err("numbered");
        assert!(err.contains("without the numbering"), "{err}");
        assert!(locate("m", TEXT, &[pair("End.", "End.")]).is_err());
        assert!(locate("m", TEXT, &[pair("", "x")]).is_err());
    }

    #[test]
    fn heavy_backslashes_and_braces_match_exactly() {
        let tex = "\\begin{equation}\n  \\int_0^1 \\frac{a}{b}\\,dx = \\sum_{k=1}^{n} x_k\n\\end{equation}\n";
        let old = "\\frac{a}{b}\\,dx = \\sum_{k=1}^{n}";
        let located = locate(
            "m.tex",
            tex,
            &[pair(old, "\\frac{a}{b}\\,dx = \\sum_{k=0}^{n}")],
        )
        .expect("found");
        assert!(apply(tex, &located).contains("\\sum_{k=0}^{n}"));
    }

    #[test]
    fn the_overlay_skips_what_no_longer_matches() {
        let out = apply_loosely(
            TEXT,
            &[pair("End.", "Fin."), pair("nothing like this", "x")],
        );
        assert!(out.ends_with("Fin.\n"));
        assert_eq!(apply_loosely(TEXT, &[]), TEXT);
    }
}
