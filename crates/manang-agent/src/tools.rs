//! The agent's own tools: eight verbs with plain-text results.
//!
//! `list_files`, `read_file`, `search`, `outline`, `diagnostics` and `bib_search` read;
//! `edit_text` PROPOSES (it returns a proposal the author accepts, nothing is written);
//! `ask_user` ends the model's turn with a question. They run as the commands `agent.<verb>`,
//! declared to the kernel through the `register_agent_tool` hook like every plugin's tools.
//!
//! Results are text, not JSON objects (tokens; models read text well), capped at about 6,000
//! tokens, and every cap says how to get the rest. Errors are text too, and say what to do next.
//!
//! The editor's text wins over the disk: the UI sends the open buffers, and the session's
//! pending proposals are applied on top (the pending overlay), so a second edit is computed
//! against the first one's result and a fix can be checked before the author accepts it.

use crate::context::Diagnostic;
use crate::edit::{self, Pair};
use crate::text::{balance_problem, clip_chars, introduced_keys, outline, pair_problem};
use manang_kernel::{Context, Payload};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Characters of one result (6,000 tokens at about 3.5 characters per token).
pub const RESULT_CAP: usize = 21_000;
/// Default lines of `read_file`.
const READ_LINES: usize = 400;
/// Entries of `list_files`.
const LIST_MAX: usize = 200;
/// Files bigger than this are not listed or read.
const MAX_FILE_BYTES: u64 = 2_000_000;
/// Characters of all replacements of one proposal.
const REPLACEMENT_CAP: usize = 200_000;
const SEARCH_DEFAULT: usize = 20;
const PROBLEMS_MAX: usize = 20;

/// A proposal of this session that is waiting for the author, oldest first in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEdit {
    pub path: String,
    pub pairs: Vec<Pair>,
}

/// What the tools may look at, set by `agent.start` for the running session.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub session_id: String,
    pub project_root: Option<PathBuf>,
    pub active: Option<String>,
    /// Open documents' text: path to text, unsaved changes included.
    pub buffers: BTreeMap<String, String>,
    /// The editor's problems when the message was sent.
    pub diagnostics: Vec<Diagnostic>,
    /// The author's message of this turn: a key it names may be changed.
    pub user_text: String,
    /// Filled by the plugin at each call from the session's pending proposals.
    pub pending: Vec<PendingEdit>,
}

type ToolResult = Result<Value, String>;

fn text(s: impl Into<String>) -> ToolResult {
    Ok(Value::String(s.into()))
}

/// The tool declarations (`{"tools": [...]}`) for the `register_agent_tool` hook.
pub fn declarations() -> Value {
    let out = json!({"type": "string"});
    json!({ "tools": [
        {
            "name": "list_files",
            "title": "List files",
            "description": "List the project's text sources (.typ .tex .md .bib and figures) as an indented tree with sizes. Use it to find what exists before reading or searching. At most 200 entries; narrow with dir.",
            "input_schema": {"type":"object","properties":{"dir":{"type":"string","description":"Project-relative folder; default the project root"}},"additionalProperties":false},
            "output_schema": out,
            "command": "agent.list-files"
        },
        {
            "name": "read_file",
            "title": "Read a file",
            "description": "Read a project file as numbered lines (`12| text`). Defaults to the first 400 lines and always reports `lines A-B of N`; ask for more with from_line and to_line. Reads the editor's text when the file is open.",
            "input_schema": {"type":"object","properties":{"path":{"type":"string","description":"Project-relative path, e.g. main.typ"},"from_line":{"type":"integer","minimum":1},"to_line":{"type":"integer","minimum":1}},"required":["path"],"additionalProperties":false},
            "output_schema": out,
            "command": "agent.read-file"
        },
        {
            "name": "search",
            "title": "Search the project",
            "description": "Search all project text files for a literal string, or a regex when regex is true. Returns `path:line: text` lines, at most max (default 20), with the total. Case insensitive by default.",
            "input_schema": {"type":"object","properties":{"query":{"type":"string","minLength":1},"regex":{"type":"boolean"},"path":{"type":"string","description":"Limit to one file or folder"},"case_sensitive":{"type":"boolean"},"max":{"type":"integer","minimum":1,"maximum":100}},"required":["query"],"additionalProperties":false},
            "output_schema": out,
            "command": "agent.search"
        },
        {
            "name": "outline",
            "title": "Outline",
            "description": "The headings of one file, or of every file when path is omitted, with levels and line numbers. A cheap way to orient before reading.",
            "input_schema": {"type":"object","properties":{"path":{"type":"string","description":"File; omit for every file"}},"additionalProperties":false},
            "output_schema": out,
            "command": "agent.outline"
        },
        {
            "name": "diagnostics",
            "title": "Problems",
            "description": "Current compiler and lint problems as `severity path:line:col message` with two lines of context. With with_pending the project is checked as if the pending edits were accepted, and the result says whether the problem count went up or down: propose, check, adjust (at most twice).",
            "input_schema": {"type":"object","properties":{"path":{"type":"string"},"with_pending":{"type":"boolean","description":"Check the project as if the pending edits were accepted"}},"additionalProperties":false},
            "output_schema": out,
            "command": "agent.diagnostics"
        },
        {
            "name": "bib_search",
            "title": "Search the bibliography",
            "description": "Search the project's bibliography by author, title, year or key. Returns up to max (default 8) lines `key | authors (year) | title`. Only keys it returns may be cited.",
            "input_schema": {"type":"object","properties":{"query":{"type":"string","minLength":1},"max":{"type":"integer","minimum":1,"maximum":25}},"required":["query"],"additionalProperties":false},
            "output_schema": out,
            "command": "agent.bib-search"
        },
        {
            "name": "edit_text",
            "title": "Propose an edit",
            "description": "Propose changes to one file as exact search-and-replace pairs. Nothing is changed: the author reviews the result as a diff and accepts or rejects it. Each old must appear exactly once in the file as you read it (include enough surrounding text to be unique, but keep it short) and must not include line-number prefixes; new replaces it. To insert, quote an anchor and repeat it in new with the addition. Several pairs in one call form one proposal. note is one sentence for the author. The host rejects a proposal it cannot locate, that unbalances braces, math or environments, or that changes citation keys, labels or references the author did not ask to change, and says which. Creating, deleting and renaming files is not possible.",
            "input_schema": {"type":"object","properties":{"path":{"type":"string"},"edits":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"object","properties":{"old":{"type":"string","minLength":1},"new":{"type":"string"}},"required":["old","new"],"additionalProperties":false}},"note":{"type":"string","description":"One sentence: what changes and why"}},"required":["path","edits"],"additionalProperties":false},
            "output_schema": out,
            "command": "agent.edit-text"
        },
        {
            "name": "ask_user",
            "title": "Ask the author",
            "description": "Ask the author one short question, optionally with up to four answer choices, and wait. Use it before anything destructive or wide, and for real ambiguity. Ends your turn; the answer arrives as the next message.",
            "input_schema": {"type":"object","properties":{"question":{"type":"string","minLength":1},"choices":{"type":"array","maxItems":4,"items":{"type":"string"}}},"required":["question"],"additionalProperties":false},
            "output_schema": out,
            "command": "agent.ask-user"
        }
    ]})
}

fn parse<T: for<'de> Deserialize<'de>>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|e| format!("invalid arguments: {e}"))
}

/// A project-relative path, or why it is not allowed.
pub fn clean_path(path: &str) -> Result<String, String> {
    let normal = path.trim().trim_start_matches("./").replace('\\', "/");
    let bad = normal.is_empty()
        || normal.starts_with('/')
        || normal.contains(':')
        || normal.split('/').any(|s| s == ".." || s.is_empty());
    if bad {
        return Err(
            "Paths are relative to the project, for example `main.typ` or `chapters/intro.typ`. \
             Use list_files to see what exists."
                .to_owned(),
        );
    }
    if normal.split('/').any(|s| s.starts_with('.')) {
        return Err(format!(
            "{normal} is not available: `.git`, `.manang` and dotfiles are off limits."
        ));
    }
    Ok(normal)
}

fn size_label(bytes: u64) -> String {
    if bytes < 1_000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.1}k", bytes as f64 / 1_000.0)
    } else {
        format!("{:.1}M", bytes as f64 / 1_000_000.0)
    }
}

/// One listed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}

const SOURCE_EXTS: [&str; 8] = [
    "typ", "tex", "md", "markdown", "bib", "bibtex", "figure", "latex",
];
const SKIP_DIRS: [&str; 7] = [
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    "_build",
    "__pycache__",
];

fn is_source(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".figure.json")
        || lower
            .rsplit('.')
            .next()
            .is_some_and(|ext| SOURCE_EXTS.contains(&ext))
}

fn is_outlined(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    ["typ", "tex", "latex", "md", "markdown"]
        .iter()
        .any(|e| lower.ends_with(&format!(".{e}")))
}

/// The project's text sources below `dir`, sorted, with sizes. `total` counts what was found
/// before the cap of `limit` entries.
pub fn project_files(
    ctx: &Context<'_>,
    tc: &ToolContext,
    dir: &str,
) -> Result<Vec<FileInfo>, String> {
    let out = ctx
        .invoke(
            "project.tree",
            Payload::Json(json!({"path": dir, "recursive": true, "limit": 5000})),
        )
        .map_err(|e| {
            if dir.is_empty() {
                format!("The project could not be listed ({e}).")
            } else {
                format!("`{dir}` could not be listed ({e}). Check the folder name with list_files.")
            }
        })?;
    let Payload::Json(value) = out else {
        return Err("The project listing came back empty.".to_owned());
    };
    let mut files: Vec<FileInfo> = Vec::new();
    for entry in value
        .get("entries")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let (Some(path), Some("file")) = (
            entry.get("path").and_then(Value::as_str),
            entry.get("kind").and_then(Value::as_str),
        ) else {
            continue;
        };
        if !is_source(path) || path.split('/').any(|s| SKIP_DIRS.contains(&s)) {
            continue;
        }
        let size = tc
            .project_root
            .as_ref()
            .and_then(|r| std::fs::metadata(r.join(path)).ok())
            .map_or(0, |m| m.len());
        if size > MAX_FILE_BYTES {
            continue;
        }
        files.push(FileInfo {
            path: path.to_owned(),
            size,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// An indented tree, one file per line with its size.
pub fn render_tree(files: &[FileInfo]) -> String {
    let mut out = String::new();
    let mut open: Vec<&str> = Vec::new();
    for file in files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let (name, dirs) = match parts.split_last() {
            Some(split) => split,
            None => continue,
        };
        let common = open
            .iter()
            .zip(dirs.iter())
            .take_while(|(a, b)| a == b)
            .count();
        open.truncate(common);
        for dir in &dirs[common..] {
            out.push_str(&"  ".repeat(open.len()));
            out.push_str(dir);
            out.push_str("/\n");
            open.push(dir);
        }
        out.push_str(&"  ".repeat(dirs.len()));
        out.push_str(&format!("{name} {}\n", size_label(file.size)));
    }
    out
}

fn disk_text(ctx: &Context<'_>, path: &str) -> Result<String, String> {
    match ctx.invoke("project.read-file", Payload::Json(json!({ "path": path }))) {
        Ok(Payload::Json(Value::String(text))) => Ok(text),
        Ok(_) => Err(format!("{path} is not a text file.")),
        Err(e) => Err(format!(
            "{path} could not be read ({e}). Check the path with list_files."
        )),
    }
}

/// The text of `path` as the editor has it: the open buffer when there is one, else the disk.
pub fn base_text(ctx: &Context<'_>, tc: &ToolContext, path: &str) -> Result<String, String> {
    match tc.buffers.get(path) {
        Some(text) => Ok(text.clone()),
        None => disk_text(ctx, path),
    }
}

/// [`base_text`] with the session's pending proposals for `path` applied, oldest first.
pub fn overlay_text(
    ctx: &Context<'_>,
    tc: &ToolContext,
    path: &str,
) -> Result<(String, usize), String> {
    let base = base_text(ctx, tc, path)?;
    let mut current = base;
    let mut applied = 0;
    for pending in tc.pending.iter().filter(|p| p.path == path) {
        current = edit::apply_loosely(&current, &pending.pairs);
        applied += 1;
    }
    Ok((current, applied))
}

// ---- list_files ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    #[serde(default)]
    dir: Option<String>,
}

pub fn list_files(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> ToolResult {
    let args: ListArgs = parse(input)?;
    let dir = match args.dir.as_deref().filter(|d| !d.trim().is_empty()) {
        Some(d) => clean_path(d.trim_end_matches('/'))?,
        None => String::new(),
    };
    let files = project_files(ctx, tc, &dir)?;
    let place = if dir.is_empty() {
        "the project".to_owned()
    } else {
        format!("{dir}/")
    };
    if files.is_empty() {
        return text(format!(
            "No text sources (.typ .tex .md .bib, figures) in {place}."
        ));
    }
    let total = files.len();
    let shown = &files[..total.min(LIST_MAX)];
    let mut out = render_tree(shown);
    if total > shown.len() {
        out.push_str(&format!(
            "Showing {} of {total} files. Narrow with dir, for example dir=\"{}\".\n",
            shown.len(),
            shown
                .first()
                .and_then(|f| f.path.split('/').next())
                .filter(|_| shown.first().is_some_and(|f| f.path.contains('/')))
                .unwrap_or("chapters")
        ));
    } else {
        out.push_str(&format!("{total} files in {place}.\n"));
    }
    text(out)
}

// ---- read_file ----------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
    #[serde(default)]
    from_line: Option<usize>,
    #[serde(default)]
    to_line: Option<usize>,
}

pub fn read_file(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> ToolResult {
    let args: ReadArgs = parse(input)?;
    let path = clean_path(&args.path)?;
    let (full, pending) = overlay_text(ctx, tc, &path)?;
    let lines: Vec<&str> = full.lines().collect();
    let total = lines.len();
    if total == 0 {
        return text(format!("{path} is empty (0 lines)."));
    }
    let from = args.from_line.unwrap_or(1).max(1);
    if from > total {
        return Err(format!(
            "from_line {from} is past the end: {path} has {total} lines. \
             Use from_line {} or less.",
            total.saturating_sub(READ_LINES - 1).max(1)
        ));
    }
    let wanted_to = args.to_line.unwrap_or(from + READ_LINES - 1).min(total);
    if wanted_to < from {
        return Err(format!(
            "to_line {wanted_to} is before from_line {from}. Give a range with from_line <= to_line."
        ));
    }
    let mut body = String::new();
    let mut to = from - 1;
    for (i, line) in lines.iter().enumerate().take(wanted_to).skip(from - 1) {
        let row = format!("{}| {line}\n", i + 1);
        if body.chars().count() + row.chars().count() > RESULT_CAP && to >= from {
            break;
        }
        body.push_str(&row);
        to = i + 1;
    }
    let mut head = format!("{path} lines {from}-{to} of {total}");
    if pending > 0 {
        head.push_str(" (with your pending edits applied)");
    } else if tc.buffers.contains_key(&path) {
        head.push_str(" (editor text)");
    }
    if to < total {
        head.push_str(&format!(". Continue with from_line {}", to + 1));
    }
    text(format!("{head}\n{body}"))
}

// ---- search -------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    max: Option<usize>,
}

fn in_scope(file: &str, scope: Option<&str>) -> bool {
    match scope {
        None => true,
        Some(s) => file == s || file.starts_with(&format!("{}/", s.trim_end_matches('/'))),
    }
}

/// Matches of `query` in `body` as `(line, text)`, for files whose text differs from the disk.
fn scan(body: &str, matcher: &Matcher) -> Vec<(usize, String)> {
    body.lines()
        .enumerate()
        .filter(|(_, l)| matcher.is_match(l))
        .map(|(i, l)| (i + 1, l.trim().chars().take(200).collect()))
        .collect()
}

enum Matcher {
    Literal { needle: String, sensitive: bool },
    Pattern(regex::Regex),
}

impl Matcher {
    fn new(query: &str, regex: bool, sensitive: bool) -> Result<Self, String> {
        if regex {
            regex::RegexBuilder::new(query)
                .case_insensitive(!sensitive)
                .size_limit(1 << 20)
                .build()
                .map(Self::Pattern)
                .map_err(|e| format!("The regular expression is not valid: {e}. Fix it or search for a literal string."))
        } else {
            Ok(Self::Literal {
                needle: if sensitive {
                    query.to_owned()
                } else {
                    query.to_lowercase()
                },
                sensitive,
            })
        }
    }

    fn is_match(&self, line: &str) -> bool {
        match self {
            Self::Literal { needle, sensitive } => {
                if *sensitive {
                    line.contains(needle.as_str())
                } else {
                    line.to_lowercase().contains(needle.as_str())
                }
            }
            Self::Pattern(re) => re.is_match(line),
        }
    }
}

pub fn search(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> ToolResult {
    let args: SearchArgs = parse(input)?;
    if args.query.is_empty() {
        return Err("`query` must not be empty.".to_owned());
    }
    let Some(root) = &tc.project_root else {
        return Err("No project is open.".to_owned());
    };
    let scope = match args.path.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(p) => Some(clean_path(p.trim_end_matches('/'))?),
        None => None,
    };
    let max = args.max.unwrap_or(SEARCH_DEFAULT).clamp(1, 100);
    let matcher = Matcher::new(&args.query, args.regex, args.case_sensitive)?;

    // Files whose text the editor or a pending proposal changed are searched in memory.
    let mut overlay: BTreeSet<String> = tc.buffers.keys().cloned().collect();
    overlay.extend(tc.pending.iter().map(|p| p.path.clone()));
    let mut hits: Vec<(String, usize, String)> = Vec::new();
    for path in &overlay {
        if !in_scope(path, scope.as_deref()) || path.split('/').any(|s| s.starts_with('.')) {
            continue;
        }
        if let Ok((body, _)) = overlay_text(ctx, tc, path) {
            for (line, t) in scan(&body, &matcher) {
                hits.push((path.clone(), line, t));
            }
        }
    }
    let out = ctx
        .invoke(
            "search.text",
            Payload::Json(json!({
                "root": root.to_string_lossy(),
                "query": args.query,
                "regex": args.regex,
                "case_sensitive": args.case_sensitive,
                "limit": 500
            })),
        )
        .map_err(|e| format!("The search failed ({e})."))?;
    if let Payload::Json(value) = out {
        for file in value
            .get("files")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            let Some(path) = file.get("path").and_then(Value::as_str) else {
                continue;
            };
            if overlay.contains(path)
                || !in_scope(path, scope.as_deref())
                || path.split('/').any(|s| s.starts_with('.'))
            {
                continue;
            }
            for m in file
                .get("matches")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice)
            {
                let line = m.get("line").and_then(Value::as_u64).unwrap_or(0);
                let t = m.get("text").and_then(Value::as_str).unwrap_or("");
                hits.push((
                    path.to_owned(),
                    usize::try_from(line).unwrap_or(0),
                    t.trim().chars().take(200).collect(),
                ));
            }
        }
    }
    hits.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let total = hits.len();
    let files: BTreeSet<&str> = hits.iter().map(|h| h.0.as_str()).collect();
    if total == 0 {
        return text(format!(
            "No matches for \"{}\"{}. Try another spelling, a shorter query or regex.",
            args.query,
            scope.as_ref().map_or(String::new(), |s| format!(" in {s}"))
        ));
    }
    let mut out = String::new();
    for (path, line, t) in hits.iter().take(max) {
        out.push_str(&format!("{path}:{line}: {t}\n"));
    }
    if total > max {
        out.push_str(&format!(
            "Showing {max} of {total} matches in {} files. Narrow with path or a longer query, or raise max.\n",
            files.len()
        ));
    } else {
        out.push_str(&format!("{total} matches in {} files.\n", files.len()));
    }
    let (clipped, cut) = clip_chars(&out, RESULT_CAP);
    text(if cut {
        format!("{clipped}\n[cut: narrow the search]")
    } else {
        clipped
    })
}

// ---- outline ------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutlineArgs {
    #[serde(default)]
    path: Option<String>,
}

fn outline_of(path: &str, body: &str) -> String {
    outline(path, body)
        .iter()
        .map(|h| {
            format!(
                "{}{} (line {})\n",
                "  ".repeat(usize::from(h.level).saturating_sub(1)),
                h.title,
                h.line
            )
        })
        .collect()
}

pub fn outline_tool(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> ToolResult {
    let args: OutlineArgs = parse(input)?;
    if let Some(p) = args.path.as_deref().filter(|p| !p.trim().is_empty()) {
        let path = clean_path(p)?;
        let (body, _) = overlay_text(ctx, tc, &path)?;
        let list = outline_of(&path, &body);
        return if list.is_empty() {
            text(format!(
                "{path} has no headings ({} lines). Use read_file to look at it.",
                body.lines().count()
            ))
        } else {
            text(format!("{path}\n{list}"))
        };
    }
    let files = project_files(ctx, tc, "")?;
    let mut out = String::new();
    let mut shown = 0;
    for file in files.iter().filter(|f| is_outlined(&f.path)) {
        let Ok((body, _)) = overlay_text(ctx, tc, &file.path) else {
            continue;
        };
        let list = outline_of(&file.path, &body);
        if list.is_empty() {
            continue;
        }
        shown += 1;
        if shown > 40 || out.chars().count() > RESULT_CAP {
            out.push_str("[more files: pass path to see one]\n");
            break;
        }
        out.push_str(&format!("{}\n{list}", file.path));
    }
    if out.is_empty() {
        text("No headings found in the project's files.")
    } else {
        text(out)
    }
}

// ---- diagnostics --------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagArgs {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    with_pending: bool,
}

/// One problem as the model reads it, with the line and the one before it.
fn problem_line(
    path: &str,
    line: u32,
    column: Option<u32>,
    severity: &str,
    message: &str,
    body: Option<&str>,
) -> String {
    let at = match column {
        Some(c) => format!("{path}:{line}:{c}"),
        None => format!("{path}:{line}"),
    };
    let mut out = format!("{severity} {at} {message}\n");
    if let Some(body) = body {
        let lines: Vec<&str> = body.lines().collect();
        let n = usize::try_from(line).unwrap_or(0);
        for k in n.saturating_sub(1)..=n {
            if k >= 1
                && let Some(text) = lines.get(k - 1)
            {
                let shown: String = text.chars().take(160).collect();
                out.push_str(&format!("    {k}| {shown}\n"));
            }
        }
    }
    out
}

/// `(path, line, column, severity, message)`.
type ProblemRow = (String, u32, Option<u32>, String, String);

fn typst_main(files: &[FileInfo], active: Option<&str>) -> Option<String> {
    let typ = |p: &str| p.to_ascii_lowercase().ends_with(".typ");
    if let Some(a) = active.filter(|a| typ(a)) {
        return Some(a.to_owned());
    }
    files
        .iter()
        .find(|f| f.path == "main.typ")
        .or_else(|| files.iter().find(|f| typ(&f.path)))
        .map(|f| f.path.clone())
}

/// Problems of a Typst project, compiled on a scratch session (nothing the preview shows changes).
fn typst_problems(
    ctx: &Context<'_>,
    tc: &ToolContext,
    main: &str,
    sources: &BTreeMap<String, String>,
) -> Result<Vec<ProblemRow>, String> {
    let root = tc
        .project_root
        .as_ref()
        .ok_or_else(|| "No project is open.".to_owned())?;
    let out = ctx
        .invoke(
            "typst.check",
            Payload::Json(json!({
                "main": main, "root": root.to_string_lossy(), "sources": sources
            })),
        )
        .map_err(|e| format!("The Typst check failed ({e})."))?;
    let Payload::Json(value) = out else {
        return Err("The Typst check returned nothing.".to_owned());
    };
    let mut list = Vec::new();
    for d in value
        .get("diagnostics")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let file = d
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or(main)
            .trim_start_matches('/')
            .to_owned();
        list.push((
            file,
            d.get("line")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(1),
            d.get("column")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            d.get("severity")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_owned(),
            d.get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        ));
    }
    Ok(list)
}

pub fn diagnostics(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> ToolResult {
    let args: DiagArgs = parse(input)?;
    let wanted = args
        .path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(clean_path)
        .transpose()?;
    let editor_rows = || -> Vec<ProblemRow> {
        tc.diagnostics
            .iter()
            .map(|d| {
                (
                    d.path.clone(),
                    d.line,
                    d.column,
                    d.severity.clone(),
                    d.message.clone(),
                )
            })
            .collect()
    };
    let (mut rows, verdict) = if args.with_pending {
        let files = project_files(ctx, tc, "")?;
        match typst_main(&files, tc.active.as_deref()) {
            Some(main) => {
                let before = typst_problems(ctx, tc, &main, &tc.buffers)?;
                let mut sources = tc.buffers.clone();
                for pending in &tc.pending {
                    if let Ok((body, _)) = overlay_text(ctx, tc, &pending.path) {
                        sources.insert(pending.path.clone(), body);
                    }
                }
                let after = typst_problems(ctx, tc, &main, &sources)?;
                let (b, a) = (before.len(), after.len());
                let verdict = format!(
                    "With {} pending edit(s) applied: {a} problem(s), before: {b}. {}\n",
                    tc.pending.len(),
                    match a.cmp(&b) {
                        std::cmp::Ordering::Less => "Fewer problems.",
                        std::cmp::Ordering::Equal => "No change in the count.",
                        std::cmp::Ordering::Greater => "More problems: adjust the edit.",
                    }
                );
                (after, verdict)
            }
            None => (
                editor_rows(),
                "A check with pending edits works for Typst projects only; these are the editor's problems as they were.\n"
                    .to_owned(),
            ),
        }
    } else {
        (editor_rows(), String::new())
    };
    if let Some(w) = &wanted {
        rows.retain(|r| &r.0 == w);
    }
    if rows.is_empty() {
        return text(format!("{verdict}No problems."));
    }
    let total = rows.len();
    let mut out = verdict;
    for (path, line, column, severity, message) in rows.iter().take(PROBLEMS_MAX) {
        let body = overlay_text(ctx, tc, path).ok().map(|(b, _)| b);
        out.push_str(&problem_line(
            path,
            *line,
            *column,
            severity,
            message,
            body.as_deref(),
        ));
    }
    if total > PROBLEMS_MAX {
        out.push_str(&format!(
            "Showing {PROBLEMS_MAX} of {total} problems. Pass path to see one file's.\n"
        ));
    }
    let (clipped, cut) = clip_chars(&out, RESULT_CAP);
    text(if cut {
        format!("{clipped}\n[cut: pass path]")
    } else {
        clipped
    })
}

// ---- bib_search ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BibArgs {
    query: String,
    #[serde(default)]
    max: Option<usize>,
}

fn authors_line(entry: &Value) -> String {
    let names: Vec<&str> = entry
        .get("authors")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|a| {
            a.get("family")
                .and_then(Value::as_str)
                .or_else(|| a.get("name").and_then(Value::as_str))
        })
        .collect();
    match names.as_slice() {
        [] => "(no authors)".to_owned(),
        [one] => (*one).to_owned(),
        [a, b] => format!("{a} and {b}"),
        [first, ..] => format!("{first} et al."),
    }
}

pub fn bib_search(ctx: &Context<'_>, input: &Value) -> ToolResult {
    let args: BibArgs = parse(input)?;
    let max = args.max.unwrap_or(8).clamp(1, 25);
    let out = ctx
        .invoke(
            "citations.search",
            Payload::Json(json!({"query": args.query, "limit": max})),
        )
        .map_err(|e| format!("The bibliography could not be searched ({e})."))?;
    let Payload::Json(value) = out else {
        return Err("The bibliography search returned nothing.".to_owned());
    };
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    if results.is_empty() {
        return text(format!(
            "No entry matches \"{}\" in the project's bibliography. Do not cite anything for it; tell the author no source was found.",
            args.query
        ));
    }
    let mut out = String::new();
    for r in results.iter().take(max) {
        let key = r.get("key").and_then(Value::as_str).unwrap_or("?");
        let entry = r.get("entry").unwrap_or(r);
        let year = entry
            .get("year")
            .and_then(Value::as_u64)
            .map_or_else(|| "n.d.".to_owned(), |y| y.to_string());
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("(no title)");
        let title: String = title.chars().take(140).collect();
        out.push_str(&format!(
            "{key} | {} ({year}) | {title}\n",
            authors_line(entry)
        ));
    }
    text(out)
}

// ---- edit_text ----------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditIn {
    old: String,
    new: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    path: String,
    edits: Vec<EditIn>,
    #[serde(default)]
    note: Option<String>,
}

/// Whether `key` is in the bibliography, or (for a Typst `@key`) a label the project defines.
fn key_known(ctx: &Context<'_>, tc: &ToolContext, key: &str, base: &str, label_ok: bool) -> bool {
    if ctx
        .invoke("citations.get", Payload::Json(json!({ "key": key })))
        .is_ok()
    {
        return true;
    }
    if !label_ok {
        return false;
    }
    let label = format!("<{key}>");
    if base.contains(&label) || tc.buffers.values().any(|b| b.contains(&label)) {
        return true;
    }
    let Some(root) = &tc.project_root else {
        return false;
    };
    matches!(
        ctx.invoke(
            "search.text",
            Payload::Json(json!({"root": root.to_string_lossy(), "query": label, "limit": 1})),
        ),
        Ok(Payload::Json(v)) if v.get("total_matches").and_then(Value::as_u64).unwrap_or(0) > 0
    )
}

/// Validates `edit_text` and returns the proposal for the session to queue:
/// `{"proposal": {...}, "removed": n, "added": n}`.
pub fn edit_text(ctx: &Context<'_>, tc: &ToolContext, input: &Value) -> ToolResult {
    let args: EditArgs = parse(input)?;
    let path = clean_path(&args.path)?;
    if args.edits.is_empty() || args.edits.len() > 20 {
        return Err("Give between 1 and 20 edits per call.".to_owned());
    }
    let (base, _) = overlay_text(ctx, tc, &path).map_err(|e| {
        format!(
            "{e} Creating, deleting and renaming files is not possible: edit existing files only."
        )
    })?;
    let pairs: Vec<Pair> = args
        .edits
        .iter()
        .map(|e| Pair {
            old: e.old.clone(),
            new: e.new.clone(),
        })
        .collect();
    let located = edit::locate(&path, &base, &pairs)?;
    let many = located.len() > 1;
    for (i, e) in located.iter().enumerate() {
        let which = if many {
            format!("edit {} of {}: ", i + 1, located.len())
        } else {
            String::new()
        };
        if let Some(problem) = pair_problem(&e.before, &e.after, &tc.user_text) {
            return Err(format!("{which}{problem}."));
        }
        for (key, cite) in introduced_keys(&e.before, &e.after) {
            let is_at = e.after.contains(&format!("@{key}"));
            if cite && !key_known(ctx, tc, &key, &base, is_at) {
                return Err(format!(
                    "{which}the edit cites `{key}`, which is not in the project's bibliography. \
                     Find real keys with bib_search, or leave a visible marker for the author instead of a citation."
                ));
            }
        }
    }
    // Braces and environments are judged on the whole file: a missing brace may be repaired by one
    // edit and the pairs of one proposal may balance each other.
    if let Some(problem) = balance_problem(&base, &edit::apply(&base, &located)) {
        return Err(format!("{problem}."));
    }
    let (removed, added) = edit::char_delta(&located);
    if located
        .iter()
        .map(|e| e.after.chars().count())
        .sum::<usize>()
        > REPLACEMENT_CAP
    {
        return Err("The replacement text is too long. Split it into smaller edits.".to_owned());
    }
    let edits: Vec<Value> = located
        .iter()
        .map(|e| json!({"from": e.from, "to": e.to, "before": e.before, "after": e.after}))
        .collect();
    Ok(json!({
        "proposal": {
            "kind": "text_edit",
            "path": path,
            "note": args.note.unwrap_or_default(),
            "edits": edits,
        },
        "removed": removed,
        "added": added,
    }))
}

// ---- ask_user -----------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskArgs {
    question: String,
    #[serde(default)]
    choices: Vec<String>,
}

pub fn ask_user(input: &Value) -> ToolResult {
    let args: AskArgs = parse(input)?;
    if args.question.trim().is_empty() {
        return Err("`question` must not be empty.".to_owned());
    }
    let choices: Vec<String> = args
        .choices
        .into_iter()
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty())
        .take(4)
        .collect();
    Ok(json!({"ask": {"question": args.question.trim(), "choices": choices}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str, size: u64) -> FileInfo {
        FileInfo {
            path: path.to_owned(),
            size,
        }
    }

    #[test]
    fn the_tree_is_indented_with_sizes() {
        let files = vec![
            f("chapters/a.typ", 1_200),
            f("chapters/deep/b.typ", 300),
            f("main.typ", 4_200),
            f("refs.bib", 2_500_000),
        ];
        insta::assert_snapshot!(render_tree(&files));
    }

    #[test]
    fn paths_stay_inside_the_project() {
        assert_eq!(clean_path("./a/b.typ").as_deref(), Ok("a/b.typ"));
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "C:/x",
            ".git/config",
            "a/.manang/x",
            ".env",
        ] {
            assert!(clean_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_declared_tools_are_small() {
        let decl = declarations();
        let tools = decl["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 8);
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(
            names,
            [
                "list_files",
                "read_file",
                "search",
                "outline",
                "diagnostics",
                "bib_search",
                "edit_text",
                "ask_user"
            ]
        );
        // What a provider gets: name, description and input schema (no title, no output schema).
        let sent: usize = tools
            .iter()
            .map(|t| {
                json!({"name": t["name"], "description": t["description"], "input_schema": t["input_schema"]})
                    .to_string()
                    .len()
            })
            .sum();
        let tokens = (sent as u64 * 2).div_ceil(7);
        assert!(tokens <= 1_800, "{tokens} tokens");
    }

    #[test]
    fn ask_user_keeps_at_most_four_choices() {
        let v = ask_user(
            &json!({"question": " Delete it? ", "choices": ["Yes", "No", " ", "Maybe", "a", "b"]}),
        )
        .expect("ok");
        assert_eq!(v["ask"]["question"], "Delete it?");
        assert_eq!(v["ask"]["choices"], json!(["Yes", "No", "Maybe", "a"]));
        assert!(ask_user(&json!({"question": "  "})).is_err());
    }

    #[test]
    fn a_problem_shows_its_line_and_the_one_before() {
        let body = "a\n#figur(\"x\")\nc";
        let out = problem_line(
            "main.typ",
            2,
            Some(2),
            "error",
            "unknown variable: figur",
            Some(body),
        );
        insta::assert_snapshot!(out);
    }
}
