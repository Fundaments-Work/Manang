//! Kernel plugin: command and hook dispatch.
//!
//! Commands (all take and return JSON):
//!
//! | command | input | output |
//! | --- | --- | --- |
//! | `writing.format.document` | `{source, language, options?}` | `{formatted, changed, skipped?}` |
//! | `writing.format.range` | `{source, from, to, language, options?}` | `{formatted, changed, range, replacement, skipped?}` |
//! | `writing.spell.check` | `{text, language?, syntax?, ignore_regions?, max_suggestions?}` | `{language, misspelled[], count}` |
//! | `writing.spell.add-word` / `remove-word` | `{word}` | `{added\|removed, count}` |
//! | `writing.spell.words` | none | `{words[]}` |
//! | `writing.spell.languages` | none | `{default, languages[]}` |
//! | `writing.lint.prose` | `{text, syntax?, rules?, file?, spell?, language?}` | `{source, diagnostics[]}` |
//! | `writing.symbols.search` | `{query, syntax, limit?, category?}` | `{symbols[]}` |
//! | `writing.snippets.list` | `{syntax?}` | `{snippets[], errors[]}` |
//! | `writing.snippets.get` | `{trigger, syntax}` | snippet |
//! | `writing.snippets.expand` | `{trigger, syntax, indent?}` | `{text, tab_stops[], final_cursor, ...}` |
//! | `writing.stats.count` | `{text, syntax?}` | statistics |
//!
//! The `on_lint` hook takes the same input as `writing.lint.prose` (`source`
//! is accepted as an alias of `text`) and returns the same output.
//!
//! The work is CPU-bound and runs on a private Rayon pool; the calling thread
//! blocks until it finishes, so async callers must reach the plugin through
//! `spawn_blocking`, never directly on a Tokio worker.

use std::path::PathBuf;

use manang_kernel::{Context, Hook, Manifest, ManifestError, Payload, Plugin, PluginError};
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde_json::{Map, Value, json};

use crate::diag::Diagnostic;
use crate::error::WritingError;
use crate::format;
use crate::lint::{LintConfig, lint};
use crate::mask::prose;
use crate::snippets::{SnippetStore, expand_body};
use crate::spell::{self, DEFAULT_LANGUAGE, DEFAULT_SUGGESTIONS, Dictionaries, UserDictionary};
use crate::stats;
use crate::symbols::{self, DEFAULT_LIMIT, MathSyntax};
use crate::syntax::Syntax;
use crate::text::CharRange;

const MANIFEST: &str = include_str!("../manifest.toml");

/// File names inside the injected config directory.
pub const USER_DICTIONARY_FILE: &str = "user-dictionary.txt";
pub const SNIPPETS_FILE: &str = "snippets.toml";
pub const DICTIONARIES_DIR: &str = "dictionaries";

/// The writing plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the plugin. `config_dir` is where the user dictionary
/// (`user-dictionary.txt`), snippets (`snippets.toml`) and extra Hunspell
/// dictionaries (`dictionaries/`) live; it is created on first write.
pub fn plugin(config_dir: impl Into<PathBuf>) -> impl Plugin {
    WritingPlugin::new(config_dir.into())
}

/// Exposes the writing tools as kernel commands and the `on_lint` hook.
pub struct WritingPlugin {
    /// `None` when the pool could not start; work then runs inline.
    pool: Option<ThreadPool>,
    dictionaries: Dictionaries,
    user: UserDictionary,
    snippets: SnippetStore,
}

impl WritingPlugin {
    pub fn new(config_dir: PathBuf) -> Self {
        let pool = ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|i| format!("manang-writing-{i}"))
            .build()
            .ok();
        Self {
            pool,
            dictionaries: Dictionaries::new(config_dir.join(DICTIONARIES_DIR)),
            user: UserDictionary::new(config_dir.join(USER_DICTIONARY_FILE)),
            snippets: SnippetStore::new(config_dir.join(SNIPPETS_FILE)),
        }
    }

    /// Run CPU-bound work on the Rayon pool.
    fn run<R: Send>(&self, work: impl FnOnce() -> R + Send) -> R {
        match &self.pool {
            Some(pool) => pool.install(work),
            None => work(),
        }
    }
}

// ---------------------------------------------------------------------------
// Argument helpers

fn json_args(input: Payload) -> Result<Value, WritingError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(WritingError::bad_arg("input", "expected a JSON payload")),
    }
}

fn field<'a>(args: &'a Value, name: &'static str) -> Option<&'a Value> {
    args.get(name).filter(|v| !v.is_null())
}

fn req_str<'a>(args: &'a Value, name: &'static str) -> Result<&'a str, WritingError> {
    field(args, name)
        .and_then(Value::as_str)
        .ok_or_else(|| WritingError::bad_arg(name, "required string"))
}

fn opt_str<'a>(args: &'a Value, name: &'static str) -> Result<Option<&'a str>, WritingError> {
    match field(args, name) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| WritingError::bad_arg(name, "expected a string")),
    }
}

fn req_usize(args: &Value, name: &'static str) -> Result<usize, WritingError> {
    field(args, name)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| WritingError::bad_arg(name, "required non-negative integer"))
}

fn opt_usize(args: &Value, name: &'static str) -> Result<Option<usize>, WritingError> {
    match field(args, name) {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| WritingError::bad_arg(name, "expected a non-negative integer")),
    }
}

fn syntax_arg(
    args: &Value,
    name: &'static str,
    default: Option<Syntax>,
) -> Result<Syntax, WritingError> {
    match opt_str(args, name)? {
        Some(s) => Syntax::parse(s).ok_or_else(|| {
            WritingError::bad_arg(
                name,
                format!("unknown syntax `{s}`; use typst, latex, markdown or plain"),
            )
        }),
        None => default.ok_or_else(|| WritingError::bad_arg(name, "required string")),
    }
}

fn editing_syntax(args: &Value, name: &'static str) -> Result<Syntax, WritingError> {
    match syntax_arg(args, name, None)? {
        s @ (Syntax::Typst | Syntax::Latex | Syntax::Markdown) => Ok(s),
        _ => Err(WritingError::bad_arg(
            name,
            "expected typst, latex or markdown",
        )),
    }
}

fn regions(args: &Value) -> Result<Vec<CharRange>, WritingError> {
    let Some(value) = field(args, "ignore_regions") else {
        return Ok(Vec::new());
    };
    let bad = || {
        WritingError::bad_arg(
            "ignore_regions",
            "expected [{start, end}] or [[start, end]]",
        )
    };
    let items = value.as_array().ok_or_else(bad)?;
    items
        .iter()
        .map(|item| {
            let (start, end) = match item {
                Value::Array(pair) if pair.len() == 2 => (pair[0].as_u64(), pair[1].as_u64()),
                Value::Object(_) => (
                    item.get("start").and_then(Value::as_u64),
                    item.get("end").and_then(Value::as_u64),
                ),
                _ => return Err(bad()),
            };
            let start = usize::try_from(start.ok_or_else(bad)?).map_err(|_| bad())?;
            let end = usize::try_from(end.ok_or_else(bad)?).map_err(|_| bad())?;
            Ok(CharRange {
                start,
                end: end.max(start),
            })
        })
        .collect()
}

fn to_payload(value: impl serde::Serialize) -> Result<Payload, WritingError> {
    serde_json::to_value(value)
        .map(Payload::Json)
        .map_err(|e| WritingError::bad_arg("output", e.to_string()))
}

// ---------------------------------------------------------------------------
// Commands

impl WritingPlugin {
    fn dispatch(&self, command: &str, args: &Value) -> Result<Payload, WritingError> {
        match command {
            "writing.format.document" => {
                let source = req_str(args, "source")?;
                let language = editing_syntax(args, "language")?;
                let options = field(args, "options");
                to_payload(self.run(|| format::format_document(source, language, options))?)
            }
            "writing.format.range" => {
                let source = req_str(args, "source")?;
                let (from, to) = (req_usize(args, "from")?, req_usize(args, "to")?);
                let language = editing_syntax(args, "language")?;
                let options = field(args, "options");
                to_payload(self.run(|| format::format_range(source, from, to, language, options))?)
            }
            "writing.spell.check" => self.spell_check(args),
            "writing.spell.add-word" => {
                let added = self.user.add(req_str(args, "word")?)?;
                to_payload(json!({ "added": added, "count": self.user.words()?.len() }))
            }
            "writing.spell.remove-word" => {
                let removed = self.user.remove(req_str(args, "word")?)?;
                to_payload(json!({ "removed": removed, "count": self.user.words()?.len() }))
            }
            "writing.spell.words" => to_payload(json!({ "words": self.user.words()? })),
            "writing.spell.languages" => to_payload(json!({
                "default": DEFAULT_LANGUAGE,
                "languages": self.dictionaries.available(),
            })),
            "writing.lint.prose" => to_payload(self.lint(args)?),
            "writing.symbols.search" => {
                let query = opt_str(args, "query")?.unwrap_or("");
                let syntax = syntax_arg(args, "syntax", None)?;
                let math = MathSyntax::from_syntax(syntax)
                    .ok_or_else(|| WritingError::bad_arg("syntax", "expected typst or latex"))?;
                let limit = opt_usize(args, "limit")?.unwrap_or(DEFAULT_LIMIT);
                let category = opt_str(args, "category")?;
                to_payload(json!({ "symbols": symbols::search(query, math, limit, category) }))
            }
            "writing.snippets.list" => {
                let syntax = match opt_str(args, "syntax")? {
                    None => None,
                    Some(_) => Some(editing_syntax(args, "syntax")?),
                };
                let view = self.snippets.view();
                to_payload(json!({ "snippets": view.list(syntax), "errors": view.errors() }))
            }
            "writing.snippets.get" => {
                let (trigger, syntax) =
                    (req_str(args, "trigger")?, editing_syntax(args, "syntax")?);
                let view = self.snippets.view();
                let snippet = view.get(trigger, syntax).ok_or_else(|| {
                    WritingError::bad_arg("trigger", format!("no snippet `{trigger}`"))
                })?;
                to_payload(snippet)
            }
            "writing.snippets.expand" => {
                let (trigger, syntax) =
                    (req_str(args, "trigger")?, editing_syntax(args, "syntax")?);
                let indent = opt_str(args, "indent")?.unwrap_or("");
                let view = self.snippets.view();
                let snippet = view.get(trigger, syntax).ok_or_else(|| {
                    WritingError::bad_arg("trigger", format!("no snippet `{trigger}`"))
                })?;
                let expansion = expand_body(&snippet.body, indent)?;
                let mut out = match serde_json::to_value(&expansion)
                    .map_err(|e| WritingError::Snippet(e.to_string()))?
                {
                    Value::Object(map) => map,
                    _ => Map::new(),
                };
                out.insert("trigger".into(), json!(snippet.trigger));
                out.insert("name".into(), json!(snippet.name));
                out.insert("requires".into(), json!(snippet.requires));
                Ok(Payload::Json(Value::Object(out)))
            }
            "writing.stats.count" => {
                let text = req_str(args, "text")?;
                let syntax = syntax_arg(args, "syntax", Some(Syntax::Auto))?;
                to_payload(self.run(|| stats::count(text, syntax)))
            }
            other => Err(WritingError::bad_arg(
                "command",
                format!("unknown command `{other}`"),
            )),
        }
    }

    fn spell_check(&self, args: &Value) -> Result<Payload, WritingError> {
        let text = req_str(args, "text")?;
        let syntax = syntax_arg(args, "syntax", Some(Syntax::Auto))?;
        let ignore = regions(args)?;
        let suggestions = opt_usize(args, "max_suggestions")?
            .unwrap_or(DEFAULT_SUGGESTIONS)
            .min(20);
        let (language, dictionary) = self.dictionaries.get(opt_str(args, "language")?)?;
        let found = self.run(|| {
            let masked = prose(text, syntax, &ignore);
            spell::check(&dictionary, &self.user, &masked, suggestions)
        })?;
        to_payload(json!({ "language": language, "count": found.len(), "misspelled": found }))
    }

    fn lint(&self, args: &Value) -> Result<Value, WritingError> {
        let text = match opt_str(args, "text")? {
            Some(t) => t,
            None => req_str(args, "source")?,
        };
        let syntax = syntax_arg(args, "syntax", Some(Syntax::Auto))?;
        let config = LintConfig::from_json(field(args, "rules"))?;
        let file = opt_str(args, "file")?;
        let with_spelling = field(args, "spell")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let spelling = if with_spelling {
            Some(self.dictionaries.get(opt_str(args, "language")?)?.1)
        } else {
            None
        };
        let diagnostics: Result<Vec<Diagnostic>, WritingError> = self.run(|| {
            let masked = prose(text, syntax, &[]);
            let mut all = lint(&masked, &config, file);
            if let Some(dictionary) = &spelling {
                let found = spell::check(dictionary, &self.user, &masked, 0)?;
                all.extend(spell::diagnostics(file, &masked, &found));
                all.sort_by_key(|d| d.range.map(|r| (r.start, r.end)));
                all.truncate(crate::diag::MAX_DIAGNOSTICS);
            }
            Ok(all)
        });
        Ok(json!({ "source": "writing", "diagnostics": diagnostics? }))
    }
}

impl Plugin for WritingPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        if !command.starts_with("writing.") {
            return Err(PluginError::UnknownCommand(command.to_owned()));
        }
        let args = json_args(input)?;
        match self.dispatch(command, &args) {
            Err(WritingError::BadArgument {
                field: "command", ..
            }) => Err(PluginError::UnknownCommand(command.to_owned())),
            other => other.map_err(PluginError::from),
        }
    }

    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        if hook != Hook::OnLint {
            return Ok(input);
        }
        let args = json_args(input)?;
        Ok(Payload::Json(self.lint(&args)?))
    }
}
