//! The agent's own tools: read the document, list problems, outline, search, and PROPOSE an
//! edit. None of them writes: `doc_propose_edit` returns a proposal the user accepts.
//!
//! They run as the commands `agent.doc-*` and `agent.project-search`, declared to the kernel
//! through the `register_agent_tool` hook like every plugin's tools.
//!
//! The editor's text wins over the disk: the session registers the text the user attached
//! (a file chip, a selection), so a proposal is computed against what is on screen, not against a
//! stale saved file.

use crate::context::Diagnostic;
use crate::error::AgentError;
use crate::text::{byte_to_utf16, clip_chars, markup_problem, outline, utf16_len, utf16_to_byte};
use manang_kernel::{Context, Payload};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

/// Most characters `doc_read` returns.
const READ_CAP: usize = 60_000;
/// Most characters of a replacement.
const REPLACEMENT_CAP: usize = 200_000;
/// Most characters of search output handed to the model.
const SEARCH_CAP: usize = 12_000;

/// A selection the user attached: its text is known even if the saved file differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub path: String,
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// What the tools may look at, set by `agent.start` from the attached chips.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub project_root: Option<PathBuf>,
    /// Project-relative path to the editor's text.
    pub files: HashMap<String, String>,
    pub selections: Vec<Selection>,
    pub diagnostics: Vec<Diagnostic>,
    /// Keep citations, references, labels and math intact (rewrite, shorten, translate).
    pub strict_markup: bool,
}

/// The tool declarations (`{"tools": [...]}`) for the `register_agent_tool` hook.
pub fn declarations() -> Value {
    json!({ "tools": [
        {
            "name": "doc_read",
            "title": "Read part of a file",
            "description": "Read a project file (or a range of its lines) as plain text. Use it when the attached context is not enough. Paths are relative to the project.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Project-relative path, for example main.typ"},
                    "from_line": {"type": "integer", "minimum": 1, "description": "First line, 1-based. Default 1."},
                    "to_line": {"type": "integer", "minimum": 1, "description": "Last line, inclusive. Default: as much as fits."}
                },
                "required": ["path"],
                "additionalProperties": false
            },
            "output_schema": {"type": "object"},
            "command": "agent.doc-read"
        },
        {
            "name": "doc_diagnostics",
            "title": "List the editor's problems",
            "description": "List the compiler and lint problems the editor showed when the request started, optionally for one file.",
            "input_schema": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "additionalProperties": false
            },
            "output_schema": {"type": "object"},
            "command": "agent.doc-diagnostics"
        },
        {
            "name": "doc_outline",
            "title": "Outline of a document",
            "description": "List the headings of a Typst, LaTeX or Markdown file with their levels and line numbers.",
            "input_schema": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            },
            "output_schema": {"type": "object"},
            "command": "agent.doc-outline"
        },
        {
            "name": "doc_propose_edit",
            "title": "Propose a text edit",
            "description": "Propose ONE edit to a file. Nothing is changed: the user reviews it and accepts or rejects it. Give either `search` (text that occurs exactly once in the file) with `replace`, or `from` and `to` (character offsets, as given in the context's range attribute) with `replacement`. Always add a short `explanation`. Keep citations, references, labels, math and braces intact.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Project-relative path of the file"},
                    "search": {"type": "string", "description": "Exact text to replace; must occur exactly once"},
                    "replace": {"type": "string", "description": "Text that replaces `search`"},
                    "from": {"type": "integer", "minimum": 0, "description": "Start offset of the range to replace"},
                    "to": {"type": "integer", "minimum": 0, "description": "End offset (exclusive) of the range to replace"},
                    "replacement": {"type": "string", "description": "Text that replaces the range"},
                    "explanation": {"type": "string", "description": "One sentence: what changes and why"}
                },
                "required": ["path"],
                "additionalProperties": false
            },
            "output_schema": {"type": "object"},
            "command": "agent.doc-propose-edit"
        },
        {
            "name": "project_search",
            "title": "Search the project",
            "description": "Search the text of the project's files for a string. Returns matching lines with their file and line number.",
            "input_schema": {
                "type": "object",
                "properties": {"query": {"type": "string", "minLength": 1}},
                "required": ["query"],
                "additionalProperties": false
            },
            "output_schema": {"type": "object"},
            "command": "agent.project-search"
        }
    ]})
}

fn parse<T: for<'de> Deserialize<'de>>(input: &Value) -> Result<T, AgentError> {
    serde_json::from_value(input.clone()).map_err(|e| AgentError::invalid(e.to_string()))
}

fn clean_path(path: &str) -> Result<String, AgentError> {
    let normal = path.trim().trim_start_matches("./").replace('\\', "/");
    let bad = normal.is_empty()
        || normal.starts_with('/')
        || normal.contains(':')
        || normal.split('/').any(|s| s == ".." || s.is_empty());
    if bad {
        return Err(AgentError::invalid(
            "paths are relative to the project, for example `main.typ` or `chapters/intro.typ`",
        ));
    }
    Ok(normal)
}

/// The text of `path`: the editor's copy when the session registered one, else the disk.
fn file_text(ctx: &Context<'_>, tc: &ToolContext, path: &str) -> Result<String, AgentError> {
    if let Some(text) = tc.files.get(path) {
        return Ok(text.clone());
    }
    match ctx.invoke("project.read-file", Payload::Json(json!({ "path": path }))) {
        Ok(Payload::Json(Value::String(text))) => Ok(text),
        Ok(_) => Err(AgentError::unavailable(
            "the file could not be read as text",
        )),
        Err(e) => Err(AgentError::not_found(format!(
            "{path} could not be read ({e}). Check the path with project_search or doc_outline."
        ))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
    #[serde(default)]
    from_line: Option<usize>,
    #[serde(default)]
    to_line: Option<usize>,
}

pub fn doc_read(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> Result<Value, AgentError> {
    let args: ReadArgs = parse(input)?;
    let path = clean_path(&args.path)?;
    let text = file_text(ctx, tc, &path)?;
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let from = args.from_line.unwrap_or(1).max(1);
    let to = args.to_line.unwrap_or(total).min(total);
    if from > to.max(1) && total > 0 {
        return Err(AgentError::invalid(format!(
            "from_line {from} is past the end: the file has {total} lines"
        )));
    }
    let slice = if total == 0 {
        String::new()
    } else {
        lines[from - 1..to].join("\n")
    };
    let (text, truncated) = clip_chars(&slice, READ_CAP);
    Ok(json!({
        "path": path, "from_line": from, "to_line": to, "total_lines": total,
        "truncated": truncated, "text": text
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagArgs {
    #[serde(default)]
    path: Option<String>,
}

pub fn doc_diagnostics(tc: &ToolContext, input: &Value) -> Result<Value, AgentError> {
    let args: DiagArgs = parse(input)?;
    let wanted = args.path.as_deref().map(clean_path).transpose()?;
    let list: Vec<&Diagnostic> = tc
        .diagnostics
        .iter()
        .filter(|d| wanted.as_deref().is_none_or(|p| d.path == p))
        .collect();
    Ok(json!({ "count": list.len(), "diagnostics": list }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    path: String,
}

pub fn doc_outline(
    ctx: &Context<'_>,
    tc: &ToolContext,
    input: &Value,
) -> Result<Value, AgentError> {
    let args: PathArgs = parse(input)?;
    let path = clean_path(&args.path)?;
    let text = file_text(ctx, tc, &path)?;
    Ok(json!({ "path": path, "headings": outline(&path, &text) }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeArgs {
    path: String,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    from: Option<usize>,
    #[serde(default)]
    to: Option<usize>,
    #[serde(default)]
    replacement: Option<String>,
    #[serde(default)]
    explanation: Option<String>,
}

pub fn doc_propose_edit(
    ctx: &Context<'_>,
    tc: &ToolContext,
    input: &Value,
) -> Result<Value, AgentError> {
    let args: ProposeArgs = parse(input)?;
    let path = clean_path(&args.path)?;
    let by_search = args.search.is_some() || args.replace.is_some();
    let by_range = args.from.is_some() || args.to.is_some() || args.replacement.is_some();
    if by_search == by_range {
        return Err(AgentError::invalid(
            "give either `search` with `replace`, or `from`, `to` and `replacement`",
        ));
    }
    let (from, to, before, after) = if by_search {
        let (Some(search), Some(replace)) = (args.search, args.replace) else {
            return Err(AgentError::invalid("`search` and `replace` go together"));
        };
        if search.is_empty() {
            return Err(AgentError::invalid("`search` must not be empty"));
        }
        let text = file_text(ctx, tc, &path)?;
        let hits: Vec<usize> = text.match_indices(&search).map(|(i, _)| i).collect();
        match hits.as_slice() {
            [] => {
                return Err(AgentError::not_found(
                    "`search` does not occur in the file. Copy it exactly from the context, \
                     including spaces and line breaks, or use from and to.",
                ));
            }
            [byte] => {
                let from = byte_to_utf16(&text, *byte);
                (from, from + utf16_len(&search), search, replace)
            }
            many => {
                return Err(AgentError::new(
                    "ambiguous",
                    format!(
                        "`search` occurs {} times. Add surrounding text so it occurs once, or \
                         use from and to.",
                        many.len()
                    ),
                ));
            }
        }
    } else {
        let (Some(from), Some(to), Some(replacement)) = (args.from, args.to, args.replacement)
        else {
            return Err(AgentError::invalid(
                "`from`, `to` and `replacement` go together",
            ));
        };
        if from > to {
            return Err(AgentError::invalid("`from` must not be after `to`"));
        }
        let known = tc
            .selections
            .iter()
            .find(|s| s.path == path && s.from == from && s.to == to);
        let before = match known {
            Some(selection) => selection.text.clone(),
            None => {
                let text = file_text(ctx, tc, &path)?;
                let len = utf16_len(&text);
                if to > len {
                    return Err(AgentError::invalid(format!(
                        "the range ends at {to} but the file has {len} characters"
                    )));
                }
                match (utf16_to_byte(&text, from), utf16_to_byte(&text, to)) {
                    (Some(a), Some(b)) => text[a..b].to_owned(),
                    _ => {
                        return Err(AgentError::invalid(
                            "the range starts or ends inside a character",
                        ));
                    }
                }
            }
        };
        (from, to, before, replacement)
    };
    if after.chars().count() > REPLACEMENT_CAP {
        return Err(AgentError::invalid("the replacement is too long"));
    }
    if before == after {
        return Err(AgentError::new(
            "no_change",
            "the replacement is identical to the current text",
        ));
    }
    if tc.strict_markup
        && let Some(problem) = markup_problem(&before, &after)
    {
        return Err(AgentError::new(
            "markup_broken",
            format!(
                "{problem}. Keep citations, references, labels and math exactly and try again."
            ),
        ));
    }
    Ok(json!({
        "proposal": {
            "kind": "text_edit",
            "path": path,
            "from": from,
            "to": to,
            "before": before,
            "after": after,
            "explanation": args.explanation.unwrap_or_default(),
        },
        "message": "Proposed to the user, who will accept or reject it. Do not repeat it."
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
}

pub fn project_search(
    ctx: &Context<'_>,
    tc: &ToolContext,
    input: &Value,
) -> Result<Value, AgentError> {
    let args: SearchArgs = parse(input)?;
    if args.query.is_empty() {
        return Err(AgentError::invalid("`query` must not be empty"));
    }
    let Some(root) = &tc.project_root else {
        return Err(AgentError::unavailable("no project is open"));
    };
    let out = ctx
        .invoke(
            "search.text",
            Payload::Json(json!({
                "root": root.to_string_lossy(),
                "query": args.query,
                "limit": 20,
                "context_lines": 1
            })),
        )
        .map_err(|e| AgentError::unavailable(format!("the search failed: {e}")))?;
    let Payload::Json(value) = out else {
        return Err(AgentError::unavailable("the search returned no data"));
    };
    let (text, truncated) = clip_chars(&value.to_string(), SEARCH_CAP);
    if truncated {
        Ok(json!({ "truncated": true, "results": text }))
    } else {
        Ok(value)
    }
}
