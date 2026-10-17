use crate::models::{Author, Entry, ParseDiagnostic};
use biblatex::{Bibliography, ChunksExt, DateValue, PermissiveType};
use std::collections::HashMap;

/// Parse a BibTeX or BibLaTeX source string into normalized entries.
pub fn parse_bibtex(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic> {
    let bib = Bibliography::parse(source).map_err(|err| {
        let offset = err.span.start.min(source.len());
        let line = source[..offset].chars().filter(|&c| c == '\n').count() + 1;
        let last_nl = source[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let column = source[last_nl..offset].chars().count() + 1;
        ParseDiagnostic {
            file: file.map(|s| s.to_string()),
            line,
            column,
            message: format!("BibTeX parse error: {err}"),
        }
    })?;

    let mut entries = Vec::new();

    for item in bib.iter() {
        let key = item.key.clone();
        let entry_type = format!("{:?}", item.entry_type).to_ascii_lowercase();

        // Extract title
        let title = item.title().ok().map(|chunks| chunks.format_verbatim());

        // Extract authors
        let mut authors = Vec::new();
        if let Ok(persons) = item.author() {
            for person in persons {
                let given = if person.given_name.is_empty() {
                    None
                } else {
                    Some(person.given_name.clone())
                };
                let family = if person.name.is_empty() {
                    None
                } else {
                    Some(person.name.clone())
                };
                let full_name = match (&given, &family) {
                    (Some(g), Some(f)) => format!("{g} {f}"),
                    (None, Some(f)) => f.clone(),
                    (Some(g), None) => g.clone(),
                    (None, None) => String::new(),
                };
                if !full_name.is_empty() {
                    authors.push(Author {
                        name: full_name,
                        family,
                        given,
                    });
                }
            }
        }

        // Extract raw fields map
        let mut raw_fields = HashMap::new();
        for (field_name, field_chunks) in &item.fields {
            raw_fields.insert(field_name.to_string(), field_chunks.format_verbatim());
        }

        // Extract year and month
        let mut year = None;
        let mut month = None;
        if let Ok(date) = item.date() {
            match date {
                PermissiveType::Typed(d) => {
                    let dt = match &d.value {
                        DateValue::At(dt) => Some(dt),
                        DateValue::After(dt) => Some(dt),
                        DateValue::Before(dt) => Some(dt),
                        DateValue::Between(dt, _) => Some(dt),
                    };
                    if let Some(dt) = dt {
                        if dt.year > 0 {
                            year = Some(dt.year as u32);
                        }
                        month = dt.month.map(|m| m as u32);
                    }
                }
                PermissiveType::Chunks(c) => {
                    let raw = c.format_verbatim();
                    for part in raw.split(|c: char| !c.is_ascii_digit()) {
                        if part.len() == 4 {
                            if let Ok(y) = part.parse::<u32>() {
                                year = Some(y);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Fallbacks for year from raw_fields if not found via date()
        if year.is_none() {
            if let Some(y_str) = raw_fields.get("year").or_else(|| raw_fields.get("date")) {
                for part in y_str.split(|c: char| !c.is_ascii_digit()) {
                    if part.len() == 4 {
                        if let Ok(y) = part.parse::<u32>() {
                            year = Some(y);
                            break;
                        }
                    }
                }
            }
        }

        // Fallback for month
        if month.is_none() {
            if let Some(m_str) = raw_fields.get("month") {
                if let Ok(m) = m_str.trim().parse::<u32>() {
                    month = Some(m);
                }
            }
        }

        // Container / journal or publisher
        let journal_or_publisher = raw_fields
            .get("journal")
            .or_else(|| raw_fields.get("journaltitle"))
            .or_else(|| raw_fields.get("booktitle"))
            .or_else(|| raw_fields.get("publisher"))
            .or_else(|| raw_fields.get("school"))
            .or_else(|| raw_fields.get("institution"))
            .cloned();

        let volume = raw_fields.get("volume").cloned();
        let number = raw_fields
            .get("number")
            .or_else(|| raw_fields.get("issue"))
            .cloned();
        let pages = raw_fields.get("pages").cloned();

        // DOI clean up
        let doi = raw_fields.get("doi").map(|d| {
            d.trim()
                .trim_start_matches("https://doi.org/")
                .trim_start_matches("http://doi.org/")
                .trim_start_matches("doi:")
                .to_string()
        });

        let url = raw_fields.get("url").cloned();

        // arXiv id from eprint or arxiv
        let arxiv_id = raw_fields
            .get("eprint")
            .filter(|_| {
                raw_fields
                    .get("eprinttype")
                    .map(|t| t.eq_ignore_ascii_case("arxiv"))
                    .unwrap_or(true)
            })
            .or_else(|| raw_fields.get("arxiv"))
            .map(|a| {
                a.trim()
                    .trim_start_matches("arXiv:")
                    .trim_start_matches("arxiv:")
                    .to_string()
            });

        let abstract_text = raw_fields.get("abstract").cloned();

        entries.push(Entry {
            key,
            entry_type,
            title,
            authors,
            year,
            month,
            journal_or_publisher,
            volume,
            number,
            pages,
            doi,
            url,
            arxiv_id,
            abstract_text,
            raw_fields,
        });
    }

    Ok(entries)
}
