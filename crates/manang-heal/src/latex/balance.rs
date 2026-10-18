//! Unbalanced braces and environments, when one edit is the only sensible fix.
//!
//! Where a missing `}` or `\end{..}` belongs is a judgement, so these fixes
//! are `likely`: the user sees the exact edit and chooses. The rules stay
//! silent when the document has more than one imbalance.

use crate::engine::{ByteEdit, Draft};
use crate::types::{Confidence, DiagnosticIn};

use super::Env;
use super::scan::{args, commands};

const RULE_BRACE: &str = "latex.unbalanced-brace";
const RULE_ENV: &str = "latex.unbalanced-environment";

/// Message starts that point at a missing or extra closer.
const TRIGGERS: &[&str] = &[
    "File ended while scanning",
    "Runaway argument",
    "Paragraph ended before",
    "Missing } inserted",
    "Missing { inserted",
    "Extra }",
    "Too many }",
    "Emergency stop",
    "no legal \\end found",
];

pub(super) fn unbalanced(env: &Env<'_>, index: usize, d: &DiagnosticIn) -> Vec<Draft> {
    let message = d.message.as_str();
    let ended_by = message.contains("\\begin{") && message.contains("ended by");
    let end_occurred = message.contains("\\end occurred");
    let triggered = ended_by || end_occurred || TRIGGERS.iter().any(|t| message.contains(t));
    if !triggered {
        return Vec::new();
    }
    let mut out = environments(env, index);
    if out.is_empty() {
        out.extend(braces(env, index));
    }
    out
}

#[derive(Debug, Clone)]
enum Event {
    Begin { at: usize, name: String },
    End { at: usize, name: String },
}

fn events(env: &Env<'_>) -> Vec<Event> {
    let masked = env.masked.as_str();
    let mut out = Vec::new();
    for (at, name) in commands(masked) {
        if name != "\\begin" && name != "\\end" {
            continue;
        }
        let a = args(masked, at + name.len());
        let Some((s, e)) = a.required else { continue };
        let env_name = masked[s..e].trim().to_owned();
        if name == "\\begin" {
            out.push(Event::Begin { at, name: env_name });
        } else {
            out.push(Event::End { at, name: env_name });
        }
    }
    out
}

/// The outcome of replaying the events with one repair applied.
fn balanced_from(events: &[Event], mut stack: Vec<String>) -> bool {
    for event in events {
        match event {
            Event::Begin { name, .. } => stack.push(name.clone()),
            Event::End { name, .. } => {
                if stack.last() == Some(name) {
                    stack.pop();
                } else {
                    return false;
                }
            }
        }
    }
    stack.is_empty()
}

fn environments(env: &Env<'_>, index: usize) -> Vec<Draft> {
    let events = events(env);
    let mut stack: Vec<(usize, String)> = Vec::new();
    for (i, event) in events.iter().enumerate() {
        match event {
            Event::Begin { at, name } => stack.push((*at, name.clone())),
            Event::End { at, name, .. } => {
                let Some((begin_at, top)) = stack.last().cloned() else {
                    // An `\end` with nothing open: which `\begin` is missing
                    // is not decidable.
                    return Vec::new();
                };
                if &top == name {
                    stack.pop();
                    continue;
                }
                return mismatch(env, index, &events, i, &stack, (begin_at, top), (*at, name));
            }
        }
    }
    // The text ended with environments open.
    match stack.as_slice() {
        [(_, name)] => {
            let eol = env.eol();
            let text = env.text();
            let lead = if text.ends_with('\n') { "" } else { eol };
            let tail = if name == "document" { eol } else { "" };
            let insert = format!("{lead}\\end{{{name}}}{tail}");
            vec![Draft {
                id: format!("latex.end:{name}:eof"),
                label: format!("Add \\end{{{name}}} at the end"),
                why: format!("`\\begin{{{name}}}` is never closed."),
                edits: vec![ByteEdit::insert(text.len(), insert)],
                // Closing the document at its end has one reading.
                confidence: if name == "document" {
                    Confidence::Certain
                } else {
                    Confidence::Likely
                },
                rule: RULE_ENV,
                diagnostics: vec![index],
                finding: None,
            }]
        }
        _ => Vec::new(),
    }
}

/// An `\end{found}` while `expected` is open. Three repairs, each offered
/// only if the rest of the document then balances.
fn mismatch(
    env: &Env<'_>,
    index: usize,
    events: &[Event],
    at_event: usize,
    stack: &[(usize, String)],
    (begin_at, expected): (usize, String),
    (end_at, found): (usize, &String),
) -> Vec<Draft> {
    let rest = &events[at_event + 1..];
    let names: Vec<String> = stack.iter().map(|(_, n)| n.clone()).collect();
    let mut out = Vec::new();
    let text = env.text();
    let masked = env.masked.as_str();

    // 1. The `\end` has the wrong name.
    let mut after = names.clone();
    after.pop();
    if balanced_from(rest, after) {
        let a = args(masked, end_at + "\\end".len());
        if let Some((s, e)) = a.required {
            out.push(Draft {
                id: format!("latex.end-rename:{found}->{expected}:{end_at}"),
                label: format!("Change \\end{{{found}}} to \\end{{{expected}}}"),
                why: format!(
                    "`\\begin{{{expected}}}` is still open where `\\end{{{found}}}` closes."
                ),
                edits: vec![ByteEdit::replace(s, e, expected.clone())],
                confidence: Confidence::Likely,
                rule: RULE_ENV,
                diagnostics: vec![index],
                finding: None,
            });
        }
    }
    // 2. The `\begin` has the wrong name.
    let mut after = names.clone();
    after.pop();
    if balanced_from(rest, after) {
        let a = args(masked, begin_at + "\\begin".len());
        if let Some((s, e)) = a.required {
            out.push(Draft {
                id: format!("latex.begin-rename:{expected}->{found}:{begin_at}"),
                label: format!("Change \\begin{{{expected}}} to \\begin{{{found}}}"),
                why: format!(
                    "`\\end{{{found}}}` closes an environment that was opened as `{expected}`."
                ),
                edits: vec![ByteEdit::replace(s, e, found.clone())],
                confidence: Confidence::Likely,
                rule: RULE_ENV,
                diagnostics: vec![index],
                finding: None,
            });
        }
    }
    // 3. The environments above `found` were never closed: close them first.
    if let Some(depth) = names.iter().rposition(|n| n == found) {
        let unclosed: Vec<&String> = names[depth + 1..].iter().rev().collect();
        let after = names[..depth].to_vec();
        if !unclosed.is_empty() && balanced_from(rest, after) {
            let line_start = text[..end_at].rfind('\n').map_or(0, |p| p + 1);
            let at_line_start = text[line_start..end_at].trim().is_empty();
            let eol = env.eol();
            let closers: String = unclosed
                .iter()
                .map(|n| {
                    if at_line_start {
                        format!("\\end{{{n}}}{eol}")
                    } else {
                        format!("\\end{{{n}}}")
                    }
                })
                .collect();
            let at = if at_line_start { line_start } else { end_at };
            let listed = unclosed
                .iter()
                .map(|n| format!("\\end{{{n}}}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(Draft {
                id: format!("latex.end-insert:{listed}:{end_at}"),
                label: format!("Add {listed} before \\end{{{found}}}"),
                why: format!(
                    "`\\end{{{found}}}` is reached while {} still open.",
                    if unclosed.len() == 1 {
                        "an environment is"
                    } else {
                        "environments are"
                    }
                ),
                edits: vec![ByteEdit::insert(at, closers)],
                confidence: Confidence::Likely,
                rule: RULE_ENV,
                diagnostics: vec![index],
                finding: None,
            });
        }
    }
    out
}

/// Exactly one stray `{` or `}` in the whole document.
fn braces(env: &Env<'_>, index: usize) -> Vec<Draft> {
    let bytes = env.masked.as_bytes();
    let mut open: Vec<usize> = Vec::new();
    let mut extra: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'{' => open.push(i),
            b'}' => {
                if open.pop().is_none() {
                    extra.push(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    let text = env.text();
    match (open.as_slice(), extra.as_slice()) {
        ([at], []) => {
            let line_end = env.masked[*at..]
                .find(['\n', '\r'])
                .map_or(text.len(), |p| at + p);
            let insert_at = at + env.masked[*at..line_end].trim_end().len();
            let line = env.ctx.lines.line_of(*at);
            vec![Draft {
                id: format!("latex.close-brace:{insert_at}"),
                label: format!("Add the missing }} at the end of line {line}"),
                why: "A `{` is never closed; this closes it at the end of the line it opens on."
                    .to_owned(),
                edits: vec![ByteEdit::insert(insert_at, "}")],
                confidence: Confidence::Likely,
                rule: RULE_BRACE,
                diagnostics: vec![index],
                finding: None,
            }]
        }
        ([], [at]) => {
            let line = env.ctx.lines.line_of(*at);
            vec![Draft {
                id: format!("latex.remove-brace:{at}"),
                label: format!("Remove the extra }} on line {line}"),
                why: "A `}` closes nothing.".to_owned(),
                edits: vec![ByteEdit::replace(*at, at + 1, "")],
                confidence: Confidence::Likely,
                rule: RULE_BRACE,
                diagnostics: vec![index],
                finding: None,
            }]
        }
        _ => Vec::new(),
    }
}
