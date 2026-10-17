//! Tokenising masked prose into words, sentences and paragraphs.
//!
//! All functions take text already passed through [`crate::mask::prose`], so
//! they never see code or math. Offsets are byte offsets into that text.

/// A word: letters and digits, with inner apostrophes and hyphens kept
/// (`don't`, `well-known`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word {
    pub start: usize,
    pub end: usize,
}

impl Word {
    pub fn text(self, source: &str) -> &str {
        &source[self.start..self.end]
    }
}

fn is_connector(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-' | '\u{2010}' | '\u{2011}')
}

pub fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::with_capacity(text.len() / 6);
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_alphanumeric() {
            start.get_or_insert(i);
        } else if start.is_some()
            && is_connector(c)
            && chars.peek().is_some_and(|&(_, n)| n.is_alphanumeric())
        {
            // Inner connector: the word continues.
        } else if let Some(s) = start.take() {
            out.push(Word { start: s, end: i });
        }
    }
    if let Some(s) = start {
        out.push(Word {
            start: s,
            end: text.len(),
        });
    }
    out
}

/// A sentence, trimmed to its first and last word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sentence {
    pub start: usize,
    pub end: usize,
    pub words: usize,
}

/// Abbreviations that end in a full stop without ending the sentence. Single
/// letters (initials, `e.g.`) are handled separately.
const ABBREVIATIONS: &[&str] = &[
    "al", "fig", "figs", "eq", "eqs", "sec", "secs", "tab", "dr", "mr", "mrs", "ms", "prof", "vs",
    "cf", "no", "vol", "vols", "ch", "pp", "ed", "eds", "approx", "resp", "ref", "refs", "incl",
    "ca", "st", "jr", "sr", "inc", "ltd", "co", "viz", "thm", "lem", "cor", "def", "prop", "ie",
    "eg",
];

fn word_ending_at(words: &[Word], end: usize) -> Option<Word> {
    let at = words.partition_point(|w| w.end < end);
    words.get(at).copied().filter(|w| w.end == end)
}

fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\u{2026}')
}

fn is_closer(c: char) -> bool {
    matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}' | ')' | ']' | '}')
}

/// Split into sentences; only chunks that contain a word are returned.
pub fn sentences(text: &str, words: &[Word]) -> Vec<Sentence> {
    let mut cuts = Vec::new();
    let mut iter = text.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if is_terminal(c) {
            let mut end = i + c.len_utf8();
            while let Some(&(j, n)) = iter.peek() {
                if is_terminal(n) || is_closer(n) {
                    end = j + n.len_utf8();
                    iter.next();
                } else {
                    break;
                }
            }
            let rest = &text[end..];
            let at_boundary = rest.chars().next().is_none_or(char::is_whitespace);
            if !at_boundary {
                continue;
            }
            if c == '.' && end == i + 1 {
                let abbreviation = word_ending_at(words, i).is_some_and(|w| {
                    let t = w.text(text);
                    t.chars().count() == 1 || ABBREVIATIONS.contains(&t.to_lowercase().as_str())
                });
                let lowercase_next = rest
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(char::is_lowercase);
                if abbreviation || lowercase_next {
                    continue;
                }
            }
            cuts.push(end);
        } else if c == '\n' {
            let tail = &text[i + 1..];
            let blank = tail.trim_start_matches([' ', '\t', '\r']);
            if blank.starts_with('\n') {
                cuts.push(i);
            }
        }
    }
    cuts.push(text.len());

    let mut out = Vec::new();
    let mut wi = 0;
    let mut from = 0;
    for cut in cuts {
        if cut < from {
            continue;
        }
        while wi < words.len() && words[wi].end <= from {
            wi += 1;
        }
        let first = wi;
        while wi < words.len() && words[wi].end <= cut {
            wi += 1;
        }
        if wi > first {
            out.push(Sentence {
                start: words[first].start,
                end: words[wi - 1].end,
                words: wi - first,
            });
        }
        from = cut;
    }
    out
}

/// Paragraphs (blank-line separated blocks) containing at least one word.
pub fn paragraph_count(text: &str) -> usize {
    let mut count = 0;
    let mut inside = false;
    for line in text.lines() {
        if line.chars().any(char::is_alphanumeric) {
            if !inside {
                count += 1;
                inside = true;
            }
        } else if line.trim().is_empty() {
            inside = false;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &str) -> Vec<&str> {
        words(s).into_iter().map(|w| w.text(s)).collect()
    }

    #[test]
    fn tokenises_words() {
        assert_eq!(
            texts("Don't stop; it's a well-known 3.14 fact -- really."),
            [
                "Don't",
                "stop",
                "it's",
                "a",
                "well-known",
                "3",
                "14",
                "fact",
                "really"
            ]
        );
        assert_eq!(texts("-- 'quoted' --"), ["quoted"]);
        assert_eq!(texts("caf\u{e9} na\u{ef}ve"), ["caf\u{e9}", "na\u{ef}ve"]);
    }

    #[test]
    fn splits_sentences() {
        let s =
            "First one. Second one! Third? Fig. 3 shows e.g. this. Dr. Who is here. etc. and more";
        let w = words(s);
        let sent: Vec<_> = sentences(s, &w)
            .into_iter()
            .map(|x| &s[x.start..x.end])
            .collect();
        assert_eq!(
            sent,
            [
                "First one",
                "Second one",
                "Third",
                "Fig. 3 shows e.g. this",
                "Dr. Who is here. etc. and more"
            ]
        );
    }

    #[test]
    fn decimals_do_not_split() {
        let s = "Pi is 3.14 roughly.";
        assert_eq!(sentences(s, &words(s)).len(), 1);
    }

    #[test]
    fn counts_paragraphs() {
        assert_eq!(paragraph_count("a b\nc\n\n\n d \n\u{e000}\u{e000}\n\n"), 2);
        assert_eq!(paragraph_count(""), 0);
    }
}
