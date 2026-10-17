use crate::models::{Author, Entry, ParseDiagnostic};
use serde_json::Value;
use std::collections::HashMap;

/// Parse a CSL-JSON source string into normalized entries.
pub fn parse_csl_json(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic> {
    let parsed: Value = serde_json::from_str(source).map_err(|err| ParseDiagnostic {
        file: file.map(|s| s.to_string()),
        line: err.line(),
        column: err.column(),
        message: format!("CSL-JSON parse error: {err}"),
    })?;

    let items = match parsed {
        Value::Array(arr) => arr,
        Value::Object(_) => vec![parsed],
        _ => {
            return Err(ParseDiagnostic {
                file: file.map(|s| s.to_string()),
                line: 1,
                column: 1,
                message: "CSL-JSON root must be a JSON object or array of objects".to_string(),
            });
        }
    };

    let mut entries = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if let Some(entry) = csl_item_to_entry(item, idx) {
            entries.push(entry);
        }
    }

    Ok(entries)
}

fn csl_item_to_entry(item: &Value, idx: usize) -> Option<Entry> {
    let obj = item.as_object()?;

    // Citation key: id field
    let key = obj
        .get("id")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                v.as_i64().map(|n| n.to_string())
            }
        })
        .unwrap_or_else(|| format!("csl_{idx}"));

    let entry_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("article")
        .to_string();

    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Authors
    let mut authors = Vec::new();
    if let Some(arr) = obj.get("author").and_then(|v| v.as_array()) {
        for a in arr {
            if let Some(literal) = a
                .get("literal")
                .or_else(|| a.get("name"))
                .and_then(|v| v.as_str())
            {
                authors.push(Author::new(literal));
            } else {
                let family = a
                    .get("family")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let given = a
                    .get("given")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
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
    }

    // Issued date (year, month)
    let mut year = None;
    let mut month = None;
    if let Some(issued) = obj.get("issued") {
        if let Some(parts) = issued.get("date-parts").and_then(|v| v.as_array()) {
            if let Some(first_date) = parts.first().and_then(|v| v.as_array()) {
                if let Some(y) = first_date.first().and_then(|v| v.as_u64()) {
                    year = Some(y as u32);
                }
                if let Some(m) = first_date.get(1).and_then(|v| v.as_u64()) {
                    month = Some(m as u32);
                }
            }
        } else if let Some(raw) = issued.get("raw").and_then(|v| v.as_str()) {
            for word in raw.split(|c: char| !c.is_ascii_digit()) {
                if word.len() == 4
                    && let Ok(y) = word.parse::<u32>()
                {
                    year = Some(y);
                    break;
                }
            }
        }
    }

    let journal_or_publisher = obj
        .get("container-title")
        .or_else(|| obj.get("publisher"))
        .or_else(|| obj.get("collection-title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let volume = obj.get("volume").and_then(|v| {
        v.as_str()
            .map(|s| s.to_string())
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    });

    let number = obj
        .get("issue")
        .or_else(|| obj.get("number"))
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        });

    let pages = obj
        .get("page")
        .or_else(|| obj.get("pages"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let doi = obj
        .get("DOI")
        .or_else(|| obj.get("doi"))
        .and_then(|v| v.as_str())
        .map(|d| {
            d.trim()
                .trim_start_matches("https://doi.org/")
                .trim_start_matches("http://doi.org/")
                .trim_start_matches("doi:")
                .to_string()
        });

    let url = obj
        .get("URL")
        .or_else(|| obj.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // arXiv ID detection
    let arxiv_id = obj
        .get("arxiv")
        .or_else(|| obj.get("eprint"))
        .and_then(|v| v.as_str())
        .map(|s| {
            s.trim()
                .trim_start_matches("arXiv:")
                .trim_start_matches("arxiv:")
                .to_string()
        })
        .or_else(|| {
            if let Some(d) = &doi
                && let Some(stripped) = d.strip_prefix("10.48550/arXiv.")
            {
                return Some(stripped.to_string());
            }
            if let Some(u) = &url
                && let Some(pos) = u.find("arxiv.org/abs/")
            {
                let id = &u[pos + 14..];
                return Some(id.trim_end_matches('/').to_string());
            }
            None
        });

    let abstract_text = obj
        .get("abstract")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Raw fields
    let mut raw_fields = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            raw_fields.insert(k.clone(), s.to_string());
        } else if let Some(n) = v.as_i64() {
            raw_fields.insert(k.clone(), n.to_string());
        } else if let Some(b) = v.as_bool() {
            raw_fields.insert(k.clone(), b.to_string());
        }
    }

    Some(Entry {
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
    })
}
