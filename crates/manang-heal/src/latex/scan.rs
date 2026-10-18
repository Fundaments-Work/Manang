//! A light scanner over LaTeX source: comments and verbatim blanked,
//! commands tokenised, the preamble summarised. Offsets are bytes of the
//! original text (blanking keeps every offset valid).

use std::collections::BTreeSet;

use crate::text::{blank, into_string};

/// Environments whose content is not LaTeX.
const VERBATIM: &[&str] = &[
    "verbatim",
    "verbatim*",
    "Verbatim",
    "lstlisting",
    "minted",
    "comment",
    "filecontents",
    "filecontents*",
];

/// The text with comments, verbatim content and `\verb` bodies blanked.
pub fn mask(text: &str) -> String {
    let src = text.as_bytes();
    let mut bytes = src.to_vec();
    let mut i = 0;
    while i < src.len() {
        match src[i] {
            b'\\' => {
                if src[i + 1..].starts_with(b"verb")
                    && !src.get(i + 5).is_some_and(u8::is_ascii_alphabetic)
                {
                    // `\verb*|text|` or `\verb|text|`.
                    let mut j = i + 5;
                    if src.get(j) == Some(&b'*') {
                        j += 1;
                    }
                    if let Some(&delim) = src.get(j)
                        && delim != b'\n'
                        && delim != b' '
                    {
                        let body = j + 1;
                        let end = src[body..]
                            .iter()
                            .position(|b| *b == delim || *b == b'\n')
                            .map_or(src.len(), |p| body + p);
                        blank(&mut bytes, body, end);
                        i = end + 1;
                        continue;
                    }
                }
                // A backslash escapes the next byte (`\%`, `\\`, `\{`).
                i += 2;
            }
            b'%' => {
                let end = src[i..]
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(src.len(), |p| i + p);
                blank(&mut bytes, i, end);
                i = end;
            }
            _ => i += 1,
        }
    }
    let masked = into_string(bytes);
    blank_verbatim(masked)
}

fn blank_verbatim(masked: String) -> String {
    let mut bytes = masked.into_bytes();
    for name in VERBATIM {
        let open = format!("\\begin{{{name}}}");
        let close = format!("\\end{{{name}}}");
        let mut from = 0;
        loop {
            let hay = into_str(&bytes);
            let Some(start) = hay[from..].find(&open).map(|p| from + p) else {
                break;
            };
            let body = start + open.len();
            let end = hay[body..].find(&close).map_or(bytes.len(), |p| body + p);
            blank(&mut bytes, body, end);
            from = end.min(bytes.len());
            if from >= bytes.len() {
                break;
            }
        }
    }
    into_string(bytes)
}

fn into_str(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}

/// `(start, name)` of every control word (`\usepackage`) in `masked`.
pub fn commands(masked: &str) -> Vec<(usize, &str)> {
    let bytes = masked.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let name_start = i + 1;
        let mut j = name_start;
        while j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'@') {
            j += 1;
        }
        if j > name_start {
            out.push((i, &masked[i..j]));
            i = j;
        } else {
            // `\\`, `\%`, `\{`: skip the escaped byte (it may be multi-byte).
            i = name_start
                + masked[name_start..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
        }
    }
    out
}

/// The `[...]` and `{...}` arguments after a command at `pos` (just past
/// its name), skipping spaces and at most one line break. Ranges exclude the
/// delimiters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub optional: Option<(usize, usize)>,
    pub required: Option<(usize, usize)>,
    /// Byte just past the last argument read.
    pub end: usize,
}

pub fn args(masked: &str, pos: usize) -> Args {
    let bytes = masked.as_bytes();
    let mut out = Args {
        end: pos,
        ..Args::default()
    };
    let mut i = skip_space(bytes, pos);
    if bytes.get(i) == Some(&b'[')
        && let Some(close) = matching(bytes, i, b'[', b']')
    {
        out.optional = Some((i + 1, close));
        out.end = close + 1;
        i = skip_space(bytes, close + 1);
    }
    if bytes.get(i) == Some(&b'{')
        && let Some(close) = matching(bytes, i, b'{', b'}')
    {
        out.required = Some((i + 1, close));
        out.end = close + 1;
    }
    out
}

fn skip_space(bytes: &[u8], mut i: usize) -> usize {
    let mut breaks = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                breaks += 1;
                if breaks > 1 {
                    break;
                }
                i += 1;
            }
            _ => break,
        }
    }
    i
}

fn matching(bytes: &[u8], open_at: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open_at;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b if b == open => depth += 1,
            b if b == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// What the preamble already does.
#[derive(Debug, Default)]
pub struct Preamble {
    /// Loaded package names (including those a class loads).
    pub packages: BTreeSet<String>,
    pub class: Option<String>,
    /// Byte just past `\documentclass[..]{..}`.
    pub class_end: Option<usize>,
    /// Byte just past the last `\usepackage`/`\RequirePackage` before the
    /// body.
    pub last_package_end: Option<usize>,
    /// TikZ libraries already named.
    pub tikz_libraries: BTreeSet<String>,
    /// Byte just past the last `\usetikzlibrary{..}`.
    pub last_library_end: Option<usize>,
    /// Byte of `\begin{document}`, when present.
    pub begin_document: Option<usize>,
    /// Names defined in the document (`\newcommand{\x}`, `\def\x`).
    pub defined: BTreeSet<String>,
}

impl Preamble {
    pub fn scan(masked: &str) -> Self {
        let mut out = Self::default();
        let cmds = commands(masked);
        out.begin_document = cmds.iter().find_map(|(at, name)| {
            (*name == "\\begin" && begins(masked, *at, "document")).then_some(*at)
        });
        let limit = out.begin_document.unwrap_or(masked.len());
        for (at, name) in &cmds {
            let after = at + name.len();
            match *name {
                "\\documentclass" if out.class.is_none() => {
                    let a = args(masked, after);
                    if let Some((s, e)) = a.required {
                        let class = masked[s..e].trim().to_owned();
                        for p in super::tables::class_loads(&class) {
                            out.packages.insert((*p).to_owned());
                        }
                        out.class = Some(class);
                        out.class_end = Some(a.end);
                    }
                }
                "\\usepackage" | "\\RequirePackage" if *at < limit => {
                    let a = args(masked, after);
                    if let Some((s, e)) = a.required {
                        for p in masked[s..e].split(',') {
                            out.packages.insert(p.trim().to_owned());
                        }
                        out.last_package_end = Some(a.end);
                    }
                }
                "\\usetikzlibrary" => {
                    let a = args(masked, after);
                    if let Some((s, e)) = a.required {
                        for p in masked[s..e].split(',') {
                            out.tikz_libraries.insert(p.trim().to_owned());
                        }
                        out.last_library_end = Some(a.end);
                    }
                }
                "\\newcommand"
                | "\\renewcommand"
                | "\\providecommand"
                | "\\DeclareRobustCommand"
                | "\\def"
                | "\\let"
                | "\\DeclareMathOperator"
                | "\\newcommand*"
                | "\\renewcommand*" => {
                    if let Some(defined) = defined_name(masked, after) {
                        out.defined.insert(defined);
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn loaded(&self, package: &str) -> bool {
        self.packages.contains(package)
    }

    /// Where a new `\usepackage` line goes: right after the last package, else
    /// after `\documentclass`. `None` for a file with neither (an included
    /// chapter: the fix belongs in the main file).
    pub fn package_insertion(&self) -> Option<usize> {
        self.last_package_end.or(self.class_end)
    }
}

/// `\begin{name}` at `at`?
pub fn begins(masked: &str, at: usize, name: &str) -> bool {
    let a = args(masked, at + "\\begin".len());
    a.required.is_some_and(|(s, e)| masked[s..e].trim() == name)
}

/// The command a `\newcommand`-like definition at `pos` names: `{\x}` or `\x`.
fn defined_name(masked: &str, pos: usize) -> Option<String> {
    let bytes = masked.as_bytes();
    let mut i = skip_space(bytes, pos);
    if bytes.get(i) == Some(&b'{') {
        i = skip_space(bytes, i + 1);
    }
    if bytes.get(i) != Some(&b'\\') {
        return None;
    }
    let start = i;
    i += 1;
    while bytes.get(i).is_some_and(|b| b.is_ascii_alphabetic()) {
        i += 1;
    }
    (i > start + 1).then(|| masked[start..i].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_keeps_offsets_and_hides_comments_and_verbatim() {
        let text =
            "a % \\usepackage{x}\n\\verb|\\url| \\begin{verbatim}\\foo\\end{verbatim} 100\\% é\n";
        let m = mask(text);
        assert_eq!(m.len(), text.len());
        assert!(!m.contains("usepackage"));
        assert!(!m.contains("\\url"));
        assert!(!m.contains("\\foo"));
        assert!(m.contains("100\\%"));
        assert!(m.contains("\\begin{verbatim}"));
    }

    #[test]
    fn preamble_summary() {
        let text = "\\documentclass[a4paper]{amsart}\n\\usepackage{tikz,\n  xcolor}\n\\usepackage[x]{graphicx}\n\\newcommand{\\url}{u}\n\\usetikzlibrary{calc}\n\\begin{document}\n\\usepackage{late}\n\\end{document}\n";
        let m = mask(text);
        let p = Preamble::scan(&m);
        assert!(p.loaded("tikz") && p.loaded("xcolor") && p.loaded("graphicx"));
        assert!(p.loaded("amsmath"), "amsart loads amsmath");
        assert!(
            !p.loaded("late"),
            "after \\begin{{document}} is not the preamble"
        );
        assert!(p.defined.contains("\\url"));
        assert!(p.tikz_libraries.contains("calc"));
        let end = p.package_insertion().expect("insertion");
        assert_eq!(
            &text[..end].lines().last(),
            &Some("\\usepackage[x]{graphicx}")
        );
        assert!(p.begin_document.is_some());
    }

    #[test]
    fn included_file_has_no_insertion_point() {
        let m = mask("\\section{A}\nbody\n");
        assert_eq!(Preamble::scan(&m).package_insertion(), None);
    }
}
