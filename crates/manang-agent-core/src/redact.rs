//! Redaction of text before it leaves the machine.
//!
//! Secret-like strings are redacted by default; email addresses and home
//! directory user names are opt-in. The result carries a diff (byte spans in
//! the ORIGINAL text) so the UI can show what was removed without this crate
//! copying the secret anywhere. Redaction is idempotent.

use serde::{Deserialize, Serialize};

/// Which rules run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RedactionConfig {
    pub secrets: bool,
    pub emails: bool,
    pub paths: bool,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            secrets: true,
            emails: false,
            paths: false,
        }
    }
}

/// What a span was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionKind {
    Secret,
    PrivateKey,
    Email,
    Path,
}

impl RedactionKind {
    fn placeholder(self) -> &'static str {
        match self {
            Self::Secret => "[REDACTED:secret]",
            Self::PrivateKey => "[REDACTED:private_key]",
            Self::Email => "[REDACTED:email]",
            Self::Path => "[REDACTED:user]",
        }
    }
}

/// One replaced range of the original text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionSpan {
    pub kind: RedactionKind,
    /// Byte offsets into the original text.
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// The redacted text and the diff that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
    pub text: String,
    pub spans: Vec<RedactionSpan>,
}

const SECRET_PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "hf_",
    "glpat-",
];
const ASSIGN_KEYS: &[&str] = &[
    "api_key", "apikey", "api-key", "secret", "token", "password", "passwd",
];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=' | '.')
}

fn span(kind: RedactionKind, start: usize, end: usize) -> RedactionSpan {
    RedactionSpan {
        kind,
        start,
        end,
        replacement: kind.placeholder().to_owned(),
    }
}

fn looks_secret(token: &str) -> bool {
    for prefix in SECRET_PREFIXES {
        if token.starts_with(prefix) && token.len() >= prefix.len() + 16 {
            return true;
        }
    }
    if token.len() == 20
        && token.starts_with("AKIA")
        && token
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return true;
    }
    if token.len() == 39 && token.starts_with("AIza") {
        return true;
    }
    // JSON web token: three dot-separated base64url parts.
    if token.starts_with("eyJ") && token.len() >= 30 && token.matches('.').count() == 2 {
        return true;
    }
    false
}

fn private_keys(text: &str, out: &mut Vec<RedactionSpan>) {
    let mut from = 0;
    while let Some(rel) = text[from..].find("-----BEGIN ") {
        let start = from + rel;
        let Some(header_rel) = text[start + 11..].find("-----") else {
            return;
        };
        let header_end = start + 11 + header_rel + 5;
        if !text[start..header_end].contains("PRIVATE KEY") {
            from = header_end;
            continue;
        }
        let end = match text[header_end..].find("-----END ") {
            Some(e_rel) => {
                let end_marker = header_end + e_rel + 9;
                match text[end_marker..].find("-----") {
                    Some(close) => end_marker + close + 5,
                    None => text.len(),
                }
            }
            None => text.len(),
        };
        out.push(span(RedactionKind::PrivateKey, start, end));
        from = end;
    }
}

fn secrets(text: &str, out: &mut Vec<RedactionSpan>) {
    private_keys(text, out);
    // Tokens with byte ranges.
    let mut tokens: Vec<(usize, usize)> = Vec::new();
    let mut begin: Option<usize> = None;
    for (i, c) in text.char_indices() {
        match (is_token_char(c), begin) {
            (true, None) => begin = Some(i),
            (false, Some(b)) => {
                tokens.push((b, i));
                begin = None;
            }
            _ => {}
        }
    }
    if let Some(b) = begin {
        tokens.push((b, text.len()));
    }
    let mut previous_was_bearer = false;
    for (start, end) in tokens {
        // A sentence-ending dot is not part of the secret.
        let trimmed_end = start + text[start..end].trim_end_matches('.').len();
        let token = &text[start..trimmed_end];
        if previous_was_bearer && token.len() >= 16 {
            out.push(span(RedactionKind::Secret, start, trimmed_end));
        } else if let Some((key, value)) = token.split_once('=') {
            let key = key.to_ascii_lowercase();
            let named_secret = value.len() >= 8
                && ASSIGN_KEYS.iter().any(|k| key.contains(k))
                && !value.starts_with("[REDACTED");
            if named_secret || looks_secret(value) {
                out.push(span(
                    RedactionKind::Secret,
                    trimmed_end - value.len(),
                    trimmed_end,
                ));
            }
        } else if looks_secret(token) {
            out.push(span(RedactionKind::Secret, start, trimmed_end));
        }
        previous_was_bearer = token.eq_ignore_ascii_case("bearer");
    }
}

fn emails(text: &str, out: &mut Vec<RedactionSpan>) {
    let local_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-');
    let domain_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
    // A local part may not reach back into the previous address.
    let mut floor = 0;
    for (at, _) in text.match_indices('@') {
        if at < floor {
            continue;
        }
        let start = text[floor..at]
            .char_indices()
            .rev()
            .take_while(|(_, c)| local_ok(*c))
            .last()
            .map_or(at, |(i, _)| floor + i);
        let domain_len = text[at + 1..]
            .char_indices()
            .take_while(|(_, c)| domain_ok(*c))
            .last()
            .map_or(0, |(i, c)| i + c.len_utf8());
        let domain = text[at + 1..at + 1 + domain_len].trim_end_matches(['.', '-']);
        if start < at && domain.contains('.') && !domain.starts_with('.') {
            floor = at + 1 + domain.len();
            out.push(span(RedactionKind::Email, start, floor));
        }
    }
}

fn paths(text: &str, out: &mut Vec<RedactionSpan>) {
    for marker in ["/home/", "/Users/", "C:\\Users\\"] {
        for (at, _) in text.match_indices(marker) {
            let start = at + marker.len();
            let len = text[start..]
                .find(|c: char| matches!(c, '/' | '\\') || c.is_whitespace() || "\"'`".contains(c))
                .unwrap_or(text.len() - start);
            if len > 0 && !text[start..].starts_with('[') {
                out.push(span(RedactionKind::Path, start, start + len));
            }
        }
    }
}

/// Redacts `input` according to `config`.
pub fn redact(input: &str, config: &RedactionConfig) -> Redaction {
    let mut found = Vec::new();
    if config.secrets {
        secrets(input, &mut found);
    }
    if config.emails {
        emails(input, &mut found);
    }
    if config.paths {
        paths(input, &mut found);
    }
    // Earlier first; for equal starts the longer span wins; drop overlaps.
    found.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut spans: Vec<RedactionSpan> = Vec::with_capacity(found.len());
    for candidate in found {
        if spans.last().is_none_or(|last| candidate.start >= last.end) {
            spans.push(candidate);
        }
    }
    let mut text = String::with_capacity(input.len());
    let mut cursor = 0;
    for s in &spans {
        text.push_str(&input[cursor..s.start]);
        text.push_str(&s.replacement);
        cursor = s.end;
    }
    text.push_str(&input[cursor..]);
    Redaction { text, spans }
}
