pub mod bibtex;
pub mod csl;

use crate::models::{Entry, ParseDiagnostic};
use std::path::Path;

pub use bibtex::{parse_bibtex, parse_bibtex_resilient};
pub use csl::{parse_csl_json, parse_csl_json_resilient};

/// Parses a bibliography string, auto-detecting BibTeX vs CSL-JSON format if not hinted.
pub fn parse_source(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') || (trimmed.starts_with('{') && !trimmed.starts_with('@')) {
        // Likely CSL-JSON
        parse_csl_json(source, file)
    } else {
        // Likely BibTeX / BibLaTeX
        parse_bibtex(source, file)
    }
}

/// Reads a file from disk and parses it into normalized entries with diagnostics.
pub fn parse_file(path: &Path) -> (Vec<Entry>, Vec<ParseDiagnostic>) {
    let file_str = path.to_string_lossy().to_string();
    let content = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                Vec::new(),
                vec![ParseDiagnostic {
                    file: Some(file_str),
                    line: 1,
                    column: 1,
                    message: format!("failed to read file `{}`: {err}", path.display()),
                }],
            );
        }
    };

    let text = match String::from_utf8(content) {
        Ok(s) => s,
        Err(err) => {
            return (
                Vec::new(),
                vec![ParseDiagnostic {
                    file: Some(file_str),
                    line: 1,
                    column: 1,
                    message: format!("file `{}` is not valid UTF-8: {err}", path.display()),
                }],
            );
        }
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "json" | "csl" => parse_csl_json_resilient(&text, Some(&file_str)),
        "bib" | "biblatex" => parse_bibtex_resilient(&text, Some(&file_str)),
        _ => {
            let trimmed = text.strip_prefix('\u{feff}').unwrap_or(&text).trim();
            if trimmed.starts_with('[') || (trimmed.starts_with('{') && !trimmed.starts_with('@')) {
                parse_csl_json_resilient(&text, Some(&file_str))
            } else {
                parse_bibtex_resilient(&text, Some(&file_str))
            }
        }
    }
}
