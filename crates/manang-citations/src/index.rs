use crate::models::{Entry, SearchResult};
use parking_lot::RwLock;
use std::collections::HashMap;

/// An in-memory, thread-safe searchable index of bibliographic entries.
pub struct CitationIndex {
    entries: RwLock<HashMap<String, Entry>>,
}

impl Default for CitationIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl CitationIndex {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Insert or update an entry by its citation key.
    pub fn insert(&self, entry: Entry) {
        self.entries.write().insert(entry.key.clone(), entry);
    }

    /// Insert a batch of entries, returning the number of entries inserted.
    pub fn insert_batch(&self, entries: impl IntoIterator<Item = Entry>) -> usize {
        let mut map = self.entries.write();
        let mut count = 0;
        for entry in entries {
            map.insert(entry.key.clone(), entry);
            count += 1;
        }
        count
    }

    /// Get an entry by its citation key.
    pub fn get(&self, key: &str) -> Option<Entry> {
        self.entries.read().get(key).cloned()
    }

    /// Check if the index contains a key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.read().contains_key(key)
    }

    /// Number of entries currently indexed.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Remove all entries from the index.
    pub fn clear(&self) {
        self.entries.write().clear();
    }

    /// List all entries currently in the index, sorted by key.
    pub fn list(&self) -> Vec<Entry> {
        let map = self.entries.read();
        let mut list: Vec<_> = map.values().cloned().collect();
        list.sort_by(|a, b| a.key.cmp(&b.key));
        list
    }

    /// Search the index across key, title, author, year, and fuzzy content.
    pub fn search(&self, query: &str, limit: Option<usize>) -> Vec<SearchResult> {
        let query_trimmed = query.trim();
        let limit = limit.unwrap_or(20);
        let map = self.entries.read();

        if query_trimmed.is_empty() {
            let mut all: Vec<_> = map
                .values()
                .map(|e| SearchResult {
                    key: e.key.clone(),
                    score: 1.0,
                    entry: e.clone(),
                })
                .collect();
            all.sort_by(|a, b| a.key.cmp(&b.key));
            all.truncate(limit);
            return all;
        }

        let q_lower = query_trimmed.to_lowercase();
        let q_terms: Vec<&str> = q_lower.split_whitespace().collect();

        let mut results = Vec::new();

        for entry in map.values() {
            let mut score = 0.0;

            let key_lower = entry.key.to_lowercase();
            if key_lower == q_lower {
                score += 150.0;
            } else if key_lower.contains(&q_lower) {
                score += 80.0;
            }

            // Title matching
            if let Some(title) = &entry.title {
                let t_lower = title.to_lowercase();
                if t_lower.contains(&q_lower) {
                    score += 90.0;
                }
                for term in &q_terms {
                    if t_lower.contains(term) {
                        score += 30.0;
                    }
                }
                if let Some(sub_score) = fuzzy_subsequence(&q_lower, &t_lower) {
                    score += sub_score * 20.0;
                }
            }

            // Author matching
            for author in &entry.authors {
                let a_lower = author.name.to_lowercase();
                if a_lower.contains(&q_lower) {
                    score += 80.0;
                }
                if let Some(family) = &author.family {
                    let fam_lower = family.to_lowercase();
                    if fam_lower == q_lower {
                        score += 70.0;
                    } else if fam_lower.contains(&q_lower) {
                        score += 40.0;
                    }
                    for term in &q_terms {
                        if fam_lower.contains(term) {
                            score += 35.0;
                        }
                    }
                }
                for term in &q_terms {
                    if a_lower.contains(term) {
                        score += 25.0;
                    }
                }
            }

            // Year matching
            if let Some(year) = entry.year {
                let year_str = year.to_string();
                if year_str == q_lower {
                    score += 60.0;
                } else if q_terms.contains(&year_str.as_str()) {
                    score += 40.0;
                }
            }

            // Fuzzy key match bonus
            if let Some(sub_score) = fuzzy_subsequence(&q_lower, &key_lower) {
                score += sub_score * 15.0;
            }

            if score > 0.0 {
                results.push(SearchResult {
                    key: entry.key.clone(),
                    score,
                    entry: entry.clone(),
                });
            }
        }

        // Sort descending by score, tie-break ascending by key for determinism
        results.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));

        results.truncate(limit);
        results
    }
}

/// Computes a fuzzy matching score between 0.0 and 1.0 if `pattern` is a subsequence of `target`.
fn fuzzy_subsequence(pattern: &str, target: &str) -> Option<f64> {
    if pattern.is_empty() || target.is_empty() {
        return None;
    }
    let mut pat_chars = pattern.chars();
    let mut current_pat = pat_chars.next()?;
    let mut matched_count = 0;
    let mut consecutive = 0;
    let mut max_consecutive = 0;

    for target_char in target.chars() {
        if target_char == current_pat {
            matched_count += 1;
            consecutive += 1;
            max_consecutive = max_consecutive.max(consecutive);
            if let Some(next) = pat_chars.next() {
                current_pat = next;
            } else {
                // Entire pattern matched
                let ratio = matched_count as f64 / target.chars().count() as f64;
                let consecutive_bonus = max_consecutive as f64 / pattern.chars().count() as f64;
                return Some(0.5 * ratio + 0.5 * consecutive_bonus);
            }
        } else {
            consecutive = 0;
        }
    }

    None
}
