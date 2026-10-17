//! Snippets with structured tab stops.
//!
//! Templates use `${1:placeholder}` tab stops. Expansion returns plain text
//! plus the character ranges of every stop, so the editor can insert the text
//! and drive Tab navigation without parsing anything itself.
//!
//! Built-in snippets live in `data/snippets.toml`; users add their own in
//! `<config_dir>/snippets.toml` (same format, hot-reloaded when the file
//! changes). A user snippet with the same trigger and syntax replaces the
//! built-in one.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::WritingError;
use crate::syntax::Syntax;
use crate::text::CharRange;

const BUILTIN_SOURCE: &str = include_str!("../data/snippets.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Builtin,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snippet {
    pub trigger: String,
    pub aliases: Vec<String>,
    pub syntax: Syntax,
    pub name: String,
    pub description: String,
    pub body: String,
    pub requires: Option<String>,
    pub source: Source,
}

impl Snippet {
    fn matches(&self, trigger: &str) -> bool {
        self.trigger == trigger || self.aliases.iter().any(|a| a == trigger)
    }
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    snippet: Vec<Raw>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    trigger: String,
    #[serde(default)]
    aliases: Vec<String>,
    syntax: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    body: String,
    #[serde(default)]
    requires: Option<String>,
}

/// Snippets parsed from a TOML document, plus a message per rejected entry.
pub(crate) fn parse_file(text: &str, source: Source) -> (Vec<Snippet>, Vec<String>) {
    let file: File = match toml::from_str(text) {
        Ok(f) => f,
        Err(e) => return (Vec::new(), vec![format!("snippets file: {e}")]),
    };
    let mut snippets = Vec::new();
    let mut errors = Vec::new();
    for raw in file.snippet {
        let label = format!("snippet `{}` ({})", raw.trigger, raw.syntax);
        let Some(syntax) = Syntax::parse(&raw.syntax)
            .filter(|s| matches!(s, Syntax::Typst | Syntax::Latex | Syntax::Markdown))
        else {
            errors.push(format!("{label}: syntax must be typst, latex or markdown"));
            continue;
        };
        let triggers = std::iter::once(&raw.trigger).chain(&raw.aliases);
        if triggers
            .clone()
            .any(|t| t.is_empty() || t.chars().any(char::is_whitespace))
        {
            errors.push(format!(
                "{label}: triggers must be non-empty and without whitespace"
            ));
            continue;
        }
        if let Err(e) = parse_template(&raw.body) {
            errors.push(format!("{label}: {e}"));
            continue;
        }
        snippets.push(Snippet {
            name: if raw.name.is_empty() {
                raw.trigger.clone()
            } else {
                raw.name
            },
            trigger: raw.trigger,
            aliases: raw.aliases,
            syntax,
            description: raw.description,
            body: raw.body,
            requires: raw.requires,
            source,
        });
    }
    (snippets, errors)
}

fn builtin() -> &'static [Snippet] {
    static BUILTIN: OnceLock<Vec<Snippet>> = OnceLock::new();
    BUILTIN.get_or_init(|| parse_file(BUILTIN_SOURCE, Source::Builtin).0)
}

// ---------------------------------------------------------------------------
// Templates

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Stop { index: u32, default: String },
}

fn parse_template(body: &str) -> Result<Vec<Piece>, String> {
    let chars: Vec<char> = body.chars().collect();
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String, pieces: &mut Vec<Piece>| {
        if !text.is_empty() {
            pieces.push(Piece::Text(std::mem::take(text)));
        }
    };
    while i < chars.len() {
        match chars[i] {
            '\\' if chars.get(i + 1) == Some(&'$')
                && chars
                    .get(i + 2)
                    .is_some_and(|c| *c == '{' || c.is_ascii_digit()) =>
            {
                text.push('$');
                i += 2;
            }
            '$' if chars.get(i + 1) == Some(&'{') => {
                let mut j = i + 2;
                let digits_start = j;
                while chars.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                if j == digits_start {
                    return Err("tab stop needs a number, like ${1:text}".to_owned());
                }
                let index: u32 = chars[digits_start..j]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .map_err(|_| "tab stop number is too large".to_owned())?;
                let mut default = String::new();
                match chars.get(j) {
                    Some('}') => j += 1,
                    Some(':') => {
                        j += 1;
                        let mut depth = 1usize;
                        loop {
                            let Some(&c) = chars.get(j) else {
                                return Err("unterminated ${...} tab stop".to_owned());
                            };
                            j += 1;
                            match c {
                                '{' => depth += 1,
                                '}' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            default.push(c);
                        }
                    }
                    _ => return Err("expected `:` or `}` after the tab stop number".to_owned()),
                }
                flush(&mut text, &mut pieces);
                pieces.push(Piece::Stop { index, default });
                i = j;
            }
            '$' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => {
                let mut j = i + 1;
                while chars.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                let index: u32 = chars[i + 1..j]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .map_err(|_| "tab stop number is too large".to_owned())?;
                flush(&mut text, &mut pieces);
                pieces.push(Piece::Stop {
                    index,
                    default: String::new(),
                });
                i = j;
            }
            c => {
                text.push(c);
                i += 1;
            }
        }
    }
    flush(&mut text, &mut pieces);
    Ok(pieces)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TabStop {
    pub index: u32,
    /// Default text, already present in the expansion at every range.
    pub placeholder: String,
    /// Where the stop appears (mirrors give several ranges).
    pub ranges: Vec<CharRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Expansion {
    pub text: String,
    /// Stops to visit with Tab, in order (`$1`, `$2`, ...). `$0` is not here.
    pub tab_stops: Vec<TabStop>,
    /// Character offset to leave the cursor at after the last stop (`$0`, or
    /// the end of the text).
    pub final_cursor: usize,
}

/// Prefix every line after the first with `indent`.
fn indented(text: &str, indent: &str) -> String {
    if indent.is_empty() {
        text.to_owned()
    } else {
        text.replace('\n', &format!("\n{indent}"))
    }
}

pub fn expand_body(body: &str, indent: &str) -> Result<Expansion, WritingError> {
    let pieces = parse_template(body).map_err(WritingError::Snippet)?;
    let mut text = String::new();
    let mut at = 0usize;
    let mut stops: Vec<TabStop> = Vec::new();
    let mut final_cursor = None;
    for piece in &pieces {
        match piece {
            Piece::Text(t) => {
                let t = indented(t, indent);
                at += t.chars().count();
                text.push_str(&t);
            }
            Piece::Stop { index, default } => {
                let existing = stops.iter().position(|s| s.index == *index);
                let shown = match existing {
                    Some(p) if default.is_empty() => stops[p].placeholder.clone(),
                    _ => indented(default, indent),
                };
                let len = shown.chars().count();
                let range = CharRange {
                    start: at,
                    end: at + len,
                };
                text.push_str(&shown);
                at += len;
                match existing {
                    Some(p) => stops[p].ranges.push(range),
                    None => stops.push(TabStop {
                        index: *index,
                        placeholder: shown,
                        ranges: vec![range],
                    }),
                }
            }
        }
    }
    if let Some(p) = stops.iter().position(|s| s.index == 0) {
        let stop = stops.remove(p);
        final_cursor = stop.ranges.first().map(|r| r.start);
    }
    stops.sort_by_key(|s| s.index);
    Ok(Expansion {
        final_cursor: final_cursor.unwrap_or(at),
        text,
        tab_stops: stops,
    })
}

// ---------------------------------------------------------------------------
// Store

struct Loaded {
    stamp: Option<(SystemTime, u64)>,
    snippets: Vec<Snippet>,
    errors: Vec<String>,
}

/// Built-in plus user snippets; the user file is re-read when it changes.
pub struct SnippetStore {
    path: PathBuf,
    cache: Mutex<Option<Arc<Loaded>>>,
}

/// A snapshot for one request.
pub struct View {
    loaded: Arc<Loaded>,
}

impl SnippetStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(None),
        }
    }

    fn stamp(&self) -> Option<(SystemTime, u64)> {
        let meta = std::fs::metadata(&self.path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    pub fn view(&self) -> View {
        let stamp = self.stamp();
        let mut cache = self.cache.lock();
        if let Some(hit) = cache.as_ref().filter(|l| l.stamp == stamp) {
            return View {
                loaded: Arc::clone(hit),
            };
        }
        let (snippets, errors) = match std::fs::read_to_string(&self.path) {
            Ok(text) => parse_file(&text, Source::User),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), Vec::new()),
            Err(e) => (
                Vec::new(),
                vec![format!("cannot read {}: {e}", self.path.display())],
            ),
        };
        let loaded = Arc::new(Loaded {
            stamp,
            snippets,
            errors,
        });
        *cache = Some(Arc::clone(&loaded));
        View { loaded }
    }
}

impl View {
    /// Problems found in the user's snippets file.
    pub fn errors(&self) -> &[String] {
        &self.loaded.errors
    }

    /// All snippets for `syntax` (every syntax when `None`), user ones first
    /// and shadowing built-ins with the same trigger.
    pub fn list(&self, syntax: Option<Syntax>) -> Vec<&Snippet> {
        let want = |s: &&Snippet| syntax.is_none_or(|x| s.syntax == x);
        let mut out: Vec<&Snippet> = self.loaded.snippets.iter().filter(want).collect();
        for b in builtin().iter().filter(want) {
            let shadowed = self
                .loaded
                .snippets
                .iter()
                .any(|u| u.syntax == b.syntax && u.matches(&b.trigger));
            if !shadowed {
                out.push(b);
            }
        }
        out.sort_by(|a, b| {
            a.trigger
                .cmp(&b.trigger)
                .then(format!("{:?}", a.syntax).cmp(&format!("{:?}", b.syntax)))
        });
        out
    }

    pub fn get(&self, trigger: &str, syntax: Syntax) -> Option<&Snippet> {
        self.loaded
            .snippets
            .iter()
            .chain(builtin())
            .find(|s| s.syntax == syntax && s.matches(trigger))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_stops_with_char_ranges() {
        let e = expand_body("a ${1:bc}\u{e9} $2 ${3:x}$0 end", "").expect("expands");
        assert_eq!(e.text, "a bc\u{e9}  x end");
        assert_eq!(
            e.tab_stops,
            vec![
                TabStop {
                    index: 1,
                    placeholder: "bc".into(),
                    ranges: vec![CharRange { start: 2, end: 4 }]
                },
                TabStop {
                    index: 2,
                    placeholder: String::new(),
                    ranges: vec![CharRange { start: 6, end: 6 }]
                },
                TabStop {
                    index: 3,
                    placeholder: "x".into(),
                    ranges: vec![CharRange { start: 7, end: 8 }]
                },
            ]
        );
        assert_eq!(e.final_cursor, 8);
    }

    #[test]
    fn mirrors_share_text_and_cursor_defaults_to_end() {
        let e = expand_body("\\begin{${1:env}} \\end{$1}", "").expect("expands");
        assert_eq!(e.text, "\\begin{env} \\end{env}");
        assert_eq!(e.tab_stops.len(), 1);
        assert_eq!(e.tab_stops[0].ranges.len(), 2);
        assert_eq!(e.final_cursor, e.text.chars().count());
    }

    #[test]
    fn nested_braces_and_escaped_dollars() {
        let e = expand_body("${1:\\frac{a}{b}} \\${2} costs $ 5", "").expect("expands");
        assert_eq!(e.text, "\\frac{a}{b} ${2} costs $ 5");
        assert_eq!(e.tab_stops.len(), 1);
    }

    #[test]
    fn indent_applies_after_newlines() {
        let e = expand_body("a\n${1:b\nc}\n$0", "  ").expect("expands");
        assert_eq!(e.text, "a\n  b\n  c\n  ");
        assert_eq!(e.tab_stops[0].ranges[0], CharRange { start: 4, end: 9 });
    }

    #[test]
    fn malformed_templates_error() {
        for bad in ["${1:oops", "${x}", "${1", "${}"] {
            assert!(expand_body(bad, "").is_err(), "{bad}");
        }
    }

    #[test]
    fn builtins_cover_the_required_snippets_in_every_syntax() {
        let store = SnippetStore::new(PathBuf::from("/nonexistent/snippets.toml"));
        let view = store.view();
        for trigger in [
            "theorem",
            "equation",
            "figure",
            "table",
            "cite",
            "algorithm",
        ] {
            for syntax in [Syntax::Typst, Syntax::Latex, Syntax::Markdown] {
                assert!(view.get(trigger, syntax).is_some(), "{trigger} {syntax:?}");
            }
        }
        assert_eq!(
            view.get("thm", Syntax::Latex).map(|s| s.trigger.as_str()),
            Some("theorem")
        );
        assert!(view.errors().is_empty());
        assert!(builtin().iter().all(|s| expand_body(&s.body, "").is_ok()));
    }

    #[test]
    fn user_snippets_load_shadow_and_reload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("snippets.toml");
        std::fs::write(
            &path,
            "[[snippet]]\ntrigger = \"cite\"\nsyntax = \"typst\"\nbody = \"#cite(<${1:key}>)\"\n\n\
             [[snippet]]\ntrigger = \"bad one\"\nsyntax = \"typst\"\nbody = \"x\"\n\n\
             [[snippet]]\ntrigger = \"broken\"\nsyntax = \"typst\"\nbody = \"${1:\"\n",
        )
        .expect("write");
        let store = SnippetStore::new(path.clone());
        let view = store.view();
        let cite = view.get("cite", Syntax::Typst).expect("user cite");
        assert_eq!(cite.source, Source::User);
        assert_eq!(
            view.get("cite", Syntax::Latex).map(|s| s.source),
            Some(Source::Builtin)
        );
        assert_eq!(view.errors().len(), 2, "{:?}", view.errors());
        let typst_cites: Vec<_> = view
            .list(Some(Syntax::Typst))
            .into_iter()
            .filter(|s| s.trigger == "cite")
            .collect();
        assert_eq!(typst_cites.len(), 1);

        std::fs::write(
            &path,
            "[[snippet]]\ntrigger = \"zz\"\nsyntax = \"latex\"\nbody = \"zz${1}\"\n",
        )
        .expect("rewrite");
        let view = store.view();
        assert!(view.get("zz", Syntax::Latex).is_some());
        assert_eq!(
            view.get("cite", Syntax::Typst).map(|s| s.source),
            Some(Source::Builtin)
        );
    }

    #[test]
    fn invalid_toml_is_reported_not_fatal() {
        let (snips, errors) = parse_file("not = [valid", Source::User);
        assert!(snips.is_empty());
        assert_eq!(errors.len(), 1);
    }
}
