//! Word count and document statistics over prose.

use serde::Serialize;

use crate::mask::{OPAQUE, prose};
use crate::syntax::Syntax;
use crate::words::{paragraph_count, sentences, words};

/// Silent-reading speed used for the reading-time estimate.
const READING_WPM: f64 = 238.0;
/// Speaking speed used for the speaking-time estimate.
const SPEAKING_WPM: f64 = 150.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stats {
    pub words: usize,
    /// Characters of prose, counting a run of spaces as one.
    pub characters: usize,
    pub characters_no_spaces: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    pub reading_time_seconds: u64,
    pub reading_time_minutes: f64,
    pub speaking_time_seconds: u64,
}

fn seconds(words: usize, wpm: f64) -> u64 {
    // Word counts are far below 2^52, so the float round trip is exact.
    (words as f64 / wpm * 60.0).round() as u64
}

/// Statistics of `text`, excluding markup, math, comments and code.
pub fn count(text: &str, syntax: Syntax) -> Stats {
    let masked = prose(text, syntax, &[]);
    let all_words = words(&masked);
    let (mut characters, mut no_spaces, mut pending_space) = (0usize, 0usize, false);
    for c in masked.chars() {
        if c == OPAQUE || c.is_whitespace() {
            pending_space = characters > 0;
        } else {
            if pending_space {
                characters += 1;
                pending_space = false;
            }
            characters += 1;
            no_spaces += 1;
        }
    }
    let minutes = seconds(all_words.len(), READING_WPM) as f64 / 60.0;
    Stats {
        words: all_words.len(),
        characters,
        characters_no_spaces: no_spaces,
        sentences: sentences(&masked, &all_words).len(),
        paragraphs: paragraph_count(&masked),
        reading_time_seconds: seconds(all_words.len(), READING_WPM),
        reading_time_minutes: (minutes * 10.0).round() / 10.0,
        speaking_time_seconds: seconds(all_words.len(), SPEAKING_WPM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_plain_text() {
        let s = count(
            "Hello world. This is two sentences!\n\nSecond paragraph.",
            Syntax::Plain,
        );
        assert_eq!(s.words, 8);
        assert_eq!(s.sentences, 3);
        assert_eq!(s.paragraphs, 2);
        assert_eq!(s.characters_no_spaces, 46);
    }

    #[test]
    fn typst_excludes_markup_math_comments() {
        let s = count(
            "= Title\nSome *bold* words $x + y$ here #cite(<k>) // ignored words\n#let a = 1\n",
            Syntax::Typst,
        );
        assert_eq!(s.words, 5, "{s:?}");
    }

    #[test]
    fn latex_excludes_commands_math_comments() {
        let s = count(
            "\\section{Title} Some \\textbf{bold} words $x+y$ here \\cite{k} % ignored words\n\\begin{equation}a=b\\end{equation}",
            Syntax::Latex,
        );
        assert_eq!(s.words, 5, "{s:?}");
    }

    #[test]
    fn markdown_excludes_code_math_and_urls() {
        let s = count(
            "# Title\n\nSome **bold** words `code here` and $x$ see https://a.b/c here.\n\n```\nfenced words\n```\n",
            Syntax::Markdown,
        );
        assert_eq!(s.words, 7, "{s:?}");
    }

    #[test]
    fn empty_and_reading_time() {
        let s = count("", Syntax::Auto);
        assert_eq!(
            (s.words, s.characters, s.sentences, s.paragraphs),
            (0, 0, 0, 0)
        );
        let long = "word ".repeat(476);
        let s = count(&long, Syntax::Plain);
        assert_eq!(s.reading_time_seconds, 120);
        assert!((s.reading_time_minutes - 2.0).abs() < f64::EPSILON);
    }
}
