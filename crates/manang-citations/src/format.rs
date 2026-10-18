use crate::error::CitationsError;
use crate::models::{Author, Entry};

/// Whether [`format_entry`] accepts `style`.
pub fn is_supported_style(style: &str) -> bool {
    matches!(
        style.trim().to_ascii_lowercase().as_str(),
        "apa" | "apa-like" | "numeric" | "ieee"
    )
}

/// Formats an entry in a given bibliographic style.
pub fn format_entry(entry: &Entry, style: &str) -> Result<String, CitationsError> {
    let style_clean = style.trim().to_ascii_lowercase();
    match style_clean.as_str() {
        "apa" | "apa-like" => Ok(format_apa(entry)),
        "numeric" | "ieee" => Ok(format_numeric(entry)),
        other => Err(CitationsError::UnsupportedStyle(other.to_string())),
    }
}

/// Format an entry in APA-like style.
/// Example: Einstein, A. (1905). Zur Elektrodynamik bewegter Körper. Annalen der Physik, 322(10), 891–921. https://doi.org/10.1002/andp.19053221004
pub fn format_apa(entry: &Entry) -> String {
    let mut parts = Vec::new();

    // 1. Authors
    let authors_str = format_apa_authors(&entry.authors);
    if !authors_str.is_empty() {
        parts.push(authors_str);
    }

    // 2. Year
    let year_str = match entry.year {
        Some(y) => format!("({y})."),
        None => "(n.d.).".to_string(),
    };
    parts.push(year_str);

    // 3. Title
    if let Some(title) = &entry.title {
        let mut t = title.trim().to_string();
        if !t.ends_with('.') && !t.ends_with('?') && !t.ends_with('!') {
            t.push('.');
        }
        parts.push(t);
    }

    // 4. Container / Source
    let mut container_parts = Vec::new();
    if let Some(source) = &entry.journal_or_publisher {
        let mut s = source.clone();
        if let Some(vol) = &entry.volume {
            s.push_str(&format!(", {vol}"));
            if let Some(num) = &entry.number {
                s.push_str(&format!("({num})"));
            }
        } else if let Some(num) = &entry.number {
            s.push_str(&format!(", No. {num}"));
        }
        if let Some(pages) = &entry.pages {
            s.push_str(&format!(", {pages}"));
        }
        if !s.ends_with('.') {
            s.push('.');
        }
        container_parts.push(s);
    }
    if !container_parts.is_empty() {
        parts.push(container_parts.join(" "));
    }

    // 5. DOI or URL
    if let Some(doi) = &entry.doi {
        parts.push(format!("https://doi.org/{doi}"));
    } else if let Some(url) = &entry.url {
        parts.push(url.clone());
    }

    parts.join(" ")
}

/// Format an entry in IEEE-like numeric style.
/// Example: [1] A. Einstein, "Zur Elektrodynamik bewegter Körper," Annalen der Physik, vol. 322, no. 10, pp. 891–921, 1905. https://doi.org/10.1002/andp.19053221004
pub fn format_numeric(entry: &Entry) -> String {
    let mut parts = Vec::new();

    // 1. Authors
    let authors_str = format_numeric_authors(&entry.authors);
    if !authors_str.is_empty() {
        parts.push(format!("{authors_str},"));
    }

    // 2. Title
    if let Some(title) = &entry.title {
        let t = title.trim().trim_end_matches('.');
        parts.push(format!("\"{t},\""));
    }

    // 3. Container
    let mut source_parts = Vec::new();
    if let Some(source) = &entry.journal_or_publisher {
        source_parts.push(source.clone());
    }
    if let Some(vol) = &entry.volume {
        source_parts.push(format!("vol. {vol}"));
    }
    if let Some(num) = &entry.number {
        source_parts.push(format!("no. {num}"));
    }
    if let Some(pages) = &entry.pages {
        let p_label = if pages.contains('-') || pages.contains('–') {
            "pp."
        } else {
            "p."
        };
        source_parts.push(format!("{p_label} {pages}"));
    }
    if let Some(year) = entry.year {
        source_parts.push(year.to_string());
    }

    if !source_parts.is_empty() {
        let mut s = source_parts.join(", ");
        if !s.ends_with('.') {
            s.push('.');
        }
        parts.push(s);
    }

    // 4. DOI or URL
    if let Some(doi) = &entry.doi {
        parts.push(format!("https://doi.org/{doi}"));
    } else if let Some(url) = &entry.url {
        parts.push(url.clone());
    }

    format!("[1] {}", parts.join(" "))
}

fn format_apa_authors(authors: &[Author]) -> String {
    if authors.is_empty() {
        return String::new();
    }

    let formatted: Vec<String> = authors
        .iter()
        .map(|a| {
            if let Some(family) = &a.family {
                if let Some(given) = &a.given {
                    let initials: Vec<String> = given
                        .split_whitespace()
                        .filter_map(|w| w.chars().next().map(|c| format!("{c}.")))
                        .collect();
                    if !initials.is_empty() {
                        format!("{family}, {}", initials.join(" "))
                    } else {
                        family.clone()
                    }
                } else {
                    family.clone()
                }
            } else {
                a.name.clone()
            }
        })
        .collect();

    match formatted.len() {
        1 => {
            let mut s = formatted[0].clone();
            if !s.ends_with('.') {
                s.push('.');
            }
            s
        }
        2 => format!(
            "{}, & {}.",
            formatted[0],
            formatted[1].trim_end_matches('.')
        ),
        n if n <= 20 => {
            let all_except_last = formatted[..n - 1].join(", ");
            let last = formatted[n - 1].trim_end_matches('.');
            format!("{all_except_last}, & {last}.")
        }
        _ => {
            let first_19 = formatted[..19].join(", ");
            let last = formatted
                .last()
                .map(|s| s.trim_end_matches('.'))
                .unwrap_or_default();
            format!("{first_19}, ... {last}.")
        }
    }
}

fn format_numeric_authors(authors: &[Author]) -> String {
    if authors.is_empty() {
        return String::new();
    }

    let formatted: Vec<String> = authors
        .iter()
        .map(|a| {
            if let (Some(family), Some(given)) = (&a.family, &a.given) {
                let initials: Vec<String> = given
                    .split_whitespace()
                    .filter_map(|w| w.chars().next().map(|c| format!("{c}.")))
                    .collect();
                if !initials.is_empty() {
                    format!("{} {family}", initials.join(" "))
                } else {
                    format!("{given} {family}")
                }
            } else {
                a.name.clone()
            }
        })
        .collect();

    match formatted.len() {
        1 => formatted[0].clone(),
        2 => format!("{} and {}", formatted[0], formatted[1]),
        3 => format!("{}, {}, and {}", formatted[0], formatted[1], formatted[2]),
        _ => format!("{} et al.", formatted[0]),
    }
}
