//! Project-wide text search on `grep-searcher` / `grep-regex`.
//!
//! Offsets reported to the UI are Unicode-scalar indices *within the line*
//! (the editor adds the line start from its own rope), plus the line's
//! absolute byte offset for tools that want it. Lines are decoded lossily, so
//! a Latin-1 file still searches; matches are located on the decoded text so
//! the reported offsets always agree with the reported text.

use std::io::{self, Read};
use std::time::Instant;

use grep_matcher::{LineTerminator, Matcher};
use grep_regex::RegexMatcher;
use grep_searcher::{
    BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind, SinkFinish,
    SinkMatch,
};
use manang_project::ProjectRoot;
use rayon::ThreadPool;
use serde::Serialize;

use crate::budget::{Budget, StopReason};
use crate::index::{FileEntry, FileIndex};
use crate::intext::CharCursor;
use crate::limits::Limits;
use crate::pattern::PathFilter;
use crate::scan::{Counters, Opened, Stats, open, scan};

/// A match span inside a line, in Unicode scalars, end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextLine {
    /// 1-based.
    pub line: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineMatch {
    /// 1-based line number.
    pub line: u64,
    /// 1-based scalar column of the first match.
    pub column: usize,
    /// Absolute byte offset of the line start in the file.
    pub byte_offset: u64,
    pub text: String,
    /// Scalars cut from the front of an overlong line (0 when not clipped).
    #[serde(skip_serializing_if = "is_zero")]
    pub text_offset: usize,
    pub ranges: Vec<Range>,
    pub before: Vec<ContextLine>,
    pub after: Vec<ContextLine>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileMatches {
    pub path: String,
    pub matches: Vec<LineMatch>,
    /// More matching lines exist in this file than were kept.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextOutcome {
    pub files: Vec<FileMatches>,
    /// Matching lines returned.
    pub total_matches: usize,
    /// Individual match ranges returned.
    pub total_ranges: usize,
    /// The result is incomplete (cap, time, byte budget or cancellation).
    pub truncated: bool,
    pub cancelled: bool,
    pub stopped: Option<StopReason>,
    pub stats: Stats,
}

/// Everything a text search needs besides the matcher.
pub struct TextJob<'a> {
    pub pool: &'a ThreadPool,
    pub root: &'a ProjectRoot,
    pub index: &'a FileIndex,
    pub filter: &'a PathFilter,
    pub limits: &'a Limits,
    pub budget: &'a Budget,
    pub context_lines: usize,
}

fn trim_eol(mut line: &[u8]) -> &[u8] {
    if let Some(rest) = line.strip_suffix(b"\n") {
        line = rest;
    }
    if let Some(rest) = line.strip_suffix(b"\r") {
        line = rest;
    }
    line
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_owned();
    }
    text.chars().take(max_chars).collect()
}

/// Decode one matched line and locate the matches in it.
fn describe(
    matcher: &RegexMatcher,
    raw: &[u8],
    limits: &Limits,
) -> Option<(String, usize, Vec<Range>)> {
    let text = String::from_utf8_lossy(trim_eol(raw));
    let mut spans: Vec<(usize, usize)> = Vec::new();
    // `RegexMatcher`'s error type is `NoError`.
    let _ = matcher.find_iter(text.as_bytes(), |m| {
        if !m.is_empty() {
            spans.push((m.start(), m.end()));
        }
        spans.len() < limits.max_matches_per_line
    });
    if spans.is_empty() {
        return None;
    }
    let mut cursor = CharCursor::new(&text);
    let mut ranges = Vec::with_capacity(spans.len());
    for (s, e) in spans {
        let (Some(start), Some(end)) = (cursor.advance(s), cursor.advance(e)) else {
            continue;
        };
        ranges.push(Range { start, end });
    }
    if ranges.is_empty() {
        return None;
    }

    if text.len() <= limits.max_line_chars {
        return Some((text.into_owned(), 0, ranges));
    }
    // Overlong (minified) line: send a window around the first match.
    let first = ranges.first().map_or(0, |r| r.start);
    let from = first.saturating_sub(limits.max_line_chars / 10);
    let to = from + limits.max_line_chars;
    let window: String = text
        .chars()
        .skip(from)
        .take(limits.max_line_chars)
        .collect();
    let ranges: Vec<Range> = ranges
        .into_iter()
        .filter(|r| r.start >= from && r.end <= to)
        .map(|r| Range {
            start: r.start - from,
            end: r.end - from,
        })
        .collect();
    Some((window, from, ranges))
}

struct LineSink<'a> {
    matcher: &'a RegexMatcher,
    budget: &'a Budget,
    limits: &'a Limits,
    out: Vec<LineMatch>,
    pending_before: Vec<ContextLine>,
    binary: bool,
    truncated: bool,
}

impl Sink for LineSink<'_> {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        if self.budget.should_stop() {
            return Ok(false);
        }
        if self.out.len() >= self.limits.max_matches_per_file {
            self.truncated = true;
            return Ok(false);
        }
        let Some((text, text_offset, ranges)) = describe(self.matcher, mat.bytes(), self.limits)
        else {
            return Ok(true);
        };
        if !self.budget.claim_result() {
            self.truncated = true;
            return Ok(false);
        }
        let column = ranges.first().map_or(1, |r| r.start + text_offset + 1);
        self.out.push(LineMatch {
            line: mat.line_number().unwrap_or(0),
            column,
            byte_offset: mat.absolute_byte_offset(),
            text,
            text_offset,
            ranges,
            before: std::mem::take(&mut self.pending_before),
            after: Vec::new(),
        });
        Ok(true)
    }

    fn context(&mut self, _: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, io::Error> {
        let line = ContextLine {
            line: ctx.line_number().unwrap_or(0),
            text: clip(
                &String::from_utf8_lossy(trim_eol(ctx.bytes())),
                self.limits.max_line_chars,
            ),
        };
        match ctx.kind() {
            SinkContextKind::Before => self.pending_before.push(line),
            SinkContextKind::After => {
                if let Some(last) = self.out.last_mut() {
                    last.after.push(line);
                }
            }
            SinkContextKind::Other => {}
        }
        Ok(true)
    }

    fn context_break(&mut self, _: &Searcher) -> Result<bool, io::Error> {
        self.pending_before.clear();
        Ok(true)
    }

    fn finish(&mut self, _: &Searcher, fin: &SinkFinish) -> Result<(), io::Error> {
        self.binary = fin.binary_byte_offset().is_some();
        Ok(())
    }
}

fn new_searcher(context: usize) -> Searcher {
    let mut b = SearcherBuilder::new();
    b.binary_detection(BinaryDetection::quit(0))
        .line_terminator(LineTerminator::crlf())
        .line_number(true)
        .before_context(context)
        .after_context(context);
    b.build()
}

fn search_file(
    searcher: &mut Searcher,
    matcher: &RegexMatcher,
    job: &TextJob<'_>,
    counters: &Counters,
    entry: &FileEntry,
) -> Option<FileMatches> {
    let file = match open(job.root, entry, job.budget, job.limits) {
        Opened::Ready(f) => f,
        Opened::TooLarge => {
            Counters::bump(&counters.large);
            return None;
        }
        Opened::OverBudget => return None,
        Opened::Unreadable => {
            Counters::bump(&counters.unreadable);
            return None;
        }
    };
    Counters::bump(&counters.searched);
    let mut sink = LineSink {
        matcher,
        budget: job.budget,
        limits: job.limits,
        out: Vec::new(),
        pending_before: Vec::new(),
        binary: false,
        truncated: false,
    };
    // `take` bounds the read if the file grew after the size check.
    let reader = file.take(job.limits.max_file_bytes);
    if searcher.search_reader(matcher, reader, &mut sink).is_err() {
        Counters::bump(&counters.unreadable);
        return None;
    }
    if sink.binary {
        Counters::bump(&counters.binary);
        return None;
    }
    if sink.out.is_empty() {
        return None;
    }
    Counters::bump(&counters.matched_files);
    Some(FileMatches {
        path: entry.path.clone(),
        matches: sink.out,
        truncated: sink.truncated,
    })
}

/// Run a project-wide search with `matcher`.
pub fn run(job: &TextJob<'_>, matcher: &RegexMatcher) -> TextOutcome {
    let started = Instant::now();
    let counters = Counters::default();
    let context = job.context_lines.min(job.limits.max_context);

    let files: Vec<FileMatches> = scan(
        job.pool,
        &job.index.entries,
        job.filter,
        job.budget,
        || new_searcher(context),
        |searcher, entry| search_file(searcher, matcher, job, &counters, entry),
    );

    let total_matches = files.iter().map(|f| f.matches.len()).sum();
    let total_ranges = files
        .iter()
        .flat_map(|f| &f.matches)
        .map(|m| m.ranges.len())
        .sum();
    let stopped = job.budget.reason();
    let truncated = stopped.is_some() || files.iter().any(|f| f.truncated);
    TextOutcome {
        files,
        total_matches,
        total_ranges,
        truncated,
        cancelled: stopped == Some(StopReason::Cancelled),
        stopped,
        stats: counters.snapshot(
            job.budget.bytes_scanned(),
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        ),
    }
}
