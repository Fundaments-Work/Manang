//! The Typst format adapter of the Visual editor (ADR 0015).
//!
//! Built on `typst-syntax`: `Source::edit` reparses incrementally, and spans
//! come from a walk of the *syntax* tree restricted to the viewport. Only what
//! the syntax tree shows is claimed (markup: headings, emphasis, raw, lists,
//! links, references, labels, math, comments; and `#image`, `#figure`,
//! `#link` calls). Scripting (`#set`, `#show`, `#let`, `#import`, other
//! calls) is reported `opaque` or left alone: a heading produced by a
//! function is never mistaken for structure.

use std::ops::Range;

use manang_visual::kind;
use manang_visual::text::{self, Enclosing};
use manang_visual::{
    AdapterError, AdapterInfo, AdapterSession, CommandState, EditPlan, FormatAdapter, OutlineItem,
    RawSpan, TextEdit,
};
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Commands the adapter implements.
pub const COMMANDS: &[&str] = &[
    "bold", "italic", "code", "heading", "list", "link", "image", "figure", "math",
];

/// The Typst adapter. Register it with the visual host.
pub struct TypstAdapter {
    info: AdapterInfo,
}

impl TypstAdapter {
    /// The adapter and its declaration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            info: AdapterInfo {
                id: "typst".to_owned(),
                title: "Typst".to_owned(),
                extensions: vec!["typ".to_owned()],
                kinds: [
                    kind::HEADING,
                    kind::STRONG,
                    kind::EMPH,
                    kind::CODE,
                    kind::CODE_BLOCK,
                    kind::LIST_ITEM,
                    kind::LINK,
                    kind::IMAGE,
                    kind::MATH,
                    kind::REF,
                    kind::LABEL,
                    kind::COMMENT,
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

impl Default for TypstAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatAdapter for TypstAdapter {
    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn open(&self, text: &str) -> Result<Box<dyn AdapterSession>, AdapterError> {
        Ok(Box::new(TypstSession::new(text)))
    }
}

/// A parsed Typst document.
pub struct TypstSession {
    source: Source,
}

impl TypstSession {
    /// Parse `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            source: Source::detached(text),
        }
    }

    fn near(&self, sel: &Range<usize>) -> Vec<RawSpan> {
        let from = text::line_start(self.source.text(), sel.start);
        let to = text::line_end(self.source.text(), sel.end);
        let mut out = Vec::new();
        collect(&self.source, from..to, &mut out);
        out
    }

    /// The innermost span of `kind_id` that holds the selection, with its
    /// delimiters taken from the marker spans.
    fn enclosing(&self, kind_id: u16, sel: &Range<usize>) -> Option<Enclosing> {
        let all = self.near(sel);
        all.iter()
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
                (open > 0 && close > 0 && e.holds(sel)).then_some(e)
            })
            .min_by_key(|e| e.outer.len())
    }

    fn wrap(&self, sel: &Range<usize>, k: u16, open: &str, close: &str) -> EditPlan {
        let enclosing = self.enclosing(k, sel);
        text::toggle_wrap(
            self.source.text(),
            sel.clone(),
            open,
            close,
            enclosing.as_ref(),
        )
    }

    fn line(&self, pos: usize) -> &str {
        let t = self.source.text();
        &t[text::line_start(t, pos)..text::line_end(t, pos)]
    }
}

impl AdapterSession for TypstSession {
    fn edit(&mut self, range: Range<usize>, insert: &str) -> Result<Range<usize>, AdapterError> {
        let t = self.source.text();
        if range.start > range.end
            || range.end > t.len()
            || !t.is_char_boundary(range.start)
            || !t.is_char_boundary(range.end)
        {
            return Err(AdapterError::Internal(format!(
                "edit {}..{} is outside the {} byte text",
                range.start,
                range.end,
                t.len()
            )));
        }
        let reparsed = self.source.edit(range.clone(), insert);
        // Spans of neighbouring lines (list markers, heading prefixes) can
        // change with the edited line: widen to whole lines.
        let t = self.source.text();
        let end = (range.start + insert.len()).max(reparsed.end).min(t.len());
        Ok(text::line_start(t, reparsed.start.min(range.start))..text::line_end(t, end))
    }

    fn spans(&self, viewport: Range<usize>, out: &mut Vec<RawSpan>) -> Result<(), AdapterError> {
        collect(&self.source, viewport, out);
        Ok(())
    }

    fn outline(&self) -> Vec<OutlineItem> {
        let mut all = Vec::new();
        collect(&self.source, 0..self.source.text().len(), &mut all);
        all.iter()
            .filter(|s| s.kind == kind::HEADING)
            .map(|s| OutlineItem {
                level: s.aux,
                title: heading_title(&self.source.text()[s.range.clone()]),
                range: s.range.clone(),
            })
            .collect()
    }

    fn command_state(&self, sel: Range<usize>) -> Vec<CommandState> {
        let sel = text::clamp(self.source.text(), sel);
        let line = self.line(sel.start);
        let active = |k: u16| self.enclosing(k, &sel).is_some();
        let near = self.near(&sel);
        let inside_leaf = near.iter().any(|s| {
            matches!(s.kind, kind::CODE_BLOCK | kind::OPAQUE | kind::COMMENT)
                && s.range.start <= sel.start
                && sel.end <= s.range.end
        });
        let mut v = vec![
            CommandState::new("bold", !inside_leaf, active(kind::STRONG)),
            CommandState::new("italic", !inside_leaf, active(kind::EMPH)),
            CommandState::new("code", !inside_leaf, active(kind::CODE)),
            CommandState::new("heading", true, heading_prefix(line).is_some()),
            CommandState::new("list", true, list_prefix(line).is_some()),
            CommandState::new("link", !inside_leaf, active(kind::LINK)),
            CommandState::new("image", true, false),
            CommandState::new("figure", true, false),
            CommandState::new("math", true, active(kind::MATH)),
        ];
        v.retain(|c| COMMANDS.contains(&c.command.as_str()));
        v
    }

    fn run_command(
        &self,
        command: &str,
        sel: Range<usize>,
        args: &str,
    ) -> Result<EditPlan, AdapterError> {
        let t = self.source.text();
        let sel = text::clamp(t, sel);
        match command {
            "bold" => Ok(self.wrap(&sel, kind::STRONG, "*", "*")),
            "italic" => Ok(self.wrap(&sel, kind::EMPH, "_", "_")),
            "code" => {
                if t[sel.clone()].contains('\n') {
                    let rep = format!("```\n{}\n```", t[sel.clone()].trim_matches('\n'));
                    let len = rep.len();
                    return Ok(text::replace_selection(t, sel, rep, 4..len - 4));
                }
                Ok(self.wrap(&sel, kind::CODE, "`", "`"))
            }
            "math" => {
                if args == "block" && self.enclosing(kind::MATH, &sel).is_none() {
                    let body = t[sel.clone()].trim();
                    let rep = format!("$ {body} $");
                    let len = rep.len();
                    return Ok(text::replace_selection(t, sel, rep, 2..len - 2));
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
            "link" => Ok(self.link(&sel, args.trim())),
            "image" | "figure" => {
                let path = args.trim();
                if path.is_empty() {
                    return Err(AdapterError::InvalidArgument(format!(
                        "`{command}` needs a path"
                    )));
                }
                let quoted = quote(path);
                let rep = if command == "image" {
                    format!("#image({quoted})")
                } else {
                    let caption = t[sel.clone()].trim();
                    format!("#figure(image({quoted}), caption: [{caption}])")
                };
                let len = rep.len();
                // A figure with an empty caption leaves the caret in it.
                let caret = if command == "figure" && t[sel.clone()].trim().is_empty() {
                    len - 2..len - 2
                } else {
                    len..len
                };
                Ok(text::replace_selection(t, sel, rep, caret))
            }
            other => Err(AdapterError::Unsupported(format!(
                "the Typst adapter has no `{other}` command"
            ))),
        }
    }
}

/// A Typst string literal.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// `(level, prefix_len)` of a heading line (`=== Title`).
fn heading_prefix(line: &str) -> Option<(usize, usize)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let eq = rest.bytes().take_while(|b| *b == b'=').count();
    if eq == 0 || eq > 6 {
        return None;
    }
    let after = &rest[eq..];
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }
    let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
    Some((eq, indent + eq + spaces))
}

/// `(ordered, indent_len, marker_len_with_spaces)` of a list line (`- `, `+ `, `1. `).
fn list_prefix(line: &str) -> Option<(bool, usize, usize)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let b = rest.as_bytes();
    let (ordered, mlen) = match b.first() {
        Some(b'-') => (false, 1),
        Some(b'+') => (true, 1),
        Some(c) if c.is_ascii_digit() => {
            let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
            if b.get(digits) != Some(&b'.') {
                return None;
            }
            (true, digits + 1)
        }
        _ => return None,
    };
    let after = &rest[mlen..];
    if !after.starts_with([' ', '\t']) {
        return None;
    }
    let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
    Some((ordered, indent, mlen + spaces))
}

impl TypstSession {
    fn heading(&self, sel: &Range<usize>, level: usize) -> EditPlan {
        let t = self.source.text();
        let lines = text::lines_of(t, sel);
        let current = heading_prefix(&t[lines[0].clone()]).map_or(0, |(l, _)| l);
        let target = if current == level { 0 } else { level };
        let mut edits = Vec::new();
        for line in &lines {
            let s = &t[line.clone()];
            if s.trim().is_empty() && target > 0 {
                continue;
            }
            let replaced = heading_prefix(s).map_or(0, |(_, p)| p);
            let prefix = if target == 0 {
                String::new()
            } else {
                format!("{} ", "=".repeat(target))
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
        let t = self.source.text();
        let lines: Vec<Range<usize>> = text::lines_of(t, sel)
            .into_iter()
            .filter(|l| !t[l.clone()].trim().is_empty())
            .collect();
        let all_match = !lines.is_empty()
            && lines
                .iter()
                .all(|l| list_prefix(&t[l.clone()]).is_some_and(|(o, _, _)| o == ordered));
        let mut edits = Vec::new();
        for line in &lines {
            let s = &t[line.clone()];
            let (indent, old_len) = list_prefix(s).map_or_else(
                || (s.len() - s.trim_start().len(), 0),
                |(_, indent, len)| (indent, len),
            );
            let marker = if all_match {
                ""
            } else if ordered {
                "+ "
            } else {
                "- "
            };
            let from = line.start + indent;
            edits.push(TextEdit::replace(from..from + old_len, marker));
        }
        let selection =
            Some(text::map_through(&edits, sel.start)..text::map_through(&edits, sel.end));
        EditPlan { edits, selection }
    }

    fn link(&self, sel: &Range<usize>, url: &str) -> EditPlan {
        let t = self.source.text();
        if let Some(e) = self.enclosing(kind::LINK, sel) {
            return text::toggle_wrap(t, sel.clone(), "", "", Some(&e));
        }
        let target = if sel.is_empty() {
            text::expand_word(t, sel.clone())
        } else {
            sel.clone()
        };
        let (label, placeholder) = if target.is_empty() {
            ("link".to_owned(), true)
        } else {
            (t[target.clone()].to_owned(), false)
        };
        let head = format!("#link({})[", quote(url));
        let rep = format!("{head}{label}]");
        let (a, b) = if url.is_empty() {
            // Caret between the quotes.
            let at = "#link(\"".len();
            (at, at)
        } else if placeholder {
            (head.len(), head.len() + label.len())
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

/// The text of a heading without its marker and trailing label.
fn heading_title(src: &str) -> String {
    let first = src.lines().next().unwrap_or("");
    let body = match heading_prefix(first) {
        Some((_, p)) => &first[p..],
        None => first,
    };
    let body = body.trim_end();
    match body.rfind('<') {
        Some(i) if body.ends_with('>') && !body[..i].trim().is_empty() => body[..i].trim_end(),
        _ => body,
    }
    .to_owned()
}

// ---------------------------------------------------------------- syntax walk

/// Spans of `source` that intersect `viewport`, any order.
fn collect(source: &Source, viewport: Range<usize>, out: &mut Vec<RawSpan>) {
    let mut walker = Walker {
        text: source.text(),
        viewport,
        out,
    };
    walker.markup(source.root(), 0);
}

struct Walker<'a, 'o> {
    text: &'a str,
    viewport: Range<usize>,
    out: &'o mut Vec<RawSpan>,
}

impl Walker<'_, '_> {
    fn hit(&self, r: &Range<usize>) -> bool {
        r.end >= self.viewport.start && r.start <= self.viewport.end
    }

    fn push(&mut self, span: RawSpan) {
        // Error recovery can leave a node short of its closing token; never
        // hand the host a range that is not on character boundaries.
        let r = &span.range;
        if r.start <= r.end
            && r.end <= self.text.len()
            && self.text.is_char_boundary(r.start)
            && self.text.is_char_boundary(r.end)
        {
            self.out.push(span);
        }
    }

    fn marker(&mut self, r: Range<usize>) {
        if r.start < r.end {
            self.push(RawSpan::new(r, kind::MARKER));
        }
    }

    /// Children of a markup-like node: text, formatting, `#` expressions.
    fn markup(&mut self, node: &SyntaxNode, start: usize) {
        let mut at = start;
        let mut hash: Option<usize> = None;
        for child in node.children() {
            let range = at..at + child.len();
            at = range.end;
            let pending = hash.take();
            if !self.hit(&range) {
                if child.kind() == SyntaxKind::Hash {
                    hash = Some(range.start);
                }
                continue;
            }
            match child.kind() {
                SyntaxKind::Hash => hash = Some(range.start),
                SyntaxKind::Heading => self.heading(child, range),
                SyntaxKind::Strong => {
                    self.push(RawSpan::new(range.clone(), kind::STRONG));
                    self.delimited(child, range, SyntaxKind::Star);
                }
                SyntaxKind::Emph => {
                    self.push(RawSpan::new(range.clone(), kind::EMPH));
                    self.delimited(child, range, SyntaxKind::Underscore);
                }
                SyntaxKind::Raw => self.raw(child, range),
                SyntaxKind::ListItem | SyntaxKind::EnumItem => self.item(child, range),
                SyntaxKind::TermItem => self.markup(child, range.start),
                SyntaxKind::Link => {
                    let url = self.text[range.clone()].to_owned();
                    self.push(RawSpan::new(range, kind::LINK).attr(url));
                }
                SyntaxKind::Ref => {
                    let key = ref_key(&self.text[range.clone()]);
                    self.push(RawSpan::new(range, kind::REF).attr(key));
                }
                SyntaxKind::Label => {
                    let key = self.text[range.clone()].trim_matches(['<', '>']).to_owned();
                    self.push(RawSpan::new(range, kind::LABEL).attr(key));
                }
                SyntaxKind::Equation => self.equation(child, range),
                SyntaxKind::LineComment | SyntaxKind::BlockComment => {
                    self.push(RawSpan::new(range, kind::COMMENT));
                }
                SyntaxKind::Markup | SyntaxKind::ContentBlock => self.markup(child, range.start),
                SyntaxKind::SetRule
                | SyntaxKind::ShowRule
                | SyntaxKind::LetBinding
                | SyntaxKind::ModuleImport
                | SyntaxKind::ModuleInclude => {
                    let from = pending.unwrap_or(range.start);
                    let label = match child.kind() {
                        SyntaxKind::SetRule => "set",
                        SyntaxKind::ShowRule => "show",
                        SyntaxKind::LetBinding => "let",
                        SyntaxKind::ModuleImport => "import",
                        _ => "include",
                    };
                    self.push(RawSpan::new(from..range.end, kind::OPAQUE).attr(label));
                }
                SyntaxKind::FuncCall => self.call(child, range, pending),
                // Spaces, text, escapes, strings, numbers: nothing to say.
                _ => {
                    if child.children().len() > 0 {
                        self.code(child, range.start);
                    }
                }
            }
        }
    }

    /// Code: only the content blocks in it are markup.
    fn code(&mut self, node: &SyntaxNode, start: usize) {
        let mut at = start;
        for child in node.children() {
            let range = at..at + child.len();
            at = range.end;
            if !self.hit(&range) {
                continue;
            }
            if child.kind() == SyntaxKind::ContentBlock {
                self.markup(child, range.start);
            } else if child.children().len() > 0 {
                self.code(child, range.start);
            }
        }
    }

    fn heading(&mut self, node: &SyntaxNode, range: Range<usize>) {
        let mut at = range.start;
        let mut level = 0usize;
        let mut marker_end = range.start;
        for (i, child) in node.children().enumerate() {
            let end = at + child.len();
            if i == 0 && child.kind() == SyntaxKind::HeadingMarker {
                level = child.len();
                marker_end = end;
                // The space after the markers hides with them.
                if node
                    .children()
                    .nth(1)
                    .is_some_and(|c| c.kind() == SyntaxKind::Space)
                {
                    marker_end += node.children().nth(1).map_or(0, SyntaxNode::len);
                }
            }
            at = end;
        }
        self.push(RawSpan::new(range.clone(), kind::HEADING).aux(level.min(6) as u8));
        self.marker(range.start..marker_end.min(range.end));
        self.markup(node, range.start);
    }

    /// The opening and closing `delim` tokens become markers; children recurse.
    fn delimited(&mut self, node: &SyntaxNode, range: Range<usize>, delim: SyntaxKind) {
        let n = node.children().len();
        let mut at = range.start;
        for (i, child) in node.children().enumerate() {
            let end = at + child.len();
            if child.kind() == delim && (i == 0 || i + 1 == n) {
                self.marker(at..end);
            }
            at = end;
        }
        self.markup(node, range.start);
    }

    fn raw(&mut self, node: &SyntaxNode, range: Range<usize>) {
        let text = &self.text[range.clone()];
        let ticks = text.bytes().take_while(|b| *b == b'`').count();
        let block = ticks >= 3 && text.contains('\n');
        let mut span = RawSpan::new(
            range.clone(),
            if block { kind::CODE_BLOCK } else { kind::CODE },
        );
        let mut at = range.start;
        let mut markers = Vec::new();
        for child in node.children() {
            let end = at + child.len();
            match child.kind() {
                SyntaxKind::RawDelim => markers.push(at..end),
                SyntaxKind::RawLang if block => span = span.attr(&self.text[at..end]),
                _ => {}
            }
            at = end;
        }
        self.push(span);
        for m in markers {
            self.marker(m);
        }
    }

    fn item(&mut self, node: &SyntaxNode, range: Range<usize>) {
        let ordered = node.kind() == SyntaxKind::EnumItem;
        self.push(
            RawSpan::new(range.clone(), kind::LIST_ITEM).aux(if ordered {
                kind::LIST_ORDERED
            } else {
                kind::LIST_BULLET
            }),
        );
        let mut at = range.start;
        let kids: Vec<&SyntaxNode> = node.children().collect();
        for (i, child) in kids.iter().enumerate() {
            let end = at + child.len();
            if matches!(
                child.kind(),
                SyntaxKind::ListMarker | SyntaxKind::EnumMarker
            ) {
                let mut m_end = end;
                if kids
                    .get(i + 1)
                    .is_some_and(|c| c.kind() == SyntaxKind::Space)
                {
                    m_end += kids[i + 1].len();
                }
                self.marker(at..m_end.min(range.end));
            }
            at = end;
        }
        self.markup(node, range.start);
    }

    fn equation(&mut self, node: &SyntaxNode, range: Range<usize>) {
        let text = &self.text[range.clone()];
        let inner = text.trim_start_matches('$');
        let block = inner.starts_with(char::is_whitespace)
            && inner.trim_end_matches('$').ends_with(char::is_whitespace);
        self.push(
            RawSpan::new(range.clone(), kind::MATH)
                .aux(if block {
                    kind::MATH_BLOCK
                } else {
                    kind::MATH_INLINE
                })
                .attr("typst"),
        );
        let n = node.children().len();
        let mut at = range.start;
        for (i, child) in node.children().enumerate() {
            let end = at + child.len();
            if child.kind() == SyntaxKind::Dollar && (i == 0 || i + 1 == n) {
                self.marker(at..end);
            }
            at = end;
        }
    }

    /// `#link`, `#image`, `#figure`; any other call is code.
    fn call(&mut self, node: &SyntaxNode, range: Range<usize>, hash: Option<usize>) {
        let from = hash.unwrap_or(range.start);
        let callee = node
            .children()
            .next()
            .filter(|c| c.kind() == SyntaxKind::Ident)
            .map(|c| &self.text[range.start..range.start + c.len()]);
        match callee {
            Some("link") => {
                let url = first_string(self.text, node, range.start).unwrap_or_default();
                self.push(RawSpan::new(from..range.end, kind::LINK).attr(url));
                // A trailing content block `[label]` is the visible text:
                // everything around it is syntax.
                if let Some(block) = trailing_content_block(node, range.start)
                    && self.text[block.clone()].ends_with(']')
                    && block.len() >= 2
                {
                    self.marker(from..block.start + 1);
                    self.marker(block.end - 1..range.end);
                }
                self.code(node, range.start);
            }
            Some("image") => {
                let src = first_string(self.text, node, range.start).unwrap_or_default();
                self.push(RawSpan::new(from..range.end, kind::IMAGE).attr(src));
            }
            Some("figure") => {
                let src = nested_image_path(self.text, node, range.start).unwrap_or_default();
                self.push(
                    RawSpan::new(from..range.end, kind::IMAGE)
                        .aux(kind::IMAGE_FIGURE)
                        .attr(src),
                );
            }
            _ => self.code(node, range.start),
        }
    }
}

fn ref_key(src: &str) -> String {
    src.trim_start_matches('@')
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
        .collect::<String>()
        .trim_end_matches('.')
        .to_owned()
}

/// The first string literal argument of a call, unescaped.
fn first_string(text: &str, call: &SyntaxNode, start: usize) -> Option<String> {
    let mut at = start;
    for child in call.children() {
        let range = at..at + child.len();
        at = range.end;
        if child.kind() == SyntaxKind::Args {
            let mut inner = range.start;
            for arg in child.children() {
                let r = inner..inner + arg.len();
                inner = r.end;
                if arg.kind() == SyntaxKind::Str {
                    return Some(unquote(&text[r]));
                }
            }
        }
    }
    None
}

/// The `image("..")` path inside a `figure` call.
fn nested_image_path(text: &str, node: &SyntaxNode, start: usize) -> Option<String> {
    let mut at = start;
    for child in node.children() {
        let range = at..at + child.len();
        at = range.end;
        if child.kind() == SyntaxKind::FuncCall {
            let is_image = child.children().next().is_some_and(|c| {
                c.kind() == SyntaxKind::Ident
                    && &text[range.start..range.start + c.len()] == "image"
            });
            if is_image {
                return first_string(text, child, range.start);
            }
        }
        if child.children().len() > 0
            && let Some(p) = nested_image_path(text, child, range.start)
        {
            return Some(p);
        }
    }
    None
}

/// Byte range of the trailing `[...]` argument of a call, if it has one.
fn trailing_content_block(call: &SyntaxNode, start: usize) -> Option<Range<usize>> {
    let mut at = start;
    for child in call.children() {
        let range = at..at + child.len();
        at = range.end;
        if child.kind() == SyntaxKind::Args {
            let mut inner = range.start;
            let mut last = None;
            for arg in child.children() {
                let r = inner..inner + arg.len();
                inner = r.end;
                if arg.kind() == SyntaxKind::ContentBlock {
                    last = Some(r);
                }
            }
            return last.filter(|b| b.end == range.end);
        }
    }
    None
}

/// The contents of a Typst string literal.
fn unquote(lit: &str) -> String {
    let inner = lit
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(lit);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests;
