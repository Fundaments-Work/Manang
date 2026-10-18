use crate::models::{Author, Entry, ParseDiagnostic};
use biblatex::{Bibliography, ChunksExt, DateValue, PermissiveType};
use std::collections::HashMap;

const DEFAULT_MACROS: &str = "\
@string{jan = \"January\"}
@string{feb = \"February\"}
@string{mar = \"March\"}
@string{apr = \"April\"}
@string{may = \"May\"}
@string{jun = \"June\"}
@string{jul = \"July\"}
@string{aug = \"August\"}
@string{sep = \"September\"}
@string{oct = \"October\"}
@string{nov = \"November\"}
@string{dec = \"December\"}
";

fn compute_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let offset = byte_offset.min(source.len());
    let line = source[..offset].chars().filter(|&c| c == '\n').count() + 1;
    let last_nl = source[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let column = source[last_nl..offset].chars().count() + 1;
    (line, column)
}

/// Parse a BibTeX or BibLaTeX source string into normalized entries.
pub fn parse_bibtex(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let bib = Bibliography::parse(source).map_err(|err| {
        let (line, column) = compute_line_col(source, err.span.start);
        ParseDiagnostic {
            file: file.map(|s| s.to_string()),
            line,
            column,
            message: format!("BibTeX parse error: {err}"),
        }
    })?;

    let mut entries = Vec::with_capacity(bib.len());
    for item in bib.iter() {
        entries.push(convert_bib_item(item));
    }
    Ok(entries)
}

/// Parse BibTeX source with error recovery, returning partial results and diagnostics.
pub fn parse_bibtex_resilient(
    source: &str,
    file: Option<&str>,
) -> (Vec<Entry>, Vec<ParseDiagnostic>) {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // Fast path: try parsing the whole source directly with standard biblatex parser.
    match Bibliography::parse(source) {
        Ok(bib) => {
            let mut diagnostics = Vec::new();
            let mut entries = Vec::with_capacity(bib.len());
            let (_seen_keys, duplicates) = scan_key_lines(source);

            for item in bib.iter() {
                entries.push(convert_bib_item(item));
            }

            for (dup_key, line, prev_line) in duplicates {
                diagnostics.push(ParseDiagnostic {
                    file: file.map(|s| s.to_string()),
                    line,
                    column: 1,
                    message: format!(
                        "duplicate citation key `{dup_key}` (previous on line {prev_line})"
                    ),
                });
            }

            (entries, diagnostics)
        }
        Err(_global_err) => {
            // Resilient fallback: parse top-level chunks
            let mut entries = Vec::new();
            let mut diagnostics = Vec::new();
            let mut header_macros = String::from(DEFAULT_MACROS);
            let mut seen_keys = HashMap::new();

            let chunks = split_top_level_chunks(source);
            for (chunk_offset, chunk) in chunks {
                let trimmed_chunk = chunk.trim();
                if trimmed_chunk.is_empty() {
                    continue;
                }

                let lower = trimmed_chunk.to_ascii_lowercase();
                if lower.starts_with("@string") || lower.starts_with("@preamble") {
                    header_macros.push_str(chunk);
                    header_macros.push('\n');
                    continue;
                }
                if lower.starts_with("@comment") {
                    continue;
                }

                // Attempt to parse chunk with accumulated header macros
                let chunk_source = format!("{header_macros}\n{chunk}");
                let prefix_len = header_macros.len() + 1;

                match Bibliography::parse(&chunk_source) {
                    Ok(bib) => {
                        for item in bib.iter() {
                            let (line, column) = compute_line_col(source, chunk_offset);
                            if let Some(&first_line) = seen_keys.get(&item.key) {
                                diagnostics.push(ParseDiagnostic {
                                    file: file.map(|s| s.to_string()),
                                    line,
                                    column,
                                    message: format!(
                                        "duplicate citation key `{}` (previous on line {first_line})",
                                        item.key
                                    ),
                                });
                            } else {
                                seen_keys.insert(item.key.clone(), line);
                            }
                            entries.push(convert_bib_item(item));
                        }
                    }
                    Err(err) => {
                        let relative_offset = err.span.start.saturating_sub(prefix_len);
                        let global_offset = (chunk_offset + relative_offset).min(source.len());
                        let (line, column) = compute_line_col(source, global_offset);
                        diagnostics.push(ParseDiagnostic {
                            file: file.map(|s| s.to_string()),
                            line,
                            column,
                            message: format!("BibTeX parse error: {err}"),
                        });
                    }
                }
            }

            (entries, diagnostics)
        }
    }
}

type DuplicateKey<'a> = (&'a str, usize, usize);
type ScanResult<'a> = (HashMap<&'a str, usize>, Vec<DuplicateKey<'a>>);

fn scan_key_lines(source: &str) -> ScanResult<'_> {
    let mut seen = HashMap::new();
    let mut duplicates = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('@')
            && let Some(open) = trimmed.find('{')
        {
            let type_name = trimmed[1..open].trim();
            if type_name.eq_ignore_ascii_case("string")
                || type_name.eq_ignore_ascii_case("preamble")
                || type_name.eq_ignore_ascii_case("comment")
            {
                continue;
            }
            let rest = &trimmed[open + 1..];
            let key = rest.split([',', ' ']).next().unwrap_or("").trim();
            if !key.is_empty() {
                let line_num = line_idx + 1;
                if let Some(&prev_line) = seen.get(key) {
                    duplicates.push((key, line_num, prev_line));
                } else {
                    seen.insert(key, line_num);
                }
            }
        }
    }

    (seen, duplicates)
}

/// Splits the source into top-level declarations starting with `@`.
fn split_top_level_chunks(source: &str) -> Vec<(usize, &str)> {
    let mut chunks = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    let mut chunk_start = None;
    let mut brace_depth: usize = 0;
    let mut in_quotes = false;
    let mut in_comment = false;

    while i < len {
        let b = bytes[i];

        if in_comment {
            if b == b'\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }

        if b == b'%' && brace_depth == 0 {
            in_comment = true;
            i += 1;
            continue;
        }

        // Look for start of a new entry
        if b == b'@' {
            let is_new_entry = if let Some(start) = chunk_start {
                if brace_depth == 0 {
                    true
                } else {
                    // Check if newline preceded this @, indicating broken previous entry
                    let mut prev = i;
                    let mut saw_nl = false;
                    while prev > start {
                        prev -= 1;
                        if bytes[prev] == b'\n' {
                            saw_nl = true;
                            break;
                        } else if !bytes[prev].is_ascii_whitespace() {
                            break;
                        }
                    }
                    saw_nl
                }
            } else {
                true
            };

            if is_new_entry {
                if let Some(start) = chunk_start {
                    chunks.push((start, &source[start..i]));
                }
                chunk_start = Some(i);
                brace_depth = 0;
                in_quotes = false;
            }
        }

        if chunk_start.is_some() {
            match b {
                b'"' if brace_depth <= 1 => {
                    in_quotes = !in_quotes;
                }
                b'{' if !in_quotes => {
                    brace_depth += 1;
                }
                b'}' if !in_quotes => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0
                        && let Some(start) = chunk_start
                    {
                        chunks.push((start, &source[start..=i]));
                        chunk_start = None;
                    }
                }
                _ => {}
            }
        }

        i += 1;
    }

    if let Some(start) = chunk_start
        && start < len
    {
        chunks.push((start, &source[start..]));
    }

    chunks
}

fn convert_bib_item(item: &biblatex::Entry) -> Entry {
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
                    if part.len() == 4
                        && let Ok(y) = part.parse::<u32>()
                    {
                        year = Some(y);
                        break;
                    }
                }
            }
        }
    }

    // Fallbacks for year from raw_fields if not found via date()
    if year.is_none()
        && let Some(y_str) = raw_fields.get("year").or_else(|| raw_fields.get("date"))
    {
        for part in y_str.split(|c: char| !c.is_ascii_digit()) {
            if part.len() == 4
                && let Ok(y) = part.parse::<u32>()
            {
                year = Some(y);
                break;
            }
        }
    }

    // Fallback for month
    if month.is_none()
        && let Some(m_str) = raw_fields.get("month")
        && let Ok(m) = m_str.trim().parse::<u32>()
    {
        month = Some(m);
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

    Entry {
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
    }
}
