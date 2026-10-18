//! Pure text helpers: UTF-16 offsets (what CodeMirror counts), outlines, and the markup guard.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Local token estimate for the live meter: about 3.5 characters per token.
pub fn estimate_tokens(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    (chars * 2).div_ceil(7)
}

/// Length in UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset of a byte index (which must lie on a character boundary).
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text.get(..byte).map_or(0, utf16_len)
}

/// One heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    pub title: String,
    /// 1-based.
    pub line: usize,
}

/// Headings of a Typst, LaTeX or Markdown source, chosen by the file extension (Markdown when it
/// is unknown).
pub fn outline(path: &str, text: &str) -> Vec<Heading> {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "typ" => typst_outline(text),
        "tex" | "latex" => latex_outline(text),
        _ => markdown_outline(text),
    }
}

fn heading_marks(line: &str, mark: char) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    let n = trimmed.chars().take_while(|c| *c == mark).count();
    if n == 0 || n > 6 {
        return None;
    }
    let rest = trimmed.get(n..)?;
    let title = rest.strip_prefix(' ')?.trim();
    (!title.is_empty()).then(|| (u8::try_from(n).unwrap_or(6), title))
}

fn typst_outline(text: &str) -> Vec<Heading> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            heading_marks(line, '=').map(|(level, title)| Heading {
                level,
                title: title.to_owned(),
                line: i + 1,
            })
        })
        .collect()
}

fn markdown_outline(text: &str) -> Vec<Heading> {
    let mut fenced = false;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some((level, title)) = heading_marks(line, '#') {
            out.push(Heading {
                level,
                title: title.trim_end_matches('#').trim().to_owned(),
                line: i + 1,
            });
        }
    }
    out
}

fn latex_outline(text: &str) -> Vec<Heading> {
    const LEVELS: [(&str, u8); 6] = [
        ("\\part", 1),
        ("\\chapter", 1),
        ("\\section", 2),
        ("\\subsection", 3),
        ("\\subsubsection", 4),
        ("\\paragraph", 5),
    ];
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        // Longest command first so `\subsection` is not read as `\section`'s prefix.
        let found = LEVELS
            .iter()
            .filter(|(cmd, _)| {
                trimmed.strip_prefix(cmd).is_some_and(|rest| {
                    rest.starts_with('{') || rest.starts_with('*') || rest.starts_with('[')
                })
            })
            .max_by_key(|(cmd, _)| cmd.len());
        if let Some((cmd, level)) = found {
            let rest = &trimmed[cmd.len()..];
            if let (Some(open), Some(close)) = (rest.find('{'), rest.rfind('}'))
                && open < close
            {
                out.push(Heading {
                    level: *level,
                    title: rest[open + 1..close].to_owned(),
                    line: i + 1,
                });
            }
        }
    }
    out
}

/// What must survive a rewrite untouched: citation keys, references, labels, math delimiters
/// and brace balance.
#[derive(Debug, Default, PartialEq, Eq)]
struct Markup {
    keys: BTreeSet<String>,
    dollars: usize,
    braces: i64,
    /// `\begin{x}` minus `\end{x}` per environment name.
    envs: BTreeMap<String, i64>,
}

fn scan_markup(text: &str) -> Markup {
    let mut m = Markup::default();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                // `\cite{a,b}`, `\ref{x}`, `\label{x}` and friends: the command and its keys.
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_alphabetic() {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                if matches!(name.as_str(), "begin" | "end") && j < chars.len() && chars[j] == '{' {
                    let mut k = j + 1;
                    let mut env = String::new();
                    while k < chars.len() && chars[k] != '}' {
                        env.push(chars[k]);
                        k += 1;
                    }
                    *m.envs.entry(env).or_insert(0) += if name == "begin" { 1 } else { -1 };
                }
                if (name.contains("cite")
                    || matches!(
                        name.as_str(),
                        "ref" | "eqref" | "autoref" | "cref" | "Cref" | "label" | "pageref"
                    ))
                    && j < chars.len()
                    && chars[j] == '{'
                {
                    let mut k = j + 1;
                    let mut key = String::new();
                    while k < chars.len() && chars[k] != '}' {
                        key.push(chars[k]);
                        k += 1;
                    }
                    for part in key.split(',') {
                        let part = part.trim();
                        if !part.is_empty() {
                            m.keys.insert(format!("\\{name}:{part}"));
                        }
                    }
                }
                // An escaped character (`\$`, `\{`) is not a delimiter.
                if j == i + 1 {
                    i += 2;
                    continue;
                }
                i = j;
                continue;
            }
            '$' => m.dollars += 1,
            '{' => m.braces += 1,
            '}' => m.braces -= 1,
            '@' => {
                // Typst reference or Pandoc citation: `@key`.
                let mut k = i + 1;
                let mut key = String::new();
                while k < chars.len()
                    && (chars[k].is_alphanumeric() || matches!(chars[k], '_' | '-' | ':' | '.'))
                {
                    key.push(chars[k]);
                    k += 1;
                }
                let key = key.trim_end_matches('.').to_owned();
                if !key.is_empty() {
                    m.keys.insert(format!("@{key}"));
                }
            }
            '<' => {
                // Typst label `<sec:intro>`.
                let mut k = i + 1;
                let mut key = String::new();
                while k < chars.len()
                    && (chars[k].is_alphanumeric() || matches!(chars[k], '_' | '-' | ':' | '.'))
                {
                    key.push(chars[k]);
                    k += 1;
                }
                if !key.is_empty() && k < chars.len() && chars[k] == '>' {
                    m.keys.insert(format!("<{key}>"));
                }
            }
            _ => {}
        }
        i += 1;
    }
    m
}

/// The key inside a descriptor from `scan_markup` (`\cite:knuth84`, `@knuth84`, `<sec:intro>`).
fn bare_key(descriptor: &str) -> &str {
    if let Some(rest) = descriptor.strip_prefix('\\') {
        return rest.split_once(':').map_or(rest, |(_, key)| key);
    }
    descriptor
        .trim_start_matches('@')
        .trim_start_matches('<')
        .trim_end_matches('>')
}

/// Keys that `after` has and `before` does not, as `(bare key, is a citation)`. A citation is
/// `\cite..{key}` or Typst/Pandoc `@key`; labels and `\ref` are not.
pub fn introduced_keys(before: &str, after: &str) -> Vec<(String, bool)> {
    let b = scan_markup(before);
    let a = scan_markup(after);
    a.keys
        .difference(&b.keys)
        .map(|d| {
            let cite = d.starts_with('@') || (d.starts_with('\\') && d.contains("cite"));
            (bare_key(d).to_owned(), cite)
        })
        .collect()
}

/// Why `after` may not replace `before`, or `None` when the markup is intact. The edit must keep
/// every citation, reference and label (unless the author's message names the key, which means
/// they asked for the change), the parity of `$` signs, the brace balance and the balance of
/// every `\begin`/`\end` pair.
pub fn markup_problem_asked(before: &str, after: &str, asked: &str) -> Option<String> {
    let b = scan_markup(before);
    let a = scan_markup(after);
    let missing: Vec<&str> = b
        .keys
        .difference(&a.keys)
        .map(|d| bare_key(d))
        .filter(|key| asked.is_empty() || !asked.contains(*key))
        .collect();
    if !missing.is_empty() {
        let plural = missing.len() > 1;
        return Some(format!(
            "the edit drops or changes {} that must stay: {}. Keep {}, or ask the author first with ask_user",
            if plural { "references" } else { "a reference" },
            missing.join(", "),
            if plural { "them" } else { "it" },
        ));
    }
    if b.dollars % 2 != a.dollars % 2 || (b.dollars > 0) != (a.dollars > 0) {
        return Some("the edit changes the math delimiters ($)".to_owned());
    }
    if b.braces != a.braces {
        return Some("the edit unbalances { and }".to_owned());
    }
    let open_envs = |m: &Markup| -> BTreeMap<String, i64> {
        m.envs
            .iter()
            .filter(|(_, v)| **v != 0)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    };
    let (be, ae) = (open_envs(&b), open_envs(&a));
    if be != ae {
        let mut names: BTreeSet<&String> = be.keys().collect();
        names.extend(ae.keys());
        let list: Vec<&str> = names.into_iter().map(String::as_str).collect();
        return Some(format!(
            "the edit unbalances \\begin and \\end ({})",
            list.join(", ")
        ));
    }
    None
}

/// [`markup_problem_asked`] for an author who named nothing.
pub fn markup_problem(before: &str, after: &str) -> Option<String> {
    markup_problem_asked(before, after, "")
}

/// Clips `text` to at most `max` characters (not bytes), reporting whether it was cut.
pub fn clip_chars(text: &str, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((byte, _)) => (text[..byte].to_owned(), true),
        None => (text.to_owned(), false),
    }
}
