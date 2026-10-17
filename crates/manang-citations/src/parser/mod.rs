pub mod bibtex;
pub mod csl;

use crate::models::{Entry, ParseDiagnostic};
use std::path::Path;

pub use bibtex::parse_bibtex;
pub use csl::parse_csl_json;

/// Parses a bibliography string, auto-detecting BibTeX vs CSL-JSON format if not hinted.
pub fn parse_source(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic> {
    let trimmed = source.trim();
    if trimmed.starts_with('[') || trimmed.starts_with('{') && !trimmed.starts_with("@") {
        // Likely CSL-JSON
        parse_csl_json(source, file)
    } else {
        // Likely BibTeX / BibLaTeX
        parse_bibtex(source, file)
    }
}

/// Reads a file from disk and parses it into normalized entries.
pub fn parse_file(path: &Path) -> Result<Vec<Entry>, ParseDiagnostic> {
    let file_str = path.to_string_lossy().to_string();
    let content = std::fs::read_to_string(path).map_err(|err| ParseDiagnostic {
        file: Some(file_str.clone()),
        line: 1,
        column: 1,
        message: format!("failed to read file `{}`: {err}", path.display()),
    })?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "json" | "csl" => parse_csl_json(&content, Some(&file_str)),
        "bib" | "biblatex" => parse_bibtex(&content, Some(&file_str)),
        _ => parse_source(&content, Some(&file_str)),
    }
}
