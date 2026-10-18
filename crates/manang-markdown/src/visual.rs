//! The Markdown format adapter of the Visual editor (ADR 0015).
//!
//! `pulldown-cmark` has no incremental reparse, so the adapter keeps the
//! document as a list of top-level *blocks*, each with its spans relative to
//! its own start. An edit reparses a small window of blocks (the edited ones,
//! their neighbours and as many more as it takes for the block boundaries to
//! line up again), splices the result in and shifts the later block starts.
//! Everything outside the window is untouched, so the cost of a keystroke is
//! independent of the size of the file.

use std::ops::Range;

use manang_visual::kind;
use manang_visual::text::{self, Enclosing};
use manang_visual::{
    AdapterError, AdapterInfo, AdapterSession, CommandState, EditPlan, FormatAdapter, OutlineItem,
    RawSpan, TextEdit,
};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag};

/// Commands the adapter implements.
pub const COMMANDS: &[&str] = &[
    "bold", "italic", "code", "strike", "heading", "list", "quote", "link", "image", "math", "rule",
];

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
}

/// The Markdown adapter. Register it with the visual host.
pub struct MarkdownAdapter {
    info: AdapterInfo,
}

impl MarkdownAdapter {
    /// The adapter and its declaration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            info: AdapterInfo {
                id: "markdown".to_owned(),
                title: "Markdown".to_owned(),
                extensions: vec!["md".to_owned(), "markdown".to_owned(), "mdx".to_owned()],
                kinds: [
                    kind::HEADING,
                    kind::STRONG,
                    kind::EMPH,
                    kind::STRIKE,
                    kind::CODE,
                    kind::CODE_BLOCK,
                    kind::LIST_ITEM,
                    kind::LINK,
                    kind::IMAGE,
                    kind::QUOTE,
                    kind::MATH,
                    kind::RULE,
                    kind::FRONTMATTER,
                    kind::TABLE,
                    kind::MARKER,
                    kind::OPAQUE,
                ]
                .iter()
                .filter_map(|k| kind::name(*k).map(str::to_owned))
                .collect(),
                commands: COMMANDS.iter().map(|c| (*c).to_owned()).collect(),
            },
        }
    }
}

impl Default for MarkdownAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatAdapter for MarkdownAdapter {
    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn open(&self, text: &str) -> Result<Box<dyn AdapterSession>, AdapterError> {
        Ok(Box::new(MarkdownSession::new(text)))
    }
}

/// One top-level block: where it starts and its spans relative to that start.
/// A block extends to the start of the next one (or the end of the text), so
/// the blank lines after it belong to it.
#[derive(Debug, Clone)]
struct Block {
    start: usize,
    spans: Vec<RawSpan>,
    /// Could continue into what follows (list, quote, fence, HTML): a window
    /// ending on one must grow until the boundaries settle.
    open: bool,
}

/// A parsed Markdown document.
pub struct MarkdownSession {
    text: String,
    blocks: Vec<Block>,
}

impl MarkdownSession {
    /// Parse `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut s = Self {
            text: text.to_owned(),
            blocks: Vec::new(),
        };
        s.blocks = parse_window(&s.text, 0, s.text.len());
        s
    }

    /// Index of the block containing `pos` (0 before the first block).
    fn block_at(&self, pos: usize) -> usize {
        self.blocks
            .partition_point(|b| b.start <= pos)
            .saturating_sub(1)
    }

    /// Absolute spans of the blocks touching `range`.
    fn spans_in(&self, range: &Range<usize>, out: &mut Vec<RawSpan>) {
        if self.blocks.is_empty() {
            return;
        }
        let first = self.block_at(range.start);
        let last = self.block_at(range.end.max(range.start));
        for b in &self.blocks[first..=last] {
            for s in &b.spans {
                let mut s = s.clone();
                s.range = b.start + s.range.start..b.start + s.range.end;
                out.push(s);
            }
        }
    }

    fn near(&self, sel: &Range<usize>) -> Vec<RawSpan> {
        let mut out = Vec::new();
        self.spans_in(sel, &mut out);
        out
    }

    /// The innermost span of `kind_id` that holds the selection.
    fn enclosing(&self, kind_id: u16, sel: &Range<usize>) -> Option<(RawSpan, Enclosing)> {
        let all = self.near(sel);
        let best = all
            .iter()
            .filter(|s| s.kind == kind_id)
            .filter_map(|s| {
                let open = all
                    .iter()
                    .find(|m| m.kind == kind::MARKER && m.range.start == s.range.start)
                    .map_or(0, |m| m.range.len().min(s.range.len()));
                let close = all
                    .iter()
                    .find(|m| m.kind == kind::MARKER && m.range.end == s.range.end)
                    .map_or(0, |m| m.range.len().min(s.range.len()));
                let e = Enclosing {
                    outer: s.range.clone(),
                    open_len: open,
                    close_len: close,
                };
                (open > 0 && close > 0 && e.holds(sel)).then(|| (s.clone(), e))
            })
            .min_by_key(|(s, _)| s.range.len())?;
        Some(best)
    }

    fn line_state(&self, sel: &Range<usize>) -> LineInfo {
        let sel = text::clamp(&self.text, sel.clone());
        let start = text::line_start(&self.text, sel.start);
        let end = text::line_end(&self.text, sel.start);
        line_info(&self.text[start..end])
    }
}

impl AdapterSession for MarkdownSession {
    fn edit(&mut self, range: Range<usize>, insert: &str) -> Result<Range<usize>, AdapterError> {
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err(AdapterError::Internal(format!(
                "edit {}..{} is outside the {} byte text",
                range.start,
                range.end,
                self.text.len()
            )));
        }
        let old_front = frontmatter_end(&self.text);
        self.text.replace_range(range.clone(), insert);
        let new_len = self.text.len();
        // Frontmatter is the one construct a distant edit can create or
        // destroy: re-read everything when this edit may have changed it.
        let new_front = frontmatter_end(&self.text);
        let frontmatter_risk = old_front.is_some() != new_front.is_some()
            || old_front.is_some_and(|end| range.start <= end);
        if self.blocks.is_empty() || frontmatter_risk {
            self.blocks = parse_window(&self.text, 0, new_len);
            return Ok(0..new_len);
        }
        let delta = insert.len() as isize - (range.end - range.start) as isize;
        let last = self.blocks.len() - 1;
        let mut first = self.block_at(range.start).saturating_sub(1);
        // A list, quote, fence or HTML block before the window can swallow
        // what the edit turned into its continuation: start earlier.
        // Start where nothing before can change how the window reads: after
        // a blank line (a link definition or a lazy continuation does not
        // show in a block's own range).
        while first > 0
            && (self.blocks[first - 1].open
                || !after_blank_line(&self.text, self.blocks[first].start))
        {
            first -= 1;
        }
        let mut end = (self.block_at(range.end) + 1).min(last);
        let window_start = if first == 0 {
            0
        } else {
            self.blocks[first].start
        };
        let (parsed, window_end) = loop {
            let window_end = if end == last {
                new_len
            } else {
                (self.blocks[end + 1].start as isize + delta) as usize
            };
            let parsed = parse_window(&self.text, window_start, window_end);
            // The next line can continue the last block (a list, a fence) or
            // turn a paragraph into a setext heading unless a blank line
            // separates them: grow until the window ends on one.
            if end < last
                && (parsed.last().is_some_and(|b| b.open)
                    || !ends_with_blank_line(&self.text[window_start..window_end]))
            {
                end += 1;
                continue;
            }
            break (parsed, window_end);
        };
        let inserted = parsed.len();
        self.blocks.splice(first..=end, parsed);
        for b in self.blocks.iter_mut().skip(first + inserted) {
            b.start = (b.start as isize + delta) as usize;
        }
        Ok(window_start..window_end)
    }

    fn spans(&self, viewport: Range<usize>, out: &mut Vec<RawSpan>) -> Result<(), AdapterError> {
        self.spans_in(&viewport, out);
        Ok(())
    }

    fn outline(&self) -> Vec<OutlineItem> {
        let mut out = Vec::new();
        for b in &self.blocks {
            for s in b.spans.iter().filter(|s| s.kind == kind::HEADING) {
                let range = b.start + s.range.start..b.start + s.range.end;
                out.push(OutlineItem {
                    level: s.aux,
                    title: heading_title(&self.text[range.clone()]),
                    range,
                });
            }
        }
        out
    }

    fn command_state(&self, sel: Range<usize>) -> Vec<CommandState> {
        let line = self.line_state(&sel);
        let active = |k: u16| self.enclosing(k, &sel).is_some();
        let mut v = vec![
            CommandState::new("bold", true, active(kind::STRONG)),
            CommandState::new("italic", true, active(kind::EMPH)),
            CommandState::new("code", true, active(kind::CODE)),
            CommandState::new("strike", true, active(kind::STRIKE)),
            CommandState::new("heading", true, line.heading > 0),
            CommandState::new("list", true, line.list.is_some()),
            CommandState::new("quote", true, line.quote),
            CommandState::new("link", true, active(kind::LINK)),
            CommandState::new("image", true, false),
            CommandState::new("math", true, active(kind::MATH)),
            CommandState::new("rule", true, false),
        ];
        // Inside code or math the inline wrappers would only add noise.
        let inside_leaf = self.near(&sel).iter().any(|s| {
            matches!(s.kind, kind::CODE_BLOCK | kind::FRONTMATTER) && contains(&s.range, &sel)
        });
        if inside_leaf {
            for c in &mut v {
                if matches!(c.command.as_str(), "bold" | "italic" | "strike" | "link") {
                    c.enabled = false;
                }
            }
        }
        v
    }

    fn run_command(
        &self,
        command: &str,
        sel: Range<usize>,
        args: &str,
    ) -> Result<EditPlan, AdapterError> {
        let t = &self.text;
        let sel = text::clamp(t, sel);
        match command {
            "bold" => Ok(self.wrap(&sel, kind::STRONG, "**", "**")),
            "italic" => Ok(self.wrap(&sel, kind::EMPH, "*", "*")),
            "strike" => Ok(self.wrap(&sel, kind::STRIKE, "~~", "~~")),
            "code" => {
                if t[sel.clone()].contains('\n') {
                    let body = &t[sel.clone()];
                    let rep = format!("```\n{}\n```", body.trim_matches('\n'));
                    let len = rep.len();
                    return Ok(text::replace_selection(t, sel, rep, 4..len - 4));
                }
                Ok(self.wrap(&sel, kind::CODE, "`", "`"))
            }
            "math" => {
                if args == "block" && self.enclosing(kind::MATH, &sel).is_none() {
                    let body = t[sel.clone()].trim_matches('\n');
                    let rep = format!("$$\n{body}\n$$");
                    let len = rep.len();
                    return Ok(text::replace_selection(t, sel, rep, 3..len - 3));
                }
                Ok(self.wrap(&sel, kind::MATH, "$", "$"))
            }
            "heading" => {
                let level: usize = args.trim().parse().map_err(|_| {
                    AdapterError::InvalidArgument("`heading` takes a level 0-6".to_owned())
                })?;
                if level > 6 {
                    return Err(AdapterError::InvalidArgument(
                        "`heading` takes a level 0-6".to_owned(),
                    ));
                }
                Ok(self.heading(&sel, level))
            }
            "list" => match args.trim() {
                "" | "bullet" => Ok(self.list(&sel, false)),
                "ordered" => Ok(self.list(&sel, true)),
                other => Err(AdapterError::InvalidArgument(format!(
                    "`list` takes `bullet` or `ordered`, not `{other}`"
                ))),
            },
            "quote" => Ok(self.quote(&sel)),
            "link" => Ok(self.link(&sel, args.trim())),
            "image" => {
                let path = args.trim();
                if path.is_empty() {
                    return Err(AdapterError::InvalidArgument(
                        "`image` needs a path".to_owned(),
                    ));
                }
                let alt = if sel.is_empty() {
                    stem(path).to_owned()
                } else {
                    t[sel.clone()].to_owned()
                };
                let rep = format!("![{alt}]({path})");
                let len = rep.len();
                Ok(text::replace_selection(t, sel, rep, len..len))
            }
            "rule" => {
                let at = text::line_end(t, sel.end);
                let rep = if at == t.len() {
                    "\n\n---\n"
                } else {
                    "\n\n---"
                };
                Ok(EditPlan {
                    edits: vec![TextEdit::insert(at, rep)],
                    selection: Some(at + rep.len()..at + rep.len()),
                })
            }
            other => Err(AdapterError::Unsupported(format!(
                "the Markdown adapter has no `{other}` command"
            ))),
        }
    }
}

/// End (after its line break) of a frontmatter block that opens the text:
/// a `---` line, anything, then `---` or `...` (YAML), or `+++` ... `+++` (TOML).
fn frontmatter_end(text: &str) -> Option<usize> {
    let (open, closers): (&str, &[&str]) = if text.starts_with("---") {
        ("---", &["---", "..."])
    } else if text.starts_with("+++") {
        ("+++", &["+++"])
    } else {
        return None;
    };
    let first_end = text::line_end(text, 0);
    if text[..first_end].trim_end() != open {
        return None;
    }
    let mut at = first_end + 1;
    while at < text.len() {
        let end = text::line_end(text, at);
        if closers.contains(&text[at..end].trim_end()) {
            return Some((end + 1).min(text.len()));
        }
        at = end + 1;
    }
    None
}

/// Whether the text ends with an empty (or blank) line.
fn ends_with_blank_line(text: &str) -> bool {
    let body = text.strip_suffix('\n').unwrap_or(text);
    let line_start = text::line_start(body, body.len());
    text.ends_with('\n') && body[line_start..].trim().is_empty() && !body.is_empty()
        || text.trim().is_empty()
}

/// Whether `pos` starts a line (after indentation only) whose previous line is blank.
fn after_blank_line(text: &str, pos: usize) -> bool {
    let ls = text::line_start(text, pos);
    if !text[ls..pos.max(ls)].trim().is_empty() {
        return false;
    }
    if ls == 0 {
        return true;
    }
    let prev_start = text::line_start(text, ls - 1);
    text[prev_start..ls - 1].trim().is_empty()
}

fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// What the first line of a selection looks like.
struct LineInfo {
    heading: usize,
    list: Option<bool>,
    quote: bool,
}

fn line_info(line: &str) -> LineInfo {
    LineInfo {
        heading: atx_prefix(line).map_or(0, |(l, _)| l),
        list: list_prefix(line).map(|(ordered, _, _)| ordered),
        quote: line.trim_start().starts_with('>'),
    }
}

/// `(level, prefix_len)` of an ATX heading line.
fn atx_prefix(line: &str) -> Option<(usize, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }
    let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
    Some((hashes, indent + hashes + spaces))
}

/// `(ordered, indent_len, marker_len_with_spaces)` of a list item line.
fn list_prefix(line: &str) -> Option<(bool, usize, usize)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let b = rest.as_bytes();
    let (ordered, mlen) = if matches!(b.first(), Some(b'-' | b'*' | b'+')) {
        (false, 1)
    } else {
        let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 || digits > 9 || !matches!(b.get(digits), Some(b'.' | b')')) {
            return None;
        }
        (true, digits + 1)
    };
    let after = &rest[mlen..];
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }
    let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
    Some((
        ordered,
        indent,
        mlen + spaces.max(usize::from(!after.is_empty())),
    ))
}

impl MarkdownSession {
    fn wrap(&self, sel: &Range<usize>, k: u16, open: &str, close: &str) -> EditPlan {
        let enclosing = self.enclosing(k, sel).map(|(_, e)| e);
        text::toggle_wrap(&self.text, sel.clone(), open, close, enclosing.as_ref())
    }

    fn heading(&self, sel: &Range<usize>, level: usize) -> EditPlan {
        let lines = text::lines_of(&self.text, sel);
        let current = atx_prefix(&self.text[lines[0].clone()]).map_or(0, |(l, _)| l);
        let target = if current == level { 0 } else { level };
        let mut edits = Vec::new();
        for line in &lines {
            let s = &self.text[line.clone()];
            if s.trim().is_empty() && target > 0 {
                continue;
            }
            let replaced = atx_prefix(s).map_or(0, |(_, p)| p);
            let prefix = if target == 0 {
                String::new()
            } else {
                format!("{} ", "#".repeat(target))
            };
            if s[..replaced] != prefix {
                edits.push(TextEdit::replace(line.start..line.start + replaced, prefix));
            }
        }
        let selection =
            Some(text::map_through(&edits, sel.start)..text::map_through(&edits, sel.end));
        EditPlan { edits, selection }
    }

    fn list(&self, sel: &Range<usize>, ordered: bool) -> EditPlan {
        let lines: Vec<Range<usize>> = text::lines_of(&self.text, sel)
            .into_iter()
            .filter(|l| !self.text[l.clone()].trim().is_empty())
            .collect();
        let all_match = !lines.is_empty()
            && lines
                .iter()
                .all(|l| list_prefix(&self.text[l.clone()]).is_some_and(|(o, _, _)| o == ordered));
        let mut edits = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let s = &self.text[line.clone()];
            let (indent, old_len) = list_prefix(s).map_or_else(
                || (s.len() - s.trim_start().len(), 0),
                |(_, indent, len)| (indent, len),
            );
            let marker = if all_match {
                String::new()
            } else if ordered {
                format!("{}. ", i + 1)
            } else {
                "- ".to_owned()
            };
            let from = line.start + indent;
            edits.push(TextEdit::replace(from..from + old_len, marker));
        }
        let selection =
            Some(text::map_through(&edits, sel.start)..text::map_through(&edits, sel.end));
        EditPlan { edits, selection }
    }

    fn quote(&self, sel: &Range<usize>) -> EditPlan {
        let lines = text::lines_of(&self.text, sel);
        let all_quoted = lines
            .iter()
            .all(|l| self.text[l.clone()].trim_start().starts_with('>'));
        let mut edits = Vec::new();
        for line in &lines {
            let s = &self.text[line.clone()];
            if all_quoted {
                let indent = s.len() - s.trim_start().len();
                let after = &s[indent + 1..];
                let extra = usize::from(after.starts_with(' '));
                edits.push(TextEdit::delete(
                    line.start + indent..line.start + indent + 1 + extra,
                ));
            } else {
                edits.push(TextEdit::insert(line.start, "> "));
            }
        }
        let selection =
            Some(text::map_through(&edits, sel.start)..text::map_through(&edits, sel.end));
        EditPlan { edits, selection }
    }

    fn link(&self, sel: &Range<usize>, url: &str) -> EditPlan {
        if let Some((_, e)) = self.enclosing(kind::LINK, sel) {
            return text::toggle_wrap(&self.text, sel.clone(), "", "", Some(&e));
        }
        let (label, label_sel) = if sel.is_empty() {
            let word = text::expand_word(&self.text, sel.clone());
            if word.is_empty() {
                ("link".to_owned(), true)
            } else {
                (self.text[word].to_owned(), false)
            }
        } else {
            (self.text[sel.clone()].to_owned(), false)
        };
        let target = if sel.is_empty() {
            let w = text::expand_word(&self.text, sel.clone());
            if w.is_empty() { sel.clone() } else { w }
        } else {
            sel.clone()
        };
        let rep = format!("[{label}]({url})");
        let (a, b) = if url.is_empty() {
            let at = 1 + label.len() + 2;
            (at, at)
        } else if label_sel {
            (1, 1 + label.len())
        } else {
            (rep.len(), rep.len())
        };
        let base = target.start;
        EditPlan {
            edits: vec![TextEdit::replace(target, rep)],
            selection: Some(base + a..base + b),
        }
    }
}

/// The text of a heading without its markers.
fn heading_title(src: &str) -> String {
    let first = src.lines().next().unwrap_or("");
    let body = match atx_prefix(first) {
        Some((_, p)) => &first[p..],
        None => first,
    };
    body.trim_end_matches([' ', '\t'])
        .trim_end_matches('#')
        .trim_end()
        .to_owned()
}

// ---------------------------------------------------------------- parsing

struct Frame {
    tag: FrameTag,
    range: Range<usize>,
    /// Hull of everything inside (links need it for their markers).
    content: Option<Range<usize>>,
    task: Option<(bool, Range<usize>)>,
}

enum FrameTag {
    Heading(u8),
    Strong,
    Emph,
    Strike,
    Link(String),
    Image(String),
    Quote,
    CodeBlock(Option<String>),
    Item(bool),
    List(bool),
    Table,
    Html,
    Other,
}

impl FrameTag {
    /// Whether spans inside this frame are left out (rendered later or opaque).
    fn suppresses(&self) -> bool {
        matches!(
            self,
            Self::Image(_) | Self::Table | Self::Html | Self::CodeBlock(_)
        )
    }
}

struct Open {
    start: usize,
    open: bool,
    spans: Vec<RawSpan>,
}

fn touch(stack: &mut [Frame], range: &Range<usize>) {
    if let Some(top) = stack.last_mut() {
        top.content = Some(match top.content.take() {
            Some(c) => c.start.min(range.start)..c.end.max(range.end),
            None => range.clone(),
        });
    }
}

/// Parse `text[from..to]` as an independent document and return its top-level
/// blocks with absolute starts. Frontmatter is only recognised at offset 0
/// (by `frontmatter_end`; the parser's own metadata option accepts it anywhere).
fn parse_window(text: &str, from: usize, to: usize) -> Vec<Block> {
    let window = &text[from..to];
    let mut blocks: Vec<Block> = Vec::new();
    if from == 0
        && let Some(front) = frontmatter_end(text).filter(|end| *end <= to)
    {
        let kept = text[..front].trim_end().len();
        blocks.push(Block {
            start: 0,
            spans: vec![RawSpan::new(0..kept, kind::FRONTMATTER)],
            open: false,
        });
        blocks.extend(parse_window(text, front, to));
        return blocks;
    }
    let opts = options();
    let mut stack: Vec<Frame> = Vec::new();
    let mut cur: Option<Open> = None;

    for (event, range) in Parser::new_ext(window, opts).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                let tag = frame_tag(&tag, window, &range, &stack);
                if stack.is_empty() {
                    let open = matches!(
                        tag,
                        FrameTag::List(_)
                            | FrameTag::Quote
                            | FrameTag::CodeBlock(_)
                            | FrameTag::Html
                    );
                    cur = Some(Open {
                        start: range.start,
                        open,
                        spans: Vec::new(),
                    });
                }
                touch(&mut stack, &range);
                stack.push(Frame {
                    tag,
                    range,
                    content: None,
                    task: None,
                });
            }
            Event::End(_) => {
                let Some(frame) = stack.pop() else { continue };
                if !stack.iter().any(|f| f.tag.suppresses())
                    && let Some(open) = cur.as_mut()
                {
                    let depth = stack
                        .iter()
                        .filter(|f| matches!(f.tag, FrameTag::Quote))
                        .count()
                        + 1;
                    emit(&frame, window, depth, &mut open.spans);
                }
                if stack.is_empty()
                    && let Some(open) = cur.take()
                {
                    blocks.push(finish(open, from));
                }
            }
            leaf => {
                let standalone = stack.is_empty();
                if standalone {
                    cur = Some(Open {
                        start: range.start,
                        open: matches!(leaf, Event::Html(_)),
                        spans: Vec::new(),
                    });
                }
                touch(&mut stack, &range);
                let suppressed = stack.iter().any(|f| f.tag.suppresses());
                if let Some(open) = cur.as_mut() {
                    match leaf {
                        Event::TaskListMarker(done) => {
                            if let Some(item) = stack
                                .iter_mut()
                                .rev()
                                .find(|f| matches!(f.tag, FrameTag::Item(_)))
                            {
                                item.task = Some((done, range.clone()));
                            }
                        }
                        Event::Code(_) if !suppressed => {
                            open.spans.push(RawSpan::new(range.clone(), kind::CODE));
                            let n = window[range.clone()]
                                .bytes()
                                .take_while(|b| *b == b'`')
                                .count();
                            delimiters(&mut open.spans, &range, n, n);
                        }
                        Event::InlineMath(_) | Event::DisplayMath(_) if !suppressed => {
                            let display = matches!(leaf, Event::DisplayMath(_));
                            let n = window[range.clone()]
                                .bytes()
                                .take_while(|b| *b == b'$')
                                .count();
                            open.spans.push(
                                RawSpan::new(range.clone(), kind::MATH)
                                    .aux(if display {
                                        kind::MATH_BLOCK
                                    } else {
                                        kind::MATH_INLINE
                                    })
                                    .attr("latex"),
                            );
                            delimiters(&mut open.spans, &range, n, n);
                        }
                        Event::Rule if !suppressed => {
                            open.spans.push(RawSpan::new(range.clone(), kind::RULE));
                        }
                        _ => {}
                    }
                }
                if standalone && let Some(open) = cur.take() {
                    blocks.push(finish(open, from));
                }
            }
        }
    }
    // The parser balances its events; an open block here would be a bug, but
    // keep what we have rather than lose the spans.
    if let Some(open) = cur.take() {
        blocks.push(finish(open, from));
    }
    blocks
}

fn finish(open: Open, window_from: usize) -> Block {
    let rel = open.start;
    Block {
        start: window_from + rel,
        spans: open
            .spans
            .into_iter()
            .map(|mut s| {
                s.range = s.range.start.saturating_sub(rel)..s.range.end.saturating_sub(rel);
                s
            })
            .collect(),
        open: open.open,
    }
}

fn frame_tag(tag: &Tag<'_>, window: &str, range: &Range<usize>, stack: &[Frame]) -> FrameTag {
    match tag {
        Tag::Heading { level, .. } => FrameTag::Heading(match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }),
        Tag::Strong => FrameTag::Strong,
        Tag::Emphasis => FrameTag::Emph,
        Tag::Strikethrough => FrameTag::Strike,
        // Reference-style links and images resolve through definitions that
        // may sit anywhere in the document, outside the window being parsed,
        // so they are left as source text: their spans would depend on text
        // the window cannot see. Inline links and autolinks are decorated.
        Tag::Link {
            link_type: LinkType::Inline | LinkType::Autolink | LinkType::Email,
            dest_url,
            ..
        } => FrameTag::Link(dest_url.to_string()),
        Tag::Image {
            link_type: LinkType::Inline,
            dest_url,
            ..
        } => FrameTag::Image(dest_url.to_string()),
        Tag::BlockQuote(_) => FrameTag::Quote,
        Tag::CodeBlock(CodeBlockKind::Fenced(info)) => FrameTag::CodeBlock(
            info.split_whitespace()
                .next()
                .filter(|l| !l.is_empty())
                .map(str::to_owned),
        ),
        Tag::CodeBlock(CodeBlockKind::Indented) => FrameTag::CodeBlock(None),
        Tag::List(start) => FrameTag::List(start.is_some()),
        Tag::Item => {
            let ordered = stack
                .iter()
                .rev()
                .find_map(|f| match f.tag {
                    FrameTag::List(o) => Some(o),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    list_prefix(&window[range.start..text::line_end(window, range.start)])
                        .is_some_and(|(o, _, _)| o)
                });
            FrameTag::Item(ordered)
        }
        Tag::Table(_) => FrameTag::Table,
        Tag::HtmlBlock => FrameTag::Html,
        _ => FrameTag::Other,
    }
}

/// Push marker spans for `open` bytes at the start and `close` at the end.
fn delimiters(out: &mut Vec<RawSpan>, range: &Range<usize>, open: usize, close: usize) {
    if open == 0 || close == 0 || range.len() <= open + close {
        return;
    }
    out.push(RawSpan::new(range.start..range.start + open, kind::MARKER));
    out.push(RawSpan::new(range.end - close..range.end, kind::MARKER));
}

/// `r` without the line breaks (and blanks) that end a block element.
fn trim_end(window: &str, r: &Range<usize>) -> Range<usize> {
    let kept = window[r.clone()]
        .trim_end_matches(['\n', '\r', ' ', '\t'])
        .len();
    r.start..r.start + kept.max(1).min(r.len())
}

fn emit(frame: &Frame, window: &str, quote_depth: usize, out: &mut Vec<RawSpan>) {
    let trimmed;
    let r = match &frame.tag {
        FrameTag::Heading(_)
        | FrameTag::Quote
        | FrameTag::CodeBlock(_)
        | FrameTag::Item(_)
        | FrameTag::Table
        | FrameTag::Html => {
            trimmed = trim_end(window, &frame.range);
            &trimmed
        }
        _ => &frame.range,
    };
    match &frame.tag {
        FrameTag::Heading(level) => {
            out.push(RawSpan::new(r.clone(), kind::HEADING).aux(*level));
            let line_end = text::line_end(window, r.start).min(r.end);
            if let Some((_, p)) = atx_prefix(&window[text::line_start(window, r.start)..line_end]) {
                let ls = text::line_start(window, r.start);
                out.push(RawSpan::new(
                    r.start..(ls + p).clamp(r.start, r.end),
                    kind::MARKER,
                ));
            }
        }
        FrameTag::Strong => {
            out.push(RawSpan::new(r.clone(), kind::STRONG));
            delimiters(out, r, 2, 2);
        }
        FrameTag::Emph => {
            out.push(RawSpan::new(r.clone(), kind::EMPH));
            delimiters(out, r, 1, 1);
        }
        FrameTag::Strike => {
            out.push(RawSpan::new(r.clone(), kind::STRIKE));
            let n = window[r.clone()].bytes().take_while(|b| *b == b'~').count();
            delimiters(out, r, n, n);
        }
        FrameTag::Link(dest) => {
            out.push(RawSpan::new(r.clone(), kind::LINK).attr(dest.clone()));
            if let Some(c) = &frame.content
                && c.start > r.start
                && c.end < r.end
            {
                out.push(RawSpan::new(r.start..c.start, kind::MARKER));
                out.push(RawSpan::new(c.end..r.end, kind::MARKER));
            }
        }
        FrameTag::Image(src) => {
            out.push(RawSpan::new(r.clone(), kind::IMAGE).attr(src.clone()));
        }
        FrameTag::Quote => {
            out.push(RawSpan::new(r.clone(), kind::QUOTE));
            quote_markers(window, r, quote_depth, out);
        }
        FrameTag::CodeBlock(lang) => {
            let mut span = RawSpan::new(r.clone(), kind::CODE_BLOCK);
            if let Some(l) = lang {
                span = span.attr(l.clone());
            }
            out.push(span);
            fence_markers(window, r, out);
        }
        FrameTag::Item(ordered) => {
            let aux = match (&frame.task, ordered) {
                (Some((true, _)), _) => kind::LIST_TASK_DONE,
                (Some((false, _)), _) => kind::LIST_TASK_OPEN,
                (None, true) => kind::LIST_ORDERED,
                (None, false) => kind::LIST_BULLET,
            };
            out.push(RawSpan::new(r.clone(), kind::LIST_ITEM).aux(aux));
            let b = window.as_bytes();
            let mut i = r.start;
            if matches!(b.get(i), Some(b'-' | b'*' | b'+')) {
                i += 1;
            } else {
                while b.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if matches!(b.get(i), Some(b'.' | b')')) {
                    i += 1;
                }
            }
            if let Some((_, t)) = &frame.task {
                i = i.max(t.end);
            }
            let mut j = i;
            while j < r.end && matches!(b.get(j), Some(b' ' | b'\t')) && j - i < 4 {
                j += 1;
            }
            if j > r.start {
                out.push(RawSpan::new(r.start..j.min(r.end), kind::MARKER));
            }
        }
        FrameTag::Table => out.push(RawSpan::new(r.clone(), kind::TABLE)),
        FrameTag::Html => out.push(RawSpan::new(r.clone(), kind::OPAQUE).attr("html")),
        FrameTag::List(_) | FrameTag::Other => {}
    }
}

fn quote_markers(window: &str, r: &Range<usize>, depth: usize, out: &mut Vec<RawSpan>) {
    let b = window.as_bytes();
    let mut line = r.start;
    let mut first = true;
    while line < r.end {
        let mut i = line;
        if !first {
            while matches!(b.get(i), Some(b' ' | b'\t')) {
                i += 1;
            }
            // Outer quotes mark this line first.
            for _ in 1..depth {
                if b.get(i) == Some(&b'>') {
                    i += 1;
                    if b.get(i) == Some(&b' ') {
                        i += 1;
                    }
                    while matches!(b.get(i), Some(b' ' | b'\t')) {
                        i += 1;
                    }
                }
            }
        }
        if b.get(i) == Some(&b'>') && i < r.end {
            let mut j = i + 1;
            if b.get(j) == Some(&b' ') {
                j += 1;
            }
            out.push(RawSpan::new(i..j.min(r.end), kind::MARKER));
        }
        first = false;
        match window[line..r.end].find('\n') {
            Some(n) => line += n + 1,
            None => break,
        }
    }
}

fn fence_markers(window: &str, r: &Range<usize>, out: &mut Vec<RawSpan>) {
    let first_end = text::line_end(window, r.start).min(r.end);
    let opening = window[r.start..first_end].trim_start();
    let fence = match opening.bytes().next() {
        Some(c @ (b'`' | b'~')) if opening.bytes().take_while(|b| *b == c).count() >= 3 => c,
        _ => return,
    };
    out.push(RawSpan::new(r.start..first_end, kind::MARKER));
    let end = {
        let t = &window[r.clone()];
        r.start + t.trim_end_matches(['\n', '\r']).len()
    };
    let last_start = text::line_start(window, end.saturating_sub(1).max(r.start));
    if last_start > first_end {
        let line = window[last_start..end].trim_start();
        if line.bytes().take_while(|b| *b == fence).count() >= 3 {
            let indent = window[last_start..end].len() - line.len();
            out.push(RawSpan::new(last_start + indent..end, kind::MARKER));
        }
    }
}

#[cfg(test)]
mod tests;
