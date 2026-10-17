//! Replace preview and atomic apply.
//!
//! A preview reads each candidate file once, records the BLAKE3 hash of what
//! it read, and computes the edits as byte ranges plus the replacement text.
//! Nothing is written. `apply` re-reads each file, refuses it when the hash
//! no longer matches (the user or another tool edited it in the meantime),
//! rebuilds the content from the recorded edits and replaces the file through
//! a temp file + rename ([`manang_project::fsops::atomic_write`]). Every path
//! is re-resolved through [`ProjectRoot::resolve_for_write`], so symlink
//! escapes, `..` and `.git` are refused at apply time regardless of what the
//! plan holds.
//!
//! Only valid UTF-8 text files without NUL bytes are ever planned; anything
//! else is skipped so a replace can never corrupt a binary or legacy-encoded
//! file.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use grep_matcher::{Captures, Matcher};
use grep_regex::RegexMatcher;
use manang_project::{ProjectRoot, fsops};
use parking_lot::Mutex;
use rayon::ThreadPool;
use rayon::prelude::*;
use serde::Serialize;

use crate::budget::{Budget, StopReason};
use crate::error::SearchError;
use crate::index::{FileEntry, FileIndex};
use crate::intext::CharCursor;
use crate::limits::Limits;
use crate::pattern::PathFilter;
use crate::scan::{Counters, Opened, Stats, open, scan};

/// Plans kept for a later `apply`.
const MAX_PLANS: usize = 8;
/// A plan not applied within this long is dropped.
const PLAN_TTL: Duration = Duration::from_secs(15 * 60);
/// Largest file `apply` will write.
const MAX_WRITE_BYTES: usize = 64 * 1024 * 1024;

/// What to substitute for each match.
#[derive(Debug, Clone)]
pub struct Replacement {
    pub text: String,
    /// Expand `$1`, `${name}` and `$$` (regex mode only).
    pub expand: bool,
}

/// One planned substitution, in file byte coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRec {
    pub byte_start: usize,
    pub byte_end: usize,
    pub replacement: String,
}

/// The planned edits of one file plus the hash they were computed against.
#[derive(Debug, Clone)]
pub struct PlanFile {
    pub path: String,
    pub hash: String,
    pub edits: Vec<EditRec>,
}

/// A stored, not-yet-applied replacement plan.
#[derive(Debug)]
pub struct Plan {
    pub id: String,
    pub root: ProjectRoot,
    pub files: Vec<PlanFile>,
    created: Instant,
}

/// One edit as shown in the preview UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewEdit {
    /// 1-based.
    pub line: usize,
    /// 1-based scalar column.
    pub column: usize,
    /// Scalar range within the line.
    pub start: usize,
    pub end: usize,
    /// Scalar range within the whole file.
    pub offset_start: usize,
    pub offset_end: usize,
    pub matched: String,
    pub replacement: String,
    pub line_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewFile {
    pub path: String,
    /// BLAKE3 of the content the edits were computed against.
    pub hash: String,
    pub edits: Vec<PreviewEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewOutcome {
    pub files: Vec<PreviewFile>,
    pub total_edits: usize,
    /// Edits were dropped by a cap, the time limit or a cancellation; the
    /// plan is only partial.
    pub truncated: bool,
    pub cancelled: bool,
    pub stopped: Option<StopReason>,
    pub stats: Stats,
}

/// Inputs shared by every file of one preview.
pub struct PreviewJob<'a> {
    pub pool: &'a ThreadPool,
    pub root: &'a ProjectRoot,
    pub index: &'a FileIndex,
    pub filter: &'a PathFilter,
    pub limits: &'a Limits,
    pub budget: &'a Budget,
    pub replacement: &'a Replacement,
}

fn trim_eol(mut line: &[u8]) -> &[u8] {
    if let Some(r) = line.strip_suffix(b"\n") {
        line = r;
    }
    if let Some(r) = line.strip_suffix(b"\r") {
        line = r;
    }
    line
}

fn plan_file(
    matcher: &RegexMatcher,
    job: &PreviewJob<'_>,
    counters: &Counters,
    entry: &FileEntry,
) -> Option<(PlanFile, PreviewFile)> {
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
    let mut bytes = Vec::new();
    if file
        .take(job.limits.max_file_bytes + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        Counters::bump(&counters.unreadable);
        return None;
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(t) if !bytes.contains(&0) => t,
        _ => {
            Counters::bump(&counters.binary);
            return None;
        }
    };
    let Ok(mut caps) = matcher.new_captures() else {
        return None;
    };

    let mut cursor = CharCursor::new(text);
    let mut edits: Vec<EditRec> = Vec::new();
    let mut shown: Vec<PreviewEdit> = Vec::new();
    let mut line_start_byte = 0usize;
    let mut out_of_budget = false;

    for raw in text.as_bytes().split_inclusive(|b| *b == b'\n') {
        let line = trim_eol(raw);
        let base = line_start_byte;
        line_start_byte += raw.len();
        if out_of_budget || job.budget.should_stop() {
            break;
        }
        let line_text = String::from_utf8_lossy(line);
        // `RegexMatcher`'s error type is `NoError`.
        let _ = matcher.captures_iter(line, &mut caps, |c| {
            let Some(m) = c.get(0) else { return true };
            if edits.len() >= job.limits.max_edits || !job.budget.claim_result() {
                out_of_budget = true;
                return false;
            }
            let mut dst: Vec<u8> = Vec::new();
            if job.replacement.expand {
                c.interpolate(
                    |name| matcher.capture_index(name),
                    line,
                    job.replacement.text.as_bytes(),
                    &mut dst,
                );
            } else {
                dst.extend_from_slice(job.replacement.text.as_bytes());
            }
            let replacement = String::from_utf8_lossy(&dst).into_owned();
            let (Some(os), Some(oe)) = (
                cursor.advance(base + m.start()),
                cursor.advance(base + m.end()),
            ) else {
                return true;
            };
            let line_no = cursor.line();
            let line_start = cursor.line_start();
            edits.push(EditRec {
                byte_start: base + m.start(),
                byte_end: base + m.end(),
                replacement: replacement.clone(),
            });
            shown.push(PreviewEdit {
                line: line_no,
                column: os - line_start + 1,
                start: os - line_start,
                end: oe - line_start,
                offset_start: os,
                offset_end: oe,
                matched: line
                    .get(m.start()..m.end())
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_default(),
                replacement,
                line_text: line_text.chars().take(job.limits.max_line_chars).collect(),
            });
            true
        });
    }
    if edits.is_empty() {
        return None;
    }
    Counters::bump(&counters.matched_files);
    let hash = blake3::hash(&bytes).to_hex().to_string();
    Some((
        PlanFile {
            path: entry.path.clone(),
            hash: hash.clone(),
            edits,
        },
        PreviewFile {
            path: entry.path.clone(),
            hash,
            edits: shown,
        },
    ))
}

/// Compute the planned edits. Writes nothing.
pub fn preview(job: &PreviewJob<'_>, matcher: &RegexMatcher) -> (Vec<PlanFile>, PreviewOutcome) {
    let started = Instant::now();
    let counters = Counters::default();
    let planned = scan(
        job.pool,
        &job.index.entries,
        job.filter,
        job.budget,
        || (),
        |(), entry| plan_file(matcher, job, &counters, entry),
    );
    let (plan_files, files): (Vec<PlanFile>, Vec<PreviewFile>) = planned.into_iter().unzip();
    let total_edits = files.iter().map(|f| f.edits.len()).sum();
    let stopped = job.budget.reason();
    let outcome = PreviewOutcome {
        files,
        total_edits,
        truncated: stopped.is_some(),
        cancelled: stopped == Some(StopReason::Cancelled),
        stopped,
        stats: counters.snapshot(
            job.budget.bytes_scanned(),
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        ),
    };
    (plan_files, outcome)
}

/// Result of applying one file of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyResult {
    pub path: String,
    pub ok: bool,
    pub replacements: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn apply_file(root: &ProjectRoot, file: &PlanFile, limits: &Limits) -> Result<usize, String> {
    let abs = root
        .resolve_for_write(&file.path)
        .map_err(|e| e.to_string())?;
    let handle = std::fs::File::open(&abs).map_err(|e| format!("cannot read the file: {e}"))?;
    let mut bytes = Vec::new();
    handle
        .take(limits.max_file_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read the file: {e}"))?;
    if blake3::hash(&bytes).to_hex().as_str() != file.hash {
        return Err("the file changed since the preview; search again".to_owned());
    }

    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut cursor = 0usize;
    for edit in &file.edits {
        let (Some(before), true) = (
            bytes.get(cursor..edit.byte_start),
            edit.byte_start <= edit.byte_end && edit.byte_end <= bytes.len(),
        ) else {
            return Err("the plan is inconsistent with the file".to_owned());
        };
        out.extend_from_slice(before);
        out.extend_from_slice(edit.replacement.as_bytes());
        cursor = edit.byte_end;
    }
    out.extend_from_slice(bytes.get(cursor..).unwrap_or_default());
    if out.len() > MAX_WRITE_BYTES {
        return Err("the result would be too large".to_owned());
    }
    fsops::atomic_write(&abs, &out).map_err(|e| e.to_string())?;
    Ok(file.edits.len())
}

/// Apply `plan` (optionally only `only` paths), one atomic write per file.
pub fn apply(
    pool: &ThreadPool,
    plan: &Plan,
    only: Option<&[String]>,
    limits: &Limits,
) -> Vec<ApplyResult> {
    pool.install(|| {
        plan.files
            .par_iter()
            .filter(|f| only.is_none_or(|o| o.iter().any(|p| p == &f.path)))
            .map(|f| match apply_file(&plan.root, f, limits) {
                Ok(n) => ApplyResult {
                    path: f.path.clone(),
                    ok: true,
                    replacements: n,
                    error: None,
                },
                Err(e) => ApplyResult {
                    path: f.path.clone(),
                    ok: false,
                    replacements: 0,
                    error: Some(e),
                },
            })
            .collect()
    })
}

/// Bounded store of pending plans.
#[derive(Default)]
pub struct PlanStore {
    next: AtomicU64,
    plans: Mutex<VecDeque<Plan>>,
}

impl PlanStore {
    /// Store a plan and return its id.
    pub fn insert(&self, root: ProjectRoot, files: Vec<PlanFile>) -> String {
        let id = format!("plan-{}", self.next.fetch_add(1, Ordering::Relaxed) + 1);
        let mut plans = self.plans.lock();
        plans.retain(|p| p.created.elapsed() < PLAN_TTL);
        while plans.len() >= MAX_PLANS {
            plans.pop_front();
        }
        plans.push_back(Plan {
            id: id.clone(),
            root,
            files,
            created: Instant::now(),
        });
        id
    }

    /// Remove and return a plan; plans are single-use.
    pub fn take(&self, id: &str) -> Result<Plan, SearchError> {
        let mut plans = self.plans.lock();
        let pos = plans
            .iter()
            .position(|p| p.id == id && p.created.elapsed() < PLAN_TTL)
            .ok_or_else(|| SearchError::UnknownPlan(id.chars().take(40).collect()))?;
        plans
            .remove(pos)
            .ok_or_else(|| SearchError::UnknownPlan(id.chars().take(40).collect()))
    }
}
