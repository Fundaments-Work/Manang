use crate::error::CitationsError;
use crate::http::HttpClient;
use crate::models::{Author, Difference, Entry, VerificationResult, VerificationStatus};
use serde_json::Value;

/// Verifies a local citation entry against authoritative registries (Crossref, OpenAlex, arXiv).
pub fn verify_entry(local: &Entry, client: &dyn HttpClient) -> VerificationResult {
    // 1. Try DOI resolution (Crossref -> OpenAlex)
    if let Some(doi) = &local.doi {
        if let Ok(result) = verify_by_doi(local, doi, client) {
            return result;
        }
    }

    // 2. Try arXiv resolution (export.arxiv.org -> OpenAlex)
    if let Some(arxiv_id) = &local.arxiv_id {
        if let Ok(result) = verify_by_arxiv(local, arxiv_id, client) {
            return result;
        }
    }

    // 3. Try bibliographic title search (Crossref search -> OpenAlex search)
    if let Some(title) = &local.title {
        if let Ok(result) = verify_by_title_search(local, title, client) {
            return result;
        }
    }

    VerificationResult {
        status: VerificationStatus::NotFound,
        matched: None,
        differences: Vec::new(),
        message: Some(
            "no identifier (DOI, arXiv ID) or title matched in remote registries".to_string(),
        ),
    }
}

fn verify_by_doi(
    local: &Entry,
    doi: &str,
    client: &dyn HttpClient,
) -> Result<VerificationResult, CitationsError> {
    // Clean DOI
    let clean_doi = doi
        .trim()
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim_start_matches("doi:");

    // Try Crossref: https://api.crossref.org/works/{clean_doi}
    let crossref_url = format!("https://api.crossref.org/works/{clean_doi}");
    if let Ok(resp) = client.get(&crossref_url, &[("Accept", "application/json")]) {
        if resp.status == 200 {
            if let Ok(val) = serde_json::from_slice::<Value>(&resp.body) {
                if let Some(msg) = val.get("message") {
                    if let Some(remote) = parse_crossref_work(msg, local.key.clone()) {
                        return Ok(compare_entries(local, remote));
                    }
                }
            }
        }
    }

    // Try OpenAlex fallback: https://api.openalex.org/works/https://doi.org/{clean_doi}
    let openalex_url = format!("https://api.openalex.org/works/https://doi.org/{clean_doi}");
    if let Ok(resp) = client.get(&openalex_url, &[("Accept", "application/json")]) {
        if resp.status == 200 {
            if let Ok(val) = serde_json::from_slice::<Value>(&resp.body) {
                if let Some(remote) = parse_openalex_work(&val, local.key.clone()) {
                    return Ok(compare_entries(local, remote));
                }
            }
        }
    }

    Err(CitationsError::Http(
        "DOI not found in Crossref or OpenAlex".to_string(),
    ))
}

fn verify_by_arxiv(
    local: &Entry,
    arxiv_id: &str,
    client: &dyn HttpClient,
) -> Result<VerificationResult, CitationsError> {
    let clean_id = arxiv_id
        .trim()
        .trim_start_matches("arXiv:")
        .trim_start_matches("arxiv:");

    // Try arXiv export: https://export.arxiv.org/api/query?id_list={clean_id}
    let arxiv_url = format!("https://export.arxiv.org/api/query?id_list={clean_id}");
    if let Ok(resp) = client.get(&arxiv_url, &[]) {
        if resp.status == 200 {
            if let Ok(xml_str) = std::str::from_utf8(&resp.body) {
                if let Some(remote) = parse_arxiv_atom(xml_str, clean_id, local.key.clone()) {
                    return Ok(compare_entries(local, remote));
                }
            }
        }
    }

    // Try OpenAlex fallback: https://api.openalex.org/works?filter=ids.arxiv:{clean_id}
    let openalex_url = format!("https://api.openalex.org/works?filter=ids.arxiv:{clean_id}");
    if let Ok(resp) = client.get(&openalex_url, &[("Accept", "application/json")]) {
        if resp.status == 200 {
            if let Ok(val) = serde_json::from_slice::<Value>(&resp.body) {
                if let Some(results) = val.get("results").and_then(|v| v.as_array()) {
                    if let Some(first) = results.first() {
                        if let Some(remote) = parse_openalex_work(first, local.key.clone()) {
                            return Ok(compare_entries(local, remote));
                        }
                    }
                }
            }
        }
    }

    Err(CitationsError::Http("arXiv ID not found".to_string()))
}

fn verify_by_title_search(
    local: &Entry,
    title: &str,
    client: &dyn HttpClient,
) -> Result<VerificationResult, CitationsError> {
    let encoded = url_encode(title);

    // Crossref query: https://api.crossref.org/works?query.bibliographic={encoded}&rows=1
    let crossref_url =
        format!("https://api.crossref.org/works?query.bibliographic={encoded}&rows=1");
    if let Ok(resp) = client.get(&crossref_url, &[("Accept", "application/json")]) {
        if resp.status == 200 {
            if let Ok(val) = serde_json::from_slice::<Value>(&resp.body) {
                if let Some(items) = val
                    .get("message")
                    .and_then(|m| m.get("items"))
                    .and_then(|it| it.as_array())
                {
                    if let Some(first) = items.first() {
                        if let Some(remote) = parse_crossref_work(first, local.key.clone()) {
                            return Ok(compare_entries(local, remote));
                        }
                    }
                }
            }
        }
    }

    // OpenAlex query: https://api.openalex.org/works?search={encoded}&per_page=1
    let openalex_url = format!("https://api.openalex.org/works?search={encoded}&per_page=1");
    if let Ok(resp) = client.get(&openalex_url, &[("Accept", "application/json")]) {
        if resp.status == 200 {
            if let Ok(val) = serde_json::from_slice::<Value>(&resp.body) {
                if let Some(results) = val.get("results").and_then(|v| v.as_array()) {
                    if let Some(first) = results.first() {
                        if let Some(remote) = parse_openalex_work(first, local.key.clone()) {
                            return Ok(compare_entries(local, remote));
                        }
                    }
                }
            }
        }
    }

    Err(CitationsError::Http(
        "Title search yielded no authoritative matches".to_string(),
    ))
}

fn compare_entries(local: &Entry, remote: Entry) -> VerificationResult {
    let mut differences = Vec::new();

    // 1. Compare title
    if let (Some(loc_t), Some(rem_t)) = (&local.title, &remote.title) {
        let norm_loc = normalize_text(loc_t);
        let norm_rem = normalize_text(rem_t);
        if norm_loc != norm_rem && !norm_rem.contains(&norm_loc) && !norm_loc.contains(&norm_rem) {
            differences.push(Difference {
                field: "title".to_string(),
                local: loc_t.clone(),
                remote: rem_t.clone(),
            });
        }
    }

    // 2. Compare primary author family name
    if let (Some(loc_a), Some(rem_a)) = (local.authors.first(), remote.authors.first()) {
        let loc_fam = loc_a.family.as_deref().unwrap_or(&loc_a.name);
        let rem_fam = rem_a.family.as_deref().unwrap_or(&rem_a.name);
        let norm_loc = normalize_text(loc_fam);
        let norm_rem = normalize_text(rem_fam);
        if norm_loc != norm_rem && !norm_rem.contains(&norm_loc) && !norm_loc.contains(&norm_rem) {
            differences.push(Difference {
                field: "author".to_string(),
                local: loc_a.name.clone(),
                remote: rem_a.name.clone(),
            });
        }
    }

    // 3. Compare publication year
    if let (Some(loc_y), Some(rem_y)) = (local.year, remote.year) {
        if loc_y != rem_y {
            differences.push(Difference {
                field: "year".to_string(),
                local: loc_y.to_string(),
                remote: rem_y.to_string(),
            });
        }
    }

    let status = if differences.is_empty() {
        VerificationStatus::Verified
    } else {
        VerificationStatus::Mismatch
    };

    VerificationResult {
        status,
        matched: Some(remote),
        differences,
        message: None,
    }
}

fn parse_crossref_work(msg: &Value, fallback_key: String) -> Option<Entry> {
    let title = msg
        .get("title")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut authors = Vec::new();
    if let Some(arr) = msg.get("author").and_then(|v| v.as_array()) {
        for a in arr {
            let given = a
                .get("given")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let family = a
                .get("family")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(fam) = family {
                if let Some(giv) = given {
                    authors.push(Author::with_parts(fam, giv));
                } else {
                    authors.push(Author {
                        name: fam.clone(),
                        family: Some(fam),
                        given: None,
                    });
                }
            } else if let Some(name) = a.get("name").and_then(|v| v.as_str()) {
                authors.push(Author::new(name));
            }
        }
    }

    let mut year = None;
    if let Some(issued) = msg
        .get("issued")
        .or_else(|| msg.get("published-print"))
        .or_else(|| msg.get("published-online"))
    {
        if let Some(parts) = issued.get("date-parts").and_then(|v| v.as_array()) {
            if let Some(first_arr) = parts.first().and_then(|v| v.as_array()) {
                if let Some(y) = first_arr.first().and_then(|v| v.as_u64()) {
                    year = Some(y as u32);
                }
            }
        }
    }

    let container = msg
        .get("container-title")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .or_else(|| msg.get("publisher").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let volume = msg
        .get("volume")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let number = msg
        .get("issue")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let pages = msg
        .get("page")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let doi = msg
        .get("DOI")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let url = msg
        .get("URL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(Entry {
        key: fallback_key,
        entry_type: "article".to_string(),
        title,
        authors,
        year,
        month: None,
        journal_or_publisher: container,
        volume,
        number,
        pages,
        doi,
        url,
        arxiv_id: None,
        abstract_text: None,
        raw_fields: std::collections::HashMap::new(),
    })
}

fn parse_openalex_work(val: &Value, fallback_key: String) -> Option<Entry> {
    let title = val
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let year = val
        .get("publication_year")
        .and_then(|v| v.as_u64())
        .map(|y| y as u32);

    let mut authors = Vec::new();
    if let Some(arr) = val.get("authorships").and_then(|v| v.as_array()) {
        for a in arr {
            if let Some(name) = a
                .get("author")
                .and_then(|auth| auth.get("display_name"))
                .and_then(|v| v.as_str())
            {
                // Simple split of first and last name
                if let Some((first, last)) = name.rsplit_once(' ') {
                    authors.push(Author::with_parts(last, first));
                } else {
                    authors.push(Author::new(name));
                }
            }
        }
    }

    let doi = val
        .get("doi")
        .and_then(|v| v.as_str())
        .map(|d| d.trim_start_matches("https://doi.org/").to_string());

    let container = val
        .get("primary_location")
        .and_then(|loc| loc.get("source"))
        .and_then(|src| src.get("display_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(Entry {
        key: fallback_key,
        entry_type: "article".to_string(),
        title,
        authors,
        year,
        month: None,
        journal_or_publisher: container,
        volume: None,
        number: None,
        pages: None,
        doi,
        url: None,
        arxiv_id: None,
        abstract_text: None,
        raw_fields: std::collections::HashMap::new(),
    })
}

fn parse_arxiv_atom(xml: &str, arxiv_id: &str, fallback_key: String) -> Option<Entry> {
    let entry_start = xml.find("<entry>")?;
    let entry_end = xml.find("</entry>")?;
    if entry_end <= entry_start {
        return None;
    }
    let entry_xml = &xml[entry_start..entry_end];

    let title = extract_xml_tag(entry_xml, "title")
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "));

    if title.as_deref() == Some("Error") {
        return None;
    }

    let mut authors = Vec::new();
    let mut cursor = entry_xml;
    while let Some(pos) = cursor.find("<author>") {
        cursor = &cursor[pos..];
        if let Some(end_pos) = cursor.find("</author>") {
            let author_xml = &cursor[..end_pos];
            if let Some(name) = extract_xml_tag(author_xml, "name") {
                let name_clean = name.trim();
                if let Some((first, last)) = name_clean.rsplit_once(' ') {
                    authors.push(Author::with_parts(last, first));
                } else {
                    authors.push(Author::new(name_clean));
                }
            }
            cursor = &cursor[end_pos + 9..];
        } else {
            break;
        }
    }

    let year = extract_xml_tag(entry_xml, "published").and_then(|p| {
        p.split('-')
            .next()
            .and_then(|y| y.trim().parse::<u32>().ok())
    });

    let summary = extract_xml_tag(entry_xml, "summary")
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "));

    Some(Entry {
        key: fallback_key,
        entry_type: "preprint".to_string(),
        title,
        authors,
        year,
        month: None,
        journal_or_publisher: Some("arXiv".to_string()),
        volume: None,
        number: None,
        pages: None,
        doi: None,
        url: Some(format!("https://arxiv.org/abs/{arxiv_id}")),
        arxiv_id: Some(arxiv_id.to_string()),
        abstract_text: summary,
        raw_fields: std::collections::HashMap::new(),
    })
}

fn extract_xml_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim())
}

fn normalize_text(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else if b == b' ' {
            encoded.push('+');
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}
