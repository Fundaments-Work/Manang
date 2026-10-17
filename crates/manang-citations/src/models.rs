use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A normalized bibliographic entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_or_publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arxiv_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstract_text: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub raw_fields: HashMap<String, String>,
}

/// An author associated with a bibliography entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given: Option<String>,
}

impl Author {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name,
            family: None,
            given: None,
        }
    }

    pub fn with_parts(family: impl Into<String>, given: impl Into<String>) -> Self {
        let family = family.into();
        let given = given.into();
        let name = format!("{given} {family}").trim().to_string();
        Self {
            name,
            family: Some(family),
            given: Some(given),
        }
    }
}

/// A difference detected during citation verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Difference {
    pub field: String,
    pub local: String,
    pub remote: String,
}

/// Verification status against remote authoritative registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Mismatch,
    NotFound,
    Error,
}

/// The outcome of verifying a citation against remote registries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    #[serde(default)]
    pub matched: Option<Entry>,
    #[serde(default)]
    pub differences: Vec<Difference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A syntax or semantic error diagnostic when parsing bibliography files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for ParseDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(file) = &self.file {
            write!(
                f,
                "{}:{}:{}: {}",
                file, self.line, self.column, self.message
            )
        } else {
            write!(f, "{}:{}: {}", self.line, self.column, self.message)
        }
    }
}

impl std::error::Error for ParseDiagnostic {}

/// A ranked search result item from the in-memory citation index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub key: String,
    pub score: f64,
    pub entry: Entry,
}
