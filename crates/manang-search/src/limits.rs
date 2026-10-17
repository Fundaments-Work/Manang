//! Hard caps that keep a single request from monopolising the host.

use std::time::Duration;

/// Resource limits for one plugin instance. Requests may ask for less, never
/// more.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Default number of matching lines / file hits returned.
    pub default_results: usize,
    /// Upper bound a request can raise `limit` to.
    pub max_results: usize,
    /// Total bytes a single text search may read.
    pub max_bytes_scanned: u64,
    /// Files larger than this are skipped (and counted).
    pub max_file_bytes: u64,
    /// Wall-clock budget of one text search or replace preview.
    pub max_duration: Duration,
    /// Largest `context_lines` honoured.
    pub max_context: usize,
    /// Matching lines kept per file.
    pub max_matches_per_file: usize,
    /// Longest query / pattern accepted.
    pub max_pattern_bytes: usize,
    /// Compiled regex size cap (bytes).
    pub regex_size_limit: usize,
    /// Lazy DFA cache cap (bytes).
    pub regex_dfa_limit: usize,
    /// Regex nesting depth cap.
    pub regex_nest_limit: u32,
    /// Files kept in a per-root index.
    pub max_index_entries: usize,
    /// Include / exclude globs accepted per request.
    pub max_globs: usize,
    /// Edits a single replace plan may hold.
    pub max_edits: usize,
    /// Longest replacement string.
    pub max_replacement_bytes: usize,
    /// Per-line text sent back to the UI is clipped to this many chars.
    pub max_line_chars: usize,
    /// Matches reported per line.
    pub max_matches_per_line: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            default_results: 1_000,
            max_results: 10_000,
            max_bytes_scanned: 512 * 1024 * 1024,
            max_file_bytes: 8 * 1024 * 1024,
            max_duration: Duration::from_secs(5),
            max_context: 10,
            max_matches_per_file: 200,
            max_pattern_bytes: 4_096,
            regex_size_limit: 2 * 1024 * 1024,
            regex_dfa_limit: 8 * 1024 * 1024,
            regex_nest_limit: 64,
            max_index_entries: 500_000,
            max_globs: 64,
            max_edits: 20_000,
            max_replacement_bytes: 64 * 1024,
            max_line_chars: 1_000,
            max_matches_per_line: 100,
        }
    }
}

impl Limits {
    /// Clamp a requested result limit to `1..=max_results`.
    pub fn clamp_results(&self, requested: Option<usize>) -> usize {
        requested
            .unwrap_or(self.default_results)
            .clamp(1, self.max_results)
    }
}
