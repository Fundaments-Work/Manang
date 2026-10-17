//! The markup languages the tools understand.

use serde::{Deserialize, Serialize};

/// Source syntax of a text. Decides what counts as prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Syntax {
    Typst,
    Latex,
    Markdown,
    /// No markup at all; only URLs and citation keys are skipped.
    Plain,
    /// Syntax-agnostic scan: math, code spans, `\commands` and `#calls` are
    /// skipped. Used when the syntax is unknown and cannot be detected.
    Generic,
    /// Detect the syntax from the text itself.
    Auto,
}

impl Syntax {
    /// Parse a user-facing syntax name or file extension.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "typst" | "typ" => Some(Self::Typst),
            "latex" | "tex" => Some(Self::Latex),
            "markdown" | "md" => Some(Self::Markdown),
            "plain" | "text" | "txt" => Some(Self::Plain),
            "generic" => Some(Self::Generic),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Replace [`Syntax::Auto`] by a concrete syntax guessed from `text`.
    pub fn resolve(self, text: &str) -> Self {
        if self == Self::Auto {
            detect(text)
        } else {
            self
        }
    }
}

fn detect(text: &str) -> Syntax {
    const LATEX: [&str; 9] = [
        "\\documentclass",
        "\\usepackage",
        "\\begin{",
        "\\end{",
        "\\section{",
        "\\subsection{",
        "\\cite{",
        "\\textbf{",
        "\\emph{",
    ];
    if LATEX.iter().any(|m| text.contains(m)) {
        return Syntax::Latex;
    }
    const TYPST_HASH: [&str; 12] = [
        "#import",
        "#set ",
        "#show ",
        "#let ",
        "#figure(",
        "#table(",
        "#image(",
        "#cite(",
        "#link(",
        "#text(",
        "#pagebreak",
        "#include",
    ];
    let mut markdown = false;
    for line in text.lines() {
        let line = line.trim_start();
        if TYPST_HASH.iter().any(|m| line.starts_with(m)) {
            return Syntax::Typst;
        }
        let eqs = line.bytes().take_while(|&b| b == b'=').count();
        if (1..=6).contains(&eqs) && line.as_bytes().get(eqs) == Some(&b' ') {
            return Syntax::Typst;
        }
        let hashes = line.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
            markdown = true;
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            markdown = true;
        }
    }
    if markdown || text.contains("](") {
        Syntax::Markdown
    } else {
        Syntax::Generic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names() {
        assert_eq!(Syntax::parse("Typst"), Some(Syntax::Typst));
        assert_eq!(Syntax::parse("tex"), Some(Syntax::Latex));
        assert_eq!(Syntax::parse("md"), Some(Syntax::Markdown));
        assert_eq!(Syntax::parse("klingon"), None);
    }

    #[test]
    fn detects() {
        assert_eq!(Syntax::Auto.resolve("\\section{A}"), Syntax::Latex);
        assert_eq!(Syntax::Auto.resolve("= Title\ntext"), Syntax::Typst);
        assert_eq!(Syntax::Auto.resolve("#set page(width: 1cm)"), Syntax::Typst);
        assert_eq!(Syntax::Auto.resolve("# Title\n\ntext"), Syntax::Markdown);
        assert_eq!(Syntax::Auto.resolve("just words"), Syntax::Generic);
        assert_eq!(Syntax::Typst.resolve("\\begin{x}"), Syntax::Typst);
    }
}
