//! Structured diagnostics from TeX and BibTeX logs.
//!
//! The parsers are total: any text, however malformed, yields a (possibly
//! empty) list and never panics. They work on the engine's own log rather
//! than on API callbacks so the same code handles Tectonic's XeTeX logs and
//! the pdfTeX-style logs users paste from elsewhere.

use serde::{Deserialize, Serialize};

/// TeX wraps log lines at this many characters.
const WRAP_WIDTH: usize = 79;
/// Stop collecting after this many diagnostics; a runaway log is not useful.
const MAX_DIAGNOSTICS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Project-relative file the engine was reading when it reported this.
    pub file: Option<String>,
    /// 1-based line in `file`.
    pub line: Option<u32>,
    /// A suggested fix, when the message is one we recognise.
    pub hint: Option<String>,
}

impl Diagnostic {
    fn new(severity: Severity, message: String) -> Self {
        Self {
            severity,
            message,
            file: None,
            line: None,
            hint: None,
        }
    }
}

/// Undo TeX's hard wrapping: a line of exactly [`WRAP_WIDTH`] characters
/// continues on the next one.
pub(crate) fn unwrap_lines(log: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut continuing = false;
    for raw in log.lines() {
        let line = raw.trim_end_matches('\r');
        if continuing && !line.is_empty() {
            if let Some(last) = out.last_mut() {
                last.push_str(line);
            }
        } else {
            out.push(line.to_owned());
        }
        continuing = line.chars().count() == WRAP_WIDTH;
    }
    out
}

fn is_tex_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '/' | '_' | '-' | '+' | '~' | ':' | '\\')
}

/// The files TeX currently has open, innermost last.
struct FileStack {
    entries: Vec<Option<String>>,
    main: String,
}

impl FileStack {
    fn new(main: &str) -> Self {
        Self {
            entries: Vec::new(),
            main: main.to_owned(),
        }
    }

    /// Feed one logical log line, tracking `(file` opens and `)` closes.
    fn scan(&mut self, line: &str) {
        let chars = line.char_indices();
        for (i, c) in chars {
            match c {
                '(' => {
                    let rest = &line[i + 1..];
                    let end = rest
                        .find(|ch: char| !is_tex_name_char(ch))
                        .unwrap_or(rest.len());
                    let token = &rest[..end];
                    let closed_at_once = rest[end..].starts_with(')');
                    let has_ext = token.contains('.') || token.contains('/');
                    let plausible = !token.is_empty() && (has_ext || !closed_at_once);
                    self.entries
                        .push(plausible.then(|| token.trim_start_matches("./").to_owned()));
                }
                ')' => {
                    self.entries.pop();
                }
                _ => {}
            }
        }
    }

    fn current(&self) -> String {
        self.entries
            .iter()
            .rev()
            .find_map(|e| e.clone())
            .unwrap_or_else(|| self.main.clone())
    }
}

/// Parse a TeX log. `main` is the name reported for diagnostics raised
/// outside any `(file` group.
pub fn parse_tex_log(log: &str, main: &str) -> Vec<Diagnostic> {
    let lines = unwrap_lines(log);
    let mut stack = FileStack::new(main);
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut i = 0;
    while i < lines.len() && out.len() < MAX_DIAGNOSTICS {
        let line = lines[i].as_str();
        let consumed = if let Some(rest) = line.strip_prefix("! ") {
            Some(parse_error_block(&lines, i, rest, &stack, &mut out))
        } else if let Some((d, consumed)) = parse_file_line_error(&lines, i) {
            out.push(d);
            Some(consumed)
        } else if line.starts_with("Overfull \\") || line.starts_with("Underfull \\") {
            Some(parse_box_block(&lines, i, &stack, &mut out))
        } else if is_warning_start(line) {
            Some(parse_warning_block(&lines, i, &stack, &mut out))
        } else if line.starts_with("Missing character:") {
            let mut d = Diagnostic::new(Severity::Warning, line.to_owned());
            d.file = Some(stack.current());
            out.push(d);
            Some(1)
        } else if line.starts_with("Emergency stop") || line.starts_with("==> Fatal error") {
            out.push(Diagnostic::new(Severity::Error, line.to_owned()));
            Some(1)
        } else {
            None
        };
        match consumed {
            Some(n) => i += n.max(1),
            None => {
                stack.scan(line);
                i += 1;
            }
        }
    }
    dedupe(out)
}

fn dedupe(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.dedup();
    diagnostics
}

/// `! message` followed by context lines, the offending `l.N` line and help.
/// Returns how many lines were consumed.
fn parse_error_block(
    lines: &[String],
    start: usize,
    first: &str,
    stack: &FileStack,
    out: &mut Vec<Diagnostic>,
) -> usize {
    // Tectonic prints `! ! LaTeX Error:`; normalise repeated bangs.
    let mut message = first;
    while let Some(rest) = message.strip_prefix("! ") {
        message = rest;
    }
    let message = message.trim().trim_end_matches(['.', ' ']).to_owned();
    let mut diagnostic = Diagnostic::new(Severity::Error, message.clone());
    diagnostic.file = Some(stack.current());

    let mut consumed = 1;
    let mut context_line = None;
    let mut last_token = None;
    let window = &lines[start + 1..lines.len().min(start + 1 + 14)];
    for (offset, line) in window.iter().enumerate() {
        if (line.is_empty() && offset > 0) || line.starts_with("! ") {
            break;
        }
        if let Some(rest) = line.strip_prefix("l.") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(n) = digits.parse::<u32>() {
                context_line = Some(n);
                last_token = rest[digits.len()..]
                    .split_whitespace()
                    .last()
                    .map(str::to_owned);
                // The remainder of the source line follows.
                consumed = offset + 3;
                break;
            }
        }
        consumed = offset + 2;
    }
    consumed = consumed.min(lines.len() - start);
    diagnostic.line = context_line;
    diagnostic.hint = error_hint(&message, last_token.as_deref());
    out.push(diagnostic);
    consumed
}

/// `path.tex:12: message`, as printed with `-file-line-error`.
fn parse_file_line_error(lines: &[String], at: usize) -> Option<(Diagnostic, usize)> {
    let line = lines.get(at)?.as_str();
    let (path, rest) = line.split_once(':')?;
    let (number, message) = rest.split_once(':')?;
    let n: u32 = number.parse().ok()?;
    let looks_like_file = path.ends_with(".tex")
        || path.ends_with(".sty")
        || path.ends_with(".cls")
        || path.ends_with(".ltx");
    if !looks_like_file || path.contains(' ') {
        return None;
    }
    let mut message = message.trim().trim_end_matches(['.', ' ']).to_owned();
    // `l.14 \includegraphics` follows: the text up to the error, whose last
    // word is the offending token; the rest of the source line comes next.
    let mut token = None;
    let mut consumed = 1;
    if let Some(context) = lines.get(at + 1).and_then(|l| l.strip_prefix("l."))
        && context.starts_with(|c: char| c.is_ascii_digit())
    {
        let digits: String = context.chars().take_while(char::is_ascii_digit).collect();
        token = context[digits.len()..]
            .split_whitespace()
            .last()
            .map(str::to_owned);
        consumed = 2;
        if lines
            .get(at + 2)
            .is_some_and(|l| l.starts_with(' ') && !l.trim().is_empty())
        {
            consumed = 3;
        }
    }
    let token = token.filter(|t| t.starts_with('\\'));
    if message == "Undefined control sequence"
        && let Some(t) = token.as_deref()
    {
        message = format!("{message}: {t}");
    }
    let mut d = Diagnostic::new(Severity::Error, message.clone());
    d.file = Some(path.trim_start_matches("./").to_owned());
    d.line = Some(n);
    d.hint = error_hint(&message, token.as_deref());
    Some((d, consumed))
}

fn error_hint(message: &str, token: Option<&str>) -> Option<String> {
    if message.starts_with("Undefined control sequence") {
        return Some(match token.filter(|t| t.starts_with('\\')) {
            Some(t) => match package_for(t) {
                Some(package) => format!(
                    "`{t}` comes from the `{package}` package: add `\\usepackage{{{package}}}` \
                     to the preamble"
                ),
                None => format!(
                    "`{t}` is not defined: check the spelling or load the package that provides it"
                ),
            },
            None => "check the spelling or load the package that provides this command".to_owned(),
        });
    }
    if let Some(name) = message
        .strip_prefix("LaTeX Error: Environment ")
        .and_then(|r| r.strip_suffix(" undefined"))
        && let Some(package) = package_for_environment(name)
    {
        return Some(format!(
            "the `{name}` environment comes from the `{package}` package: add \
             `\\usepackage{{{package}}}` to the preamble"
        ));
    }
    if message.contains("not found") && (message.contains("File `") || message.contains("File '")) {
        return Some(
            "check the file name and path; a missing package may need the TeX bundle to be \
             downloaded once (enable network access)"
                .to_owned(),
        );
    }
    if message.starts_with("Missing $ inserted") {
        return Some(
            "a math-only command (`^`, `_`, `\\frac`, ...) was used outside math mode; \
             wrap it in `$...$`"
                .to_owned(),
        );
    }
    if message.starts_with("File ended while scanning") || message.starts_with("Runaway argument") {
        return Some("a `{` is never closed; look for a missing `}` above this point".to_owned());
    }
    if message.starts_with("Missing } inserted") || message.starts_with("Missing { inserted") {
        return Some("braces are unbalanced near here".to_owned());
    }
    if message.starts_with("Extra }") || message.contains("Too many }'s") {
        return Some("there is a `}` without a matching `{`".to_owned());
    }
    if message.contains("\\begin{") && message.contains("ended by") {
        return Some("an environment is closed with the wrong `\\end{...}`".to_owned());
    }
    None
}

/// The package that defines a commonly used command, for the "undefined
/// control sequence" hint. Only unambiguous, widely used ones.
fn package_for(command: &str) -> Option<&'static str> {
    Some(match command {
        "\\includegraphics" | "\\graphicspath" | "\\rotatebox" | "\\scalebox" | "\\resizebox" => {
            "graphicx"
        }
        "\\textcolor" | "\\color" | "\\definecolor" | "\\colorbox" | "\\fcolorbox" => "xcolor",
        "\\url" | "\\href" | "\\hyperref" | "\\autoref" | "\\nameref" => "hyperref",
        "\\mathbb" | "\\mathfrak" | "\\checkmark" | "\\leqslant" | "\\geqslant" => "amssymb",
        "\\text" | "\\tag" | "\\eqref" | "\\dfrac" | "\\tfrac" | "\\binom" | "\\boldsymbol"
        | "\\substack" | "\\intertext" | "\\operatorname" => "amsmath",
        "\\toprule" | "\\midrule" | "\\bottomrule" | "\\cmidrule" => "booktabs",
        "\\multirow" => "multirow",
        "\\SI" | "\\si" | "\\num" | "\\qty" | "\\unit" => "siunitx",
        "\\lstinline" | "\\lstlisting" | "\\lstset" => "listings",
        "\\subfloat" | "\\subcaption" | "\\captionof" => "subcaption",
        "\\cref" | "\\Cref" | "\\crefrange" => "cleveref",
        "\\citep" | "\\citet" | "\\citeauthor" | "\\citeyear" | "\\citealp" => "natbib",
        "\\bm" => "bm",
        "\\tikz" | "\\draw" | "\\node" | "\\fill" | "\\path" => "tikz",
        "\\includepdf" => "pdfpages",
        "\\FloatBarrier" => "placeins",
        "\\xspace" => "xspace",
        "\\cancel" | "\\bcancel" => "cancel",
        "\\hl" => "soul",
        "\\mathscr" => "mathrsfs",
        "\\mathbbm" => "bbm",
        "\\csvreader" => "csvsimple",
        "\\textbackslash" | "\\textless" => return None,
        _ => return None,
    })
}

fn package_for_environment(name: &str) -> Option<&'static str> {
    Some(
        match name.trim_matches(|c| c == '`' || c == '\'' || c == ' ') {
            "align" | "align*" | "equation*" | "gather" | "gather*" | "multline" | "multline*"
            | "split" | "cases" | "aligned" | "alignat" | "flalign" | "matrix" | "pmatrix"
            | "bmatrix" | "vmatrix" => "amsmath",
            "tikzpicture" => "tikz",
            "lstlisting" => "listings",
            "subfigure" | "subtable" => "subcaption",
            "algorithm" | "algorithmic" => "algorithm2e",
            "tabularx" => "tabularx",
            "longtable" => "longtable",
            "multicols" => "multicol",
            "theorem" | "lemma" | "proof" => "amsthm",
            _ => return None,
        },
    )
}

fn is_warning_start(line: &str) -> bool {
    line.starts_with("LaTeX Warning:")
        || line.starts_with("LaTeX Font Warning:")
        || line.starts_with("pdfTeX warning")
        || line.starts_with("xdvipdfmx:warning")
        || (line.starts_with("Package ") && line.contains(" Warning:"))
        || (line.starts_with("Class ") && line.contains(" Warning:"))
}

/// Pull `on input line N` (or `at line N`) out of a joined message.
fn line_in(text: &str) -> Option<u32> {
    for marker in ["on input line ", "at line ", "at lines "] {
        if let Some(pos) = text.rfind(marker) {
            let digits: String = text[pos + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(n) = digits.parse() {
                return Some(n);
            }
        }
    }
    None
}

fn parse_warning_block(
    lines: &[String],
    start: usize,
    stack: &FileStack,
    out: &mut Vec<Diagnostic>,
) -> usize {
    let mut text = lines[start].clone();
    let mut consumed = 1;
    for line in lines.iter().skip(start + 1).take(12) {
        if line.is_empty() {
            break;
        }
        // Continuations look like `(pkg)      more text`.
        let trimmed = line.trim_start();
        let body = match trimmed.strip_prefix('(').and_then(|r| r.split_once(')')) {
            Some((_, after)) => after.trim_start(),
            None => trimmed,
        };
        text.push(' ');
        text.push_str(body);
        consumed += 1;
        if line_in(&text).is_some() {
            break;
        }
    }
    let message = text.trim().to_owned();
    let mut d = Diagnostic::new(Severity::Warning, message.clone());
    d.file = Some(stack.current());
    d.line = line_in(&message);
    d.hint = warning_hint(&message);
    out.push(d);
    consumed
}

fn parse_box_block(
    lines: &[String],
    start: usize,
    stack: &FileStack,
    out: &mut Vec<Diagnostic>,
) -> usize {
    let first = lines[start].as_str();
    let mut d = Diagnostic::new(Severity::Warning, first.trim().to_owned());
    d.file = Some(stack.current());
    d.line = line_in(first);
    if first.starts_with("Overfull") {
        d.hint = Some(
            "the content is wider than the line: rephrase, allow hyphenation, or use \
             `\\sloppy`"
                .to_owned(),
        );
    }
    out.push(d);
    // Swallow the box dump up to the blank line so its parentheses don't
    // disturb the file stack.
    let mut consumed = 1;
    for line in lines.iter().skip(start + 1).take(40) {
        if line.is_empty() {
            break;
        }
        consumed += 1;
    }
    consumed
}

fn warning_hint(message: &str) -> Option<String> {
    if message.contains("Reference `") && message.contains("undefined") {
        return Some("the label does not exist yet; check its name, then compile again".to_owned());
    }
    if message.contains("Citation `") && message.contains("undefined") {
        return Some("check the citation key and that the .bib file defines it".to_owned());
    }
    if message.contains("Label `") && message.contains("multiply defined") {
        return Some("two `\\label`s share one name; rename one".to_owned());
    }
    if message.contains("Rerun to get") {
        return Some("compile once more so cross-references settle".to_owned());
    }
    None
}

/// Parse a BibTeX `.blg` log. `aux` names the file BibTeX read, used when the
/// log gives no better location.
pub fn parse_bibtex_log(log: &str, aux: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    for raw in log.lines() {
        if out.len() >= MAX_DIAGNOSTICS {
            break;
        }
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("Warning--") {
            let mut d = Diagnostic::new(Severity::Warning, rest.trim().to_owned());
            d.file = Some(aux.to_owned());
            if rest.contains("didn't find a database entry") {
                d.hint =
                    Some("check the citation key and that the .bib file defines it".to_owned());
            }
            out.push(d);
        } else if let Some(rest) = line.strip_prefix("---line ") {
            // Location detail for the error printed just before.
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let (Some(last), Ok(n)) = (out.last_mut(), digits.parse::<u32>())
                && last.severity == Severity::Error
                && last.line.is_none()
            {
                last.line = Some(n);
                if let Some((_, file)) = rest.split_once("of file ") {
                    last.file = Some(file.trim().to_owned());
                }
            }
        } else if line.starts_with("I couldn't open")
            || line.starts_with("I found no")
            || line.starts_with("I was expecting")
            || line.starts_with("Illegal")
            || line.starts_with("Sorry---you've exceeded")
        {
            let (message, location) = match line.split_once("---while reading file ") {
                Some((m, f)) => (m.trim().to_owned(), Some(f.trim().to_owned())),
                None => (line.to_owned(), None),
            };
            let mut d = Diagnostic::new(Severity::Error, message.clone());
            d.file = Some(location.unwrap_or_else(|| aux.to_owned()));
            d.hint = bibtex_hint(&message);
            out.push(d);
        }
    }
    dedupe(out)
}

fn bibtex_hint(message: &str) -> Option<String> {
    if message.contains("database file") {
        return Some(
            "create the .bib file next to your document or fix the `\\bibliography{...}` name"
                .to_owned(),
        );
    }
    if message.contains("style file") {
        return Some("check the `\\bibliographystyle{...}` name".to_owned());
    }
    if message.starts_with("I found no database files") {
        return Some("add a `\\bibliography{...}` command pointing at your .bib file".to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    /// The user's own project: `\includegraphics` without `graphicx`, in the
    /// `-file-line-error` form the plugin runs TeX with.
    const MISSING_GRAPHICX: &str = "(./main.tex\n\
./main.tex:14: Undefined control sequence.\n\
l.14 \\includegraphics\n\
                     [width=0.8\\linewidth]{figures/untitled.pdf}\n\
\n\
./main.tex:14: Missing number, treated as zero.\n\
l.14 \\includegraphics[\n\
                      width=0.8\\linewidth]{figures/untitled.pdf}\n";

    #[test]
    fn an_undefined_command_is_named_and_its_package_suggested() {
        let found = parse_tex_log(MISSING_GRAPHICX, "main.tex");
        let first = &found[0];
        assert_eq!(
            first.message,
            "Undefined control sequence: \\includegraphics"
        );
        assert_eq!(first.file.as_deref(), Some("main.tex"));
        assert_eq!(first.line, Some(14));
        let hint = first.hint.as_deref().unwrap_or_default();
        assert!(hint.contains("graphicx"), "{hint}");
        assert!(hint.contains("\\usepackage{graphicx}"), "{hint}");
        // The context lines are consumed, not reported as errors of their own.
        assert!(found.iter().all(|d| !d.message.starts_with("l.14")));
    }

    #[test]
    fn an_unknown_command_gets_the_generic_hint() {
        let log = "./a.tex:3: Undefined control sequence.\nl.3 \\frobnicate\n\n";
        let found = parse_tex_log(log, "a.tex");
        assert_eq!(found[0].message, "Undefined control sequence: \\frobnicate");
        assert!(
            found[0]
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("check the spelling")
        );
    }

    #[test]
    fn a_missing_environment_package_is_suggested() {
        let log = "! LaTeX Error: Environment align undefined.\n\nSee the LaTeX manual.\nl.5 \\begin{align}\n";
        let found = parse_tex_log(log, "a.tex");
        let hint = found[0].hint.as_deref().unwrap_or_default();
        assert!(hint.contains("amsmath"), "{hint}");
    }

    use super::*;

    #[test]
    fn empty_and_garbage_input_yield_nothing() {
        assert!(parse_tex_log("", "main.tex").is_empty());
        assert!(parse_tex_log("((((\n))))\n", "main.tex").is_empty());
        assert!(parse_bibtex_log("", "main.aux").is_empty());
    }

    #[test]
    fn unwraps_79_column_lines() {
        let a = "x".repeat(79);
        let log = format!("{a}\nrest\n\nnext");
        assert_eq!(
            unwrap_lines(&log),
            vec![format!("{a}rest"), String::new(), "next".to_owned()]
        );
    }

    #[test]
    fn file_stack_tracks_nesting() {
        let mut s = FileStack::new("main.tex");
        assert_eq!(s.current(), "main.tex");
        s.scan("(./main.tex (./chap/one.tex");
        assert_eq!(s.current(), "chap/one.tex");
        s.scan(")");
        assert_eq!(s.current(), "main.tex");
        s.scan("(Font) )");
        assert_eq!(s.current(), "main.tex");
    }
}
