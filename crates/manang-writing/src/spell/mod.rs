//! Spell checking over prose.

mod dictionary;
mod user;

use std::collections::HashMap;

use serde::Serialize;

pub use dictionary::{
    DEFAULT_LANGUAGE, Dictionaries, Dictionary, LanguageInfo, normalise_language,
};
pub use user::UserDictionary;

use crate::diag::{DiagSeverity, Diagnostic, MAX_DIAGNOSTICS};
use crate::error::WritingError;
use crate::text::{CharRange, TextIndex};
use crate::words::{Word, words};

/// Default number of suggestions per misspelling.
pub const DEFAULT_SUGGESTIONS: usize = 5;
/// Upper bound on misspellings reported per request.
pub const MAX_MISSPELLINGS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Misspelling {
    pub word: String,
    pub range: CharRange,
    pub line: usize,
    pub column: usize,
    pub suggestions: Vec<String>,
}

/// Words that are never worth checking: contain digits, acronyms
/// (`NASA`), identifiers with inner capitals (`camelCase`), lone symbols.
fn skippable(word: &str) -> bool {
    if word.chars().any(|c| c.is_ascii_digit() || c.is_numeric()) {
        return true;
    }
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 2 {
        return letters.is_empty();
    }
    if letters.iter().all(|c| c.is_uppercase()) {
        return true;
    }
    letters
        .windows(2)
        .any(|w| w[0].is_lowercase() && w[1].is_uppercase())
}

fn valid_word(dict: &Dictionary, user: &user::UserState, word: &str) -> bool {
    let normalised = word.replace('\u{2019}', "'");
    let word = normalised.as_str();
    if user.contains(word) || dict.check(word) {
        return true;
    }
    if word.contains(['-', '\u{2010}', '\u{2011}']) {
        return word
            .split(['-', '\u{2010}', '\u{2011}'])
            .filter(|p| !p.is_empty())
            .all(|p| p.chars().count() < 2 || skippable(p) || user.contains(p) || dict.check(p));
    }
    false
}

/// Check masked prose. `masked` must come from [`crate::mask::prose`].
pub fn check(
    dict: &Dictionary,
    user: &UserDictionary,
    masked: &str,
    max_suggestions: usize,
) -> Result<Vec<Misspelling>, WritingError> {
    let user = user.read()?;
    let index = TextIndex::new(masked);
    let mut verdicts: HashMap<&str, bool> = HashMap::new();
    let mut out = Vec::new();
    for word in words(masked) {
        let text = word.text(masked);
        let ok = *verdicts
            .entry(text)
            .or_insert_with(|| skippable(text) || valid_word(dict, &user, text));
        if ok {
            continue;
        }
        if out.len() >= MAX_MISSPELLINGS {
            break;
        }
        out.push(misspelling(dict, &index, word, text, max_suggestions));
    }
    Ok(out)
}

fn misspelling(
    dict: &Dictionary,
    index: &TextIndex<'_>,
    word: Word,
    text: &str,
    max_suggestions: usize,
) -> Misspelling {
    let range = index.char_range(word.start..word.end);
    let (line, column) = index.line_col(range.start);
    let mut suggestions = Vec::new();
    if max_suggestions > 0 {
        dict.suggest(&text.replace('\u{2019}', "'"), &mut suggestions);
        suggestions.truncate(max_suggestions);
    }
    Misspelling {
        word: text.to_owned(),
        range,
        line,
        column,
        suggestions,
    }
}

/// Spelling findings as lint diagnostics (rule `spelling`).
pub fn diagnostics(file: Option<&str>, masked: &str, found: &[Misspelling]) -> Vec<Diagnostic> {
    let index = TextIndex::new(masked);
    found
        .iter()
        .take(MAX_DIAGNOSTICS)
        .map(|m| {
            let hints = if m.suggestions.is_empty() {
                vec!["Add it to the user dictionary if it is correct.".to_owned()]
            } else {
                vec![format!("Did you mean: {}?", m.suggestions.join(", "))]
            };
            Diagnostic::new(
                &index,
                file,
                "spelling",
                DiagSeverity::Warning,
                m.range,
                format!("Unknown word `{}`", m.word),
                hints,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::prose;
    use crate::syntax::Syntax;
    use std::path::PathBuf;

    fn setup() -> (Dictionaries, UserDictionary, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        (
            Dictionaries::new(dir.path().join("dictionaries")),
            UserDictionary::new(dir.path().join("user.txt")),
            dir,
        )
    }

    fn run(text: &str, syntax: Syntax, user: &UserDictionary, dicts: &Dictionaries) -> Vec<String> {
        let (_, dict) = dicts.get(None).expect("dict");
        let masked = prose(text, syntax, &[]);
        check(&dict, user, &masked, 0)
            .expect("check")
            .into_iter()
            .map(|m| m.word)
            .collect()
    }

    #[test]
    fn finds_misspellings_with_suggestions() {
        let (dicts, user, _dir) = setup();
        let (_, dict) = dicts.get(None).expect("dict");
        let text = "The quik brown fox";
        let found = check(&dict, &user, text, 5).expect("check");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].word, "quik");
        assert_eq!(found[0].range, CharRange { start: 4, end: 8 });
        assert!(found[0].suggestions.iter().any(|s| s == "quick"));
        assert!(found[0].suggestions.len() <= 5);
    }

    #[test]
    fn skips_code_math_urls_commands_citations() {
        let (dicts, user, _dir) = setup();
        let typst = "A sentense with $xyzzy^2$ and `qwrtp` and #cite(<smithh2020>) at https://exmple.org/qqq and @foobarr.";
        assert_eq!(run(typst, Syntax::Typst, &user, &dicts), ["sentense"]);
        let latex = "A sentense \\textbf{here} \\cite{smithh} $xyzzy$ % commentt\n\\begin{equation} zzqx \\end{equation}";
        assert_eq!(run(latex, Syntax::Latex, &user, &dicts), ["sentense"]);
        let md = "A sentense `qwrtp` $xyzzy$ [text](http://exmple.org)\n\n```\nzzqq\n```\n";
        assert_eq!(run(md, Syntax::Markdown, &user, &dicts), ["sentense"]);
    }

    #[test]
    fn skips_numbers_acronyms_and_identifiers() {
        let (dicts, user, _dir) = setup();
        assert!(
            run(
                "NASA uses camelCase and abc123 and 3rd",
                Syntax::Plain,
                &user,
                &dicts
            )
            .is_empty()
        );
    }

    #[test]
    fn user_words_and_contractions_and_hyphens() {
        let (dicts, user, _dir) = setup();
        assert_eq!(
            run("Manang doesn't well-known", Syntax::Plain, &user, &dicts),
            ["Manang"]
        );
        user.add("Manang").expect("add");
        assert!(
            run(
                "Manang doesn\u{2019}t well-known",
                Syntax::Plain,
                &user,
                &dicts
            )
            .is_empty()
        );
        assert_eq!(
            run("a pre-fooo b", Syntax::Plain, &user, &dicts),
            ["pre-fooo"]
        );
    }

    #[test]
    fn ranges_are_character_offsets_after_unicode() {
        let (dicts, user, _dir) = setup();
        let (_, dict) = dicts.get(None).expect("dict");
        let text = "\u{2192} \u{1F600} wrongg";
        let found = check(&dict, &user, text, 0).expect("check");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range, CharRange { start: 4, end: 10 });
        assert_eq!((found[0].line, found[0].column), (1, 5));
    }

    #[test]
    fn plugin_dir_is_not_required() {
        let dicts = Dictionaries::new(PathBuf::from("/nonexistent"));
        assert!(dicts.get(None).is_ok());
    }
}
