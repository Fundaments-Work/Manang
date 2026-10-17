//! Prose extraction.
//!
//! Every feature (spelling, prose lint, statistics) works on *prose*: the
//! human-readable words of a document. [`prose`] returns the text with every
//! non-prose character (code, math, comments, commands, URLs, citation keys)
//! replaced by [`OPAQUE`], one for one. Because the replacement is
//! character-for-character, character offsets into the masked text are valid
//! offsets into the original, and the opaque marker keeps neighbouring words
//! from being treated as adjacent (so `the $x$ the` is not a repeated word).
//! Newlines survive masking so paragraph structure is preserved.

use std::ops::Range;

use typst_syntax::{SyntaxKind, SyntaxNode};

use crate::syntax::Syntax;
use crate::text::CharRange;

/// Stands in for every masked character. A private-use code point: it is
/// neither whitespace nor alphanumeric, so it separates tokens.
pub const OPAQUE: char = '\u{E000}';

/// Mask everything in `text` that is not prose for `syntax`, plus the
/// caller's own `ignore` regions (character offsets).
pub fn prose(text: &str, syntax: Syntax, ignore: &[CharRange]) -> String {
    let spans = match syntax.resolve(text) {
        Syntax::Typst => typst_spans(text),
        Syntax::Markdown => markdown_spans(text),
        Syntax::Latex => complement(text.len(), latex_masked(text, LatexMode::Latex)),
        Syntax::Generic => complement(text.len(), latex_masked(text, LatexMode::Generic)),
        Syntax::Plain | Syntax::Auto => std::iter::once(0..text.len()).collect(),
    };
    let mut out = mask_tokens(&apply_spans(text, &spans));
    if !ignore.is_empty() {
        out = mask_char_ranges(&out, ignore);
    }
    out
}

/// Keep `spans` (sorted, disjoint byte ranges), replace the rest.
fn apply_spans(text: &str, spans: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut next = spans.iter().peekable();
    for (byte, ch) in text.char_indices() {
        while next.next_if(|s| s.end <= byte).is_some() {}
        let keep = next.peek().is_some_and(|s| s.start <= byte);
        out.push(if keep || ch == '\n' { ch } else { OPAQUE });
    }
    out
}

fn mask_char_ranges(text: &str, ranges: &[CharRange]) -> String {
    let mut sorted: Vec<CharRange> = ranges.to_vec();
    sorted.sort_by_key(|r| r.start);
    let mut out = String::with_capacity(text.len());
    for (at, ch) in text.chars().enumerate() {
        let hidden = sorted.iter().any(|r| r.start <= at && at < r.end);
        out.push(if hidden && ch != '\n' { OPAQUE } else { ch });
    }
    out
}

/// Replace URLs, e-mail addresses, DOIs and citation keys (`@key`) found in
/// whitespace-delimited runs.
fn mask_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run_start = None;
    for (byte, ch) in text.char_indices() {
        match (ch.is_whitespace(), run_start) {
            (false, None) => run_start = Some(byte),
            (true, Some(start)) => {
                flush_run(&text[start..byte], &mut out);
                run_start = None;
                out.push(ch);
            }
            (true, None) => out.push(ch),
            (false, Some(_)) => {}
        }
    }
    if let Some(start) = run_start {
        flush_run(&text[start..], &mut out);
    }
    out
}

fn flush_run(run: &str, out: &mut String) {
    if opaque_token(run) {
        out.extend(run.chars().map(|_| OPAQUE));
    } else {
        out.push_str(run);
    }
}

fn opaque_token(run: &str) -> bool {
    let trimmed = run.trim_start_matches(['(', '[', '<', '"', '\'']);
    if run.contains("://") || trimmed.starts_with("www.") {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("mailto:") || lower.starts_with("doi:") {
        return true;
    }
    if lower.starts_with("10.") && lower.contains('/') {
        return true;
    }
    // E-mail addresses and pandoc/Markdown citation keys.
    run.contains('@') && run.chars().any(char::is_alphanumeric)
}

// ---------------------------------------------------------------------------
// Typst

fn typst_spans(text: &str) -> Vec<Range<usize>> {
    let root = typst_syntax::parse(text);
    let mut spans = Spans::default();
    // Explicit stack: nesting depth is attacker-controlled.
    let mut stack: Vec<(&SyntaxNode, usize)> = vec![(&root, 0)];
    let mut children = Vec::new();
    while let Some((node, offset)) = stack.pop() {
        match node.kind() {
            SyntaxKind::Text
            | SyntaxKind::Space
            | SyntaxKind::Parbreak
            | SyntaxKind::SmartQuote => spans.push(offset..offset + node.len()),
            SyntaxKind::Equation
            | SyntaxKind::Raw
            | SyntaxKind::LineComment
            | SyntaxKind::BlockComment
            | SyntaxKind::Link
            | SyntaxKind::Ref
            | SyntaxKind::Label
            | SyntaxKind::Escape
            | SyntaxKind::Shorthand
            | SyntaxKind::Shebang
            | SyntaxKind::Error => {}
            _ => {
                children.clear();
                let mut at = offset;
                for child in node.children() {
                    children.push((child, at));
                    at += child.len();
                }
                stack.extend(children.drain(..).rev());
            }
        }
    }
    spans.finish()
}

// ---------------------------------------------------------------------------
// Markdown

fn markdown_spans(text: &str) -> Vec<Range<usize>> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut spans = Spans::default();
    let mut skipping = 0usize;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_)) => skipping += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_)) => {
                skipping = skipping.saturating_sub(1);
            }
            Event::Text(_) | Event::SoftBreak | Event::HardBreak if skipping == 0 => {
                spans.push(range);
            }
            _ => {}
        }
    }
    spans.finish()
}

// ---------------------------------------------------------------------------
// LaTeX (and the syntax-agnostic scan)

#[derive(Clone, Copy, PartialEq, Eq)]
enum LatexMode {
    Latex,
    Generic,
}

/// Environments whose body is never prose.
const OPAQUE_ENVS: &[&str] = &[
    "equation",
    "align",
    "alignat",
    "flalign",
    "gather",
    "multline",
    "eqnarray",
    "math",
    "displaymath",
    "split",
    "verbatim",
    "lstlisting",
    "minted",
    "tikzpicture",
    "pgfpicture",
    "comment",
    "filecontents",
    "algorithmic",
    "thebibliography",
];

/// Environments that take a column or layout specification right after
/// `\begin{...}`.
const SPEC_ENVS: &[&str] = &[
    "tabular",
    "tabularx",
    "tabulary",
    "longtable",
    "array",
    "minipage",
    "multicols",
];

/// Commands whose arguments (all consecutive `[..]` and `{..}` groups) are
/// identifiers, paths or code.
const ARG_OPAQUE: &[&str] = &[
    "cite",
    "citep",
    "citet",
    "citealp",
    "citeauthor",
    "citeyear",
    "parencite",
    "textcite",
    "autocite",
    "footcite",
    "nocite",
    "ref",
    "eqref",
    "autoref",
    "cref",
    "Cref",
    "pageref",
    "nameref",
    "vref",
    "label",
    "hyperref",
    "includegraphics",
    "usepackage",
    "RequirePackage",
    "documentclass",
    "input",
    "include",
    "includeonly",
    "bibliography",
    "bibliographystyle",
    "addbibresource",
    "url",
    "path",
    "texttt",
    "lstinline",
    "mintinline",
    "newcommand",
    "renewcommand",
    "providecommand",
    "newenvironment",
    "renewenvironment",
    "DeclareMathOperator",
    "newtheorem",
    "setlength",
    "pagestyle",
    "thispagestyle",
    "geometry",
    "hypersetup",
    "def",
    "let",
    "graphicspath",
    "color",
    "textcolor",
];

struct LatexScan<'a> {
    text: &'a str,
    bytes: &'a [u8],
    mode: LatexMode,
    masked: Vec<Range<usize>>,
}

fn latex_masked(text: &str, mode: LatexMode) -> Vec<Range<usize>> {
    let mut scan = LatexScan {
        text,
        bytes: text.as_bytes(),
        mode,
        masked: Vec::new(),
    };
    scan.run();
    scan.masked
}

impl LatexScan<'_> {
    fn run(&mut self) {
        let n = self.bytes.len();
        let mut i = 0;
        while i < n {
            i = match self.bytes[i] {
                b'%' if self.mode == LatexMode::Latex => {
                    let end = self.line_end(i);
                    self.masked.push(i..end);
                    end
                }
                b'\\' => self.command(i),
                b'$' => self.dollar(i),
                b'`' if self.mode == LatexMode::Generic => self.backtick(i),
                b'#' if self.mode == LatexMode::Generic => self.hash_call(i),
                _ => i + 1,
            };
        }
    }

    fn line_end(&self, from: usize) -> usize {
        self.bytes[from..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(self.bytes.len(), |p| from + p)
    }

    /// Mask `start..end` and return `end`.
    fn mask(&mut self, start: usize, end: usize) -> usize {
        self.masked.push(start..end);
        end
    }

    fn command(&mut self, start: usize) -> usize {
        let n = self.bytes.len();
        let mut i = start + 1;
        let Some(&first) = self.bytes.get(i) else {
            return self.mask(start, n);
        };
        if !first.is_ascii_alphabetic() {
            return self.symbol_command(start);
        }
        while i < n && self.bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let name = &self.text[start + 1..i];
        if self.bytes.get(i) == Some(&b'*') {
            i += 1;
        }
        match name {
            "begin" => self.begin(start, i),
            "end" => {
                let end = self.group_end(i).unwrap_or(i);
                self.mask(start, end)
            }
            "verb" => {
                let Some(&delim) = self.bytes.get(i) else {
                    return self.mask(start, i);
                };
                let close = self.bytes[i + 1..]
                    .iter()
                    .take_while(|&&b| b != b'\n')
                    .position(|&b| b == delim);
                let end = close.map_or(i, |p| i + 2 + p);
                self.mask(start, end)
            }
            _ if ARG_OPAQUE.contains(&name) => {
                let end = self.groups_end(i, usize::MAX);
                self.mask(start, end)
            }
            "href" => {
                let end = self.groups_end(i, 1);
                self.mask(start, end)
            }
            _ => self.mask(start, i),
        }
    }

    /// `\(`, `\[`, `\\`, `\%`, `\$`, ...
    fn symbol_command(&mut self, start: usize) -> usize {
        let after = start + 1;
        let width = self.text[after..].chars().next().map_or(0, char::len_utf8);
        let next = after + width;
        let closer = match self.bytes[after] {
            b'(' => "\\)",
            b'[' => "\\]",
            _ => return self.mask(start, next),
        };
        match self.text[next..].find(closer) {
            Some(p) => self.mask(start, next + p + 2),
            None => self.mask(start, next),
        }
    }

    fn begin(&mut self, start: usize, after_name: usize) -> usize {
        let Some(group_end) = self.group_end(after_name) else {
            return self.mask(start, after_name);
        };
        let env = self.text[after_name + 1..group_end - 1].trim_end_matches('*');
        if OPAQUE_ENVS.contains(&env) {
            let closer = format!("\\end{{{}", &self.text[after_name + 1..group_end - 1]);
            return match self.text[group_end..].find(&closer) {
                Some(p) => {
                    let tail = group_end + p + closer.len();
                    let end = if self.bytes.get(tail) == Some(&b'}') {
                        tail + 1
                    } else {
                        tail
                    };
                    self.mask(start, end)
                }
                None => self.mask(start, self.bytes.len()),
            };
        }
        let end = if SPEC_ENVS.contains(&env) {
            self.groups_end(group_end, usize::MAX)
        } else {
            self.groups_end(group_end, 0)
                .max(self.optional_end(group_end))
        };
        self.mask(start, end)
    }

    /// End of an immediately following `[...]` group, or `at` if none.
    fn optional_end(&self, at: usize) -> usize {
        if self.bytes.get(at) == Some(&b'[') {
            self.bracket_end(at, b'[', b']').unwrap_or(at)
        } else {
            at
        }
    }

    /// End of the run of up to `max` consecutive `[..]` / `{..}` groups (and
    /// bare `\commands`) starting at `at`, allowing spaces between them.
    fn groups_end(&self, at: usize, max: usize) -> usize {
        let mut end = at;
        let mut probe = at;
        let mut count = 0;
        while count < max {
            while matches!(self.bytes.get(probe), Some(b' ' | b'\t')) {
                probe += 1;
            }
            let next = match self.bytes.get(probe) {
                Some(b'{') => self.bracket_end(probe, b'{', b'}'),
                Some(b'[') => self.bracket_end(probe, b'[', b']'),
                Some(b'\\') if count == 0 => {
                    let mut e = probe + 1;
                    while self.bytes.get(e).is_some_and(u8::is_ascii_alphabetic) {
                        e += 1;
                    }
                    (e > probe + 1).then_some(e)
                }
                _ => None,
            };
            match next {
                Some(e) => {
                    end = e;
                    probe = e;
                    count += 1;
                }
                None => break,
            }
        }
        end
    }

    fn group_end(&self, at: usize) -> Option<usize> {
        (self.bytes.get(at) == Some(&b'{'))
            .then(|| self.bracket_end(at, b'{', b'}'))
            .flatten()
    }

    /// Index one past the bracket matching the one at `at`; `None` when
    /// unbalanced within the paragraph.
    fn bracket_end(&self, at: usize, open: u8, close: u8) -> Option<usize> {
        let mut depth = 0usize;
        let mut i = at;
        while i < self.bytes.len() {
            let b = self.bytes[i];
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'\n' && self.bytes.get(i + 1) == Some(&b'\n') {
                return None;
            }
            if b == open {
                depth += 1;
            } else if b == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            i += 1;
        }
        None
    }

    fn dollar(&mut self, start: usize) -> usize {
        let n = self.bytes.len();
        if self.bytes.get(start + 1) == Some(&b'$') {
            return match self.text[start + 2..].find("$$") {
                Some(p) => self.mask(start, start + 2 + p + 2),
                None => self.mask(start, start + 2),
            };
        }
        let mut i = start + 1;
        while i < n {
            match self.bytes[i] {
                b'\\' => i += 2,
                b'$' => return self.mask(start, i + 1),
                b'\n' if self.bytes.get(i + 1) == Some(&b'\n') => break,
                _ => i += 1,
            }
        }
        self.mask(start, start + 1)
    }

    fn backtick(&mut self, start: usize) -> usize {
        let ticks = self.bytes[start..]
            .iter()
            .take_while(|&&b| b == b'`')
            .count();
        let fence = "`".repeat(ticks);
        match self.text[start + ticks..].find(&fence) {
            Some(p) => self.mask(start, start + ticks + p + ticks),
            None => self.mask(start, start + ticks),
        }
    }

    /// `#name(args)` in agnostic mode.
    fn hash_call(&mut self, start: usize) -> usize {
        let n = self.bytes.len();
        let mut i = start + 1;
        if !self.bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
            return i;
        }
        while i < n
            && (self.bytes[i].is_ascii_alphanumeric()
                || matches!(self.bytes[i], b'-' | b'_' | b'.'))
        {
            i += 1;
        }
        let end = if self.bytes.get(i) == Some(&b'(') {
            self.bracket_end(i, b'(', b')').unwrap_or(i)
        } else {
            i
        };
        self.mask(start, end)
    }
}

// ---------------------------------------------------------------------------
// Span helpers

/// Sorted, merged byte ranges.
#[derive(Default)]
struct Spans(Vec<Range<usize>>);

impl Spans {
    fn push(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let start = match self.0.last_mut() {
            Some(last) if range.start <= last.end => {
                last.end = last.end.max(range.end);
                return;
            }
            Some(last) => range.start.max(last.end),
            None => range.start,
        };
        self.0.push(start..range.end);
    }

    fn finish(self) -> Vec<Range<usize>> {
        self.0
    }
}

/// Turn masked intervals into the prose intervals between them.
fn complement(len: usize, mut masked: Vec<Range<usize>>) -> Vec<Range<usize>> {
    masked.sort_by_key(|r| r.start);
    let mut spans = Spans::default();
    let mut at = 0;
    for r in masked {
        if r.start > at {
            spans.push(at..r.start);
        }
        at = at.max(r.end);
    }
    if at < len {
        spans.push(at..len);
    }
    spans.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible(text: &str, syntax: Syntax) -> String {
        prose(text, syntax, &[]).replace(OPAQUE, "_")
    }

    #[test]
    fn masking_is_one_for_one() {
        for (text, syntax) in [
            ("= Titel $x$ `code` #f(1)", Syntax::Typst),
            ("# T\n\n`a` $b$ [l](http://x)", Syntax::Markdown),
            ("\\section{A} $x$ % c\n\\cite{k}", Syntax::Latex),
            ("héllo wörld $é$", Syntax::Generic),
        ] {
            let masked = prose(text, syntax, &[]);
            assert_eq!(masked.chars().count(), text.chars().count(), "{text}");
        }
    }

    #[test]
    fn typst_keeps_prose_only() {
        let masked = visible(
            "= Intro\nThe *quick* fox $a+b$ jumps #cite(<k>) over @ref and `raw`.\n// hidden\n",
            Syntax::Typst,
        );
        assert!(masked.contains("Intro"));
        assert!(masked.contains("quick"));
        assert!(masked.contains("jumps"));
        assert!(masked.contains("over"));
        for gone in ["a+b", "cite", "ref", "raw", "hidden"] {
            assert!(!masked.contains(gone), "{gone} leaked: {masked}");
        }
    }

    #[test]
    fn typst_content_blocks_in_code_are_prose() {
        let masked = visible(
            "#figure(caption: [A caption here])[body text]",
            Syntax::Typst,
        );
        assert!(masked.contains("A caption here"));
        assert!(masked.contains("body text"));
        assert!(!masked.contains("figure"));
    }

    #[test]
    fn markdown_skips_code_math_html() {
        let masked = visible(
            "---\ntitle: x\n---\n# Head\n\ntext `code` and $m$ <b>x</b>\n\n```\nfenced words\n```\n\nsee [link text](http://u.rl)\n",
            Syntax::Markdown,
        );
        assert!(masked.contains("Head"));
        assert!(masked.contains("link text"));
        for gone in ["title", "code", "fenced", "u.rl"] {
            assert!(!masked.contains(gone), "{gone} leaked: {masked}");
        }
    }

    #[test]
    fn latex_skips_commands_math_comments() {
        let masked = visible(
            "\\section{Intro} Some \\textbf{bold} words $x^2$ and \\cite{key} here.\n% a comment\n\\begin{equation}\n y = mx\n\\end{equation}\nafter",
            Syntax::Latex,
        );
        for kept in ["Intro", "Some", "bold", "words", "here", "after"] {
            assert!(masked.contains(kept), "{kept} lost: {masked}");
        }
        for gone in [
            "section", "textbf", "x^2", "key", "comment", "mx", "equation",
        ] {
            assert!(!masked.contains(gone), "{gone} leaked: {masked}");
        }
    }

    #[test]
    fn urls_and_citation_keys_are_masked_everywhere() {
        let masked = visible(
            "see https://example.com/a?b=1 and mail me@x.org or [@smith2020] ok",
            Syntax::Plain,
        );
        assert!(masked.contains("see"));
        assert!(masked.contains("ok"));
        for gone in ["example", "me@x", "smith"] {
            assert!(!masked.contains(gone), "{gone} leaked: {masked}");
        }
    }

    #[test]
    fn ignore_regions_apply() {
        let masked = prose(
            "alpha beta gamma",
            Syntax::Plain,
            &[CharRange { start: 6, end: 10 }],
        );
        assert_eq!(masked.replace(OPAQUE, "_"), "alpha ____ gamma");
    }

    #[test]
    fn unbalanced_constructs_do_not_swallow_the_document() {
        let masked = visible("a $ b\n\nc \\begin{equation} d", Syntax::Latex);
        assert!(masked.contains('a'));
        assert!(masked.contains('b'));
        assert!(masked.contains('c'));
    }
}
