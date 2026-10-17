//! Typst formatting through `typstyle-core`.
//!
//! typstyle is the community-standard Typst formatter and ships as a library
//! (`typstyle-core`, Apache-2.0), built on the same `typst-syntax` version as
//! the compiler. This wrapper adds two guarantees on top of it:
//!
//! * **Idempotence by construction.** The output is only returned once
//!   formatting it again changes nothing; otherwise the document is returned
//!   untouched with a `skipped` reason.
//! * **No syntax regressions.** Output that does not parse cleanly is rejected
//!   the same way.

use serde_json::Value;
use typst_syntax::Source;
use typstyle_core::{Config, LineEnding, Typstyle, WrapMode};

use crate::error::WritingError;
use crate::text::TextIndex;

/// How many extra passes may be spent reaching a fixpoint.
const MAX_PASSES: usize = 3;

pub fn config_from_json(value: Option<&Value>) -> Result<Config, WritingError> {
    let mut config = Config::default();
    let map = match value {
        None | Some(Value::Null) => return Ok(config),
        Some(Value::Object(map)) => map,
        Some(_) => return Err(WritingError::bad_arg("options", "expected an object")),
    };
    for (key, v) in map {
        let bad = |what: &str| WritingError::bad_arg("options", format!("`{key}` must be {what}"));
        let uint = |lo: u64, hi: u64| {
            v.as_u64()
                .filter(|n| (lo..=hi).contains(n))
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| bad(&format!("an integer from {lo} to {hi}")))
        };
        match key.as_str() {
            "tab_spaces" => config.tab_spaces = uint(1, 16)?,
            "max_width" => config.max_width = uint(20, 1000)?,
            "blank_lines_upper_bound" => config.blank_lines_upper_bound = uint(0, 100)?,
            "collapse_markup_spaces" => {
                config.collapse_markup_spaces = v.as_bool().ok_or_else(|| bad("a boolean"))?;
            }
            "reorder_import_items" => {
                config.reorder_import_items = v.as_bool().ok_or_else(|| bad("a boolean"))?;
            }
            "wrap_mode" => {
                config.wrap_mode = match v.as_str() {
                    Some("none") => WrapMode::None,
                    Some("fill") => WrapMode::Fill,
                    Some("sentence") => WrapMode::Sentence,
                    Some("fill-sentence") => WrapMode::FillSentence,
                    _ => return Err(bad("none, fill, sentence or fill-sentence")),
                };
            }
            "line_ending" => {
                config.line_ending = match v.as_str() {
                    Some("lf") => LineEnding::Lf,
                    Some("crlf") => LineEnding::Crlf,
                    Some("preserve") => LineEnding::Preserve,
                    _ => return Err(bad("lf, crlf or preserve")),
                };
            }
            other => {
                return Err(WritingError::bad_arg(
                    "options",
                    format!("unknown typst option `{other}`"),
                ));
            }
        }
    }
    Ok(config)
}

/// Result of a Typst format request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub formatted: String,
    /// Why the source was returned unchanged although formatting succeeded.
    pub skipped: Option<String>,
}

fn once(typstyle: &Typstyle, text: &str) -> Result<String, WritingError> {
    typstyle
        .format_text(text)
        .render()
        .map_err(|e| WritingError::Syntax(e.to_string()))
}

fn parses_cleanly(text: &str) -> bool {
    !typst_syntax::parse(text).diagnosis().errors
}

pub fn format_document(source: &str, config: &Config) -> Result<Outcome, WritingError> {
    let typstyle = Typstyle::new(config.clone());
    let mut current = once(&typstyle, source)?;
    for _ in 0..=MAX_PASSES {
        let next = once(&typstyle, &current)?;
        if next == current {
            return Ok(if parses_cleanly(&current) {
                Outcome {
                    formatted: current,
                    skipped: None,
                }
            } else {
                unchanged(source, "formatted output did not parse cleanly")
            });
        }
        current = next;
    }
    Ok(unchanged(
        source,
        "formatting did not reach a stable result",
    ))
}

fn unchanged(source: &str, reason: &str) -> Outcome {
    Outcome {
        formatted: source.to_owned(),
        skipped: Some(reason.to_owned()),
    }
}

/// Format the smallest syntax node covering the character range. Returns the
/// whole updated text plus the replaced character range and its replacement.
pub fn format_range(
    source: &str,
    from: usize,
    to: usize,
    config: &Config,
) -> Result<RangeOutcome, WritingError> {
    let index = TextIndex::new(source);
    let (from_b, to_b) = (index.char_to_byte(from), index.char_to_byte(to));
    let typstyle = Typstyle::new(config.clone());
    let result = typstyle
        .format_source_range(Source::detached(source), from_b..to_b)
        .map_err(|e| WritingError::Syntax(e.to_string()))?;
    let replaced = result.source_range.clone();
    let mut formatted = String::with_capacity(source.len());
    formatted.push_str(&source[..replaced.start]);
    formatted.push_str(&result.content);
    formatted.push_str(&source[replaced.end..]);
    if !parses_cleanly(&formatted) && parses_cleanly(source) {
        return Ok(RangeOutcome {
            range: index.char_range(replaced.clone()),
            replacement: source[replaced].to_owned(),
            formatted: source.to_owned(),
            skipped: Some("formatted range did not parse cleanly".to_owned()),
        });
    }
    Ok(RangeOutcome {
        range: index.char_range(replaced),
        replacement: result.content,
        formatted,
        skipped: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeOutcome {
    pub formatted: String,
    pub range: crate::text::CharRange,
    pub replacement: String,
    pub skipped: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> String {
        format_document(s, &Config::default())
            .expect("formats")
            .formatted
    }

    #[test]
    fn formats_and_is_idempotent() {
        let src = "#let f(x,y)=x+y\n#f( 1,2 )\n\n\n\n= Heading\nText   here\n";
        let once = fmt(src);
        assert_ne!(once, src);
        assert_eq!(fmt(&once), once);
    }

    #[test]
    fn syntax_errors_are_reported() {
        assert!(matches!(
            format_document("#let x = (", &Config::default()),
            Err(WritingError::Syntax(_))
        ));
    }

    #[test]
    fn options_are_validated() {
        let cfg = config_from_json(Some(&serde_json::json!({"tab_spaces": 4, "max_width": 60})))
            .expect("valid");
        assert_eq!((cfg.tab_spaces, cfg.max_width), (4, 60));
        for bad in [
            serde_json::json!({"tab_spaces": 0}),
            serde_json::json!({"wrap_mode": "zigzag"}),
            serde_json::json!({"nope": 1}),
            serde_json::json!(3),
        ] {
            assert!(config_from_json(Some(&bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn range_formats_only_the_covering_node() {
        let src = "#let a=1\n#let b=2\n";
        let out = format_range(src, 0, 8, &Config::default()).expect("range");
        assert_eq!(out.range.start, 0);
        assert!(out.formatted.contains("#let a = 1"));
        assert!(out.formatted.contains("#let b=2"), "{}", out.formatted);
    }
}
