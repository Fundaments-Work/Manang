use crate::error::CitationsError;
use std::collections::HashSet;

/// Extracts citation keys from a source text given the syntax dialect.
pub fn extract_keys(source: &str, syntax: &str) -> Result<Vec<String>, CitationsError> {
    match syntax.trim().to_ascii_lowercase().as_str() {
        "typst" => Ok(extract_typst_keys(source)),
        "latex" | "tex" => Ok(extract_latex_keys(source)),
        "markdown" | "md" | "pandoc" | "quarto" => Ok(extract_markdown_keys(source)),
        other => Err(CitationsError::UnsupportedSyntax(other.to_string())),
    }
}

/// Extracts `@key` from Typst source, avoiding email addresses (`user@domain`).
pub fn extract_typst_keys(source: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();

    let bytes = source.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'@' {
            // Check preceding character: must NOT be alphanumeric (e.g. email)
            let preceded_by_word = if idx > 0 {
                let prev = bytes[idx - 1] as char;
                prev.is_ascii_alphanumeric() || prev == '_'
            } else {
                false
            };

            if !preceded_by_word {
                idx += 1;
                let start = idx;
                // Citation key characters: ASCII alphanumeric, _, -, :
                while idx < bytes.len() {
                    let c = bytes[idx] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':' {
                        idx += 1;
                    } else {
                        break;
                    }
                }

                if idx > start {
                    let key = &source[start..idx];
                    // Skip purely numeric keys or single punctuation
                    if !key.is_empty() && seen.insert(key.to_string()) {
                        keys.push(key.to_string());
                    }
                }
                continue;
            }
        }
        idx += 1;
    }

    keys
}

/// Extracts keys from LaTeX citation commands like `\cite{...}`, `\citep{...}`, etc.
pub fn extract_latex_keys(source: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();

    const CITE_COMMANDS: &[&str] = &[
        "\\cite",
        "\\citep",
        "\\citet",
        "\\citeauthor",
        "\\citeyear",
        "\\nocite",
        "\\parencite",
        "\\textcite",
        "\\autocite",
        "\\footcite",
        "\\citealp",
        "\\citealt",
    ];

    let mut rest = source;
    while let Some(backslash_pos) = rest.find('\\') {
        rest = &rest[backslash_pos..];
        let mut matched_cmd = false;

        for cmd in CITE_COMMANDS {
            if rest.starts_with(cmd) {
                let after_cmd = &rest[cmd.len()..];
                // Followed by optional brackets [...] and then {...}
                let mut cursor = after_cmd.trim_start();
                while cursor.starts_with('[') {
                    if let Some(close_bracket) = cursor.find(']') {
                        cursor = cursor[close_bracket + 1..].trim_start();
                    } else {
                        break;
                    }
                }

                if cursor.starts_with('{') {
                    if let Some(close_brace) = cursor.find('}') {
                        let inner = &cursor[1..close_brace];
                        for raw_key in inner.split(',') {
                            let key = raw_key.trim();
                            if !key.is_empty() && seen.insert(key.to_string()) {
                                keys.push(key.to_string());
                            }
                        }
                        rest = &cursor[close_brace + 1..];
                        matched_cmd = true;
                        break;
                    }
                }
            }
        }

        if !matched_cmd {
            if rest.len() > 1 {
                rest = &rest[1..];
            } else {
                break;
            }
        }
    }

    keys
}

/// Extracts keys from Pandoc/Quarto markdown citations: `[@key1; @key2]`, `[-@key]`, `@key[p. 30]`.
pub fn extract_markdown_keys(source: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();

    let bytes = source.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'@' {
            let preceded_by_word = if idx > 0 {
                let prev = bytes[idx - 1] as char;
                prev.is_ascii_alphanumeric() || prev == '_'
            } else {
                false
            };

            if !preceded_by_word {
                idx += 1;
                // Optional leading minus inside brackets `[-@key]`
                let start = idx;
                while idx < bytes.len() {
                    let c = bytes[idx] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':' {
                        idx += 1;
                    } else {
                        break;
                    }
                }

                if idx > start {
                    let mut key = &source[start..idx];
                    key = key.trim_start_matches('-');
                    if !key.is_empty() && seen.insert(key.to_string()) {
                        keys.push(key.to_string());
                    }
                }
                continue;
            }
        }
        idx += 1;
    }

    keys
}
