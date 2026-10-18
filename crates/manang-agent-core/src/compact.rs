//! Context compaction: pure functions over the request view of a conversation.
//!
//! The on-disk transcript is lossless and never touched here; only the view that is sent to the
//! provider changes, and it changes rarely (each rewrite breaks the provider's cached prefix).
//!
//! * Stage 1, at 60% of the working window: results of old tool calls become one-line stubs
//!   (`[read_file main.typ 1-400: cleared]`), except the last few. Edits and the author's own
//!   messages are never cleared.
//! * Stage 2, at 85%: everything before the author's last two messages is summarised by the
//!   model into a short note; the view becomes `header + summary + the last two turns`.

use crate::block::{BlockKind, ContentBlock, Message, Role};
use serde_json::Value;
use std::collections::HashMap;

/// Stage 1 starts at this share of the working window (percent).
pub const STAGE1_PERCENT: u64 = 60;
/// Stage 2 starts at this share (percent).
pub const STAGE2_PERCENT: u64 = 85;
/// Tool results that stay whole, counted from the newest.
pub const KEEP_RESULTS: usize = 4;
/// Author turns that stay whole after a summary.
pub const KEEP_TURNS: usize = 2;
/// Most tokens the summary note may take.
pub const SUMMARY_TOKENS: u32 = 1_500;
/// Tools whose results are never cleared: the author's decisions hang on them.
const PROTECTED: [&str; 2] = ["edit_text", "ask_user"];

/// What to do about the current fill level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compaction {
    None,
    /// Stage 1: clear old tool results.
    ClearResults,
    /// Stage 2: summarise the older conversation.
    Summarise,
}

/// Chooses the stage for `fill` tokens in a window of `window` tokens. `cleared` says whether
/// stage 1 already ran (it runs once).
pub fn decide(fill: u64, window: u64, cleared: bool) -> Compaction {
    if window == 0 {
        return Compaction::None;
    }
    let scaled = fill.saturating_mul(100);
    if scaled >= window.saturating_mul(STAGE2_PERCENT) {
        Compaction::Summarise
    } else if !cleared && scaled >= window.saturating_mul(STAGE1_PERCENT) {
        Compaction::ClearResults
    } else {
        Compaction::None
    }
}

/// Fill as a whole percent (0 when the window is unknown).
pub fn fill_percent(fill: u64, window: u64) -> u64 {
    fill.saturating_mul(100).checked_div(window).unwrap_or(0)
}

fn value_of(block: &ContentBlock) -> Option<Value> {
    serde_json::from_str(block.raw_json()).ok()
}

/// A short description of a call for a stub or a summary line: its path, query or directory,
/// plus the line range of a read.
pub fn brief(name: &str, input: &Value) -> String {
    let text = |key: &str| input.get(key).and_then(Value::as_str);
    let mut out = String::new();
    if let Some(v) = text("path").or_else(|| text("query")).or_else(|| text("dir")) {
        let v: String = v.chars().take(60).collect();
        out.push_str(&v);
    }
    if name == "read_file" {
        let from = input.get("from_line").and_then(Value::as_u64);
        let to = input.get("to_line").and_then(Value::as_u64);
        if from.is_some() || to.is_some() {
            out.push_str(&format!(
                " {}-{}",
                from.unwrap_or(1),
                to.map_or_else(|| "end".to_owned(), |t| t.to_string())
            ));
        }
    }
    out
}

fn stub(name: &str, input: &Value) -> String {
    let what = brief(name, input);
    if what.is_empty() {
        format!("[{name}: cleared]")
    } else {
        format!("[{name} {what}: cleared]")
    }
}

/// Where a tool result sits: message index and block index.
struct Slot {
    message: usize,
    block: usize,
    id: String,
}

/// Stage 1. Returns the new view and how many results were cleared. The structure (messages,
/// blocks, tool-call ids) is unchanged, so providers still see every call paired with a result.
pub fn clear_old_results(messages: &[Message], keep: usize) -> (Vec<Message>, usize) {
    let mut calls: HashMap<String, (String, Value)> = HashMap::new();
    let mut slots: Vec<Slot> = Vec::new();
    for (mi, message) in messages.iter().enumerate() {
        for (bi, block) in message.content.iter().enumerate() {
            match block.kind() {
                BlockKind::ToolUse => {
                    if let Some(call) = block.as_tool_use() {
                        calls.insert(call.id, (call.name, call.input));
                    }
                }
                BlockKind::ToolResult => {
                    let id = value_of(block)
                        .and_then(|v| v.get("tool_use_id").and_then(Value::as_str).map(str::to_owned))
                        .unwrap_or_default();
                    slots.push(Slot {
                        message: mi,
                        block: bi,
                        id,
                    });
                }
                _ => {}
            }
        }
    }
    let older = slots.len().saturating_sub(keep);
    let mut out: Vec<Message> = messages.to_vec();
    let mut cleared = 0;
    for slot in &slots[..older] {
        let Some((name, input)) = calls.get(&slot.id) else {
            continue;
        };
        if PROTECTED.contains(&name.as_str()) {
            continue;
        }
        let Some(value) = out
            .get(slot.message)
            .and_then(|m| m.content.get(slot.block))
            .and_then(value_of)
        else {
            continue;
        };
        let text = stub(name, input);
        let current = match value.get("content") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        if current == text || current.chars().count() <= text.chars().count() {
            continue;
        }
        let is_error = value.get("is_error").and_then(Value::as_bool).unwrap_or(false);
        let Ok(block) = ContentBlock::tool_result(&slot.id, &text, is_error) else {
            continue;
        };
        if let Some(target) = out
            .get_mut(slot.message)
            .and_then(|m| m.content.get_mut(slot.block))
        {
            *target = block;
            cleared += 1;
        }
    }
    (out, cleared)
}

/// Whether `message` is something the author wrote (a user message with no tool results).
pub fn is_author_turn(message: &Message) -> bool {
    message.role == Role::User
        && !message.content.is_empty()
        && !message
            .content
            .iter()
            .any(|b| b.kind() == BlockKind::ToolResult)
}

/// Splits the view for stage 2: the older part to summarise and the recent part (the last `keep`
/// author turns and everything after them) to keep whole. `None` when there is nothing older.
pub fn split_for_summary(messages: &[Message], keep: usize) -> Option<(&[Message], &[Message])> {
    let turns: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| is_author_turn(m))
        .map(|(i, _)| i)
        .collect();
    if turns.len() <= keep {
        return None;
    }
    let cut = *turns.get(turns.len() - keep)?;
    (cut > 0).then(|| messages.split_at(cut))
}

/// The conversation as plain text for the summariser: author and assistant text in full, tool
/// calls as one line each, results as their first line. Cut to about `cap` characters from the
/// start (the oldest part is the least detailed anyway).
pub fn render_for_summary(messages: &[Message], cap: usize) -> String {
    let mut names: HashMap<String, String> = HashMap::new();
    let mut out = String::new();
    for message in messages {
        for block in &message.content {
            match block.kind() {
                BlockKind::Text => {
                    let who = if message.role == Role::User {
                        "Author"
                    } else {
                        "Assistant"
                    };
                    if let Some(text) = block.as_text() {
                        out.push_str(&format!("{who}: {text}\n"));
                    }
                }
                BlockKind::ToolUse => {
                    if let Some(call) = block.as_tool_use() {
                        out.push_str(&format!("Tool call: {} {}\n", call.name, brief(&call.name, &call.input)));
                        names.insert(call.id, call.name);
                    }
                }
                BlockKind::ToolResult => {
                    if let Some(v) = value_of(block) {
                        let id = v.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                        let name = names.get(id).map_or("tool", String::as_str);
                        let content = v.get("content").and_then(Value::as_str).unwrap_or("");
                        let first: String = content.lines().next().unwrap_or("").chars().take(200).collect();
                        out.push_str(&format!("Result of {name}: {first}\n"));
                    }
                }
                _ => {}
            }
        }
    }
    let count = out.chars().count();
    if count > cap {
        let skip = count - cap;
        let tail: String = out.chars().skip(skip).collect();
        format!("[earlier part omitted]\n{tail}")
    } else {
        out
    }
}

/// The instruction of the summarising call. Constant, like the system prompt.
pub const SUMMARY_PROMPT: &str = "You condense an earlier part of a conversation between an author and a writing assistant, so the conversation can continue with less text. Write a note of at most 1,200 words with these headings: Task, Decisions, Files touched, Pending proposals, Open questions. Keep file names, labels, citation keys and exact wording the author asked for. Do not add anything that was not said. No preamble.";

/// The view after stage 2: the session header and the summary as one user message, a short
/// acknowledgement (so roles alternate), then the recent turns untouched.
pub fn rebuild_after_summary(
    header: &str,
    summary: &str,
    recent: &[Message],
) -> Option<Vec<Message>> {
    let first = if header.is_empty() {
        format!("<summary of the earlier conversation>\n{summary}\n</summary>")
    } else {
        format!("{header}\n\n<summary of the earlier conversation>\n{summary}\n</summary>")
    };
    let mut out = Vec::with_capacity(recent.len() + 2);
    out.push(Message::user_text(&first).ok()?);
    out.push(
        Message::new(
            Role::Assistant,
            vec![ContentBlock::text("Understood. I have the summary.").ok()?],
        ),
    );
    out.extend_from_slice(recent);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str, input: Value) -> Message {
        Message::new(
            Role::Assistant,
            vec![ContentBlock::tool_use(id, name, &input).expect("block")],
        )
    }

    fn result(id: &str, text: &str) -> Message {
        Message::new(
            Role::User,
            vec![ContentBlock::tool_result(id, text, false).expect("block")],
        )
    }

    fn convo(reads: usize) -> Vec<Message> {
        let mut v = vec![Message::user_text("start").expect("msg")];
        for i in 0..reads {
            let id = format!("t{i}");
            v.push(call(&id, "read_file", json!({"path": format!("f{i}.typ"), "from_line": 1, "to_line": 400})));
            v.push(result(&id, &"line of text\n".repeat(200)));
        }
        v
    }

    #[test]
    fn thresholds_pick_the_stage() {
        assert_eq!(decide(10, 100, false), Compaction::None);
        assert_eq!(decide(60, 100, false), Compaction::ClearResults);
        assert_eq!(decide(60, 100, true), Compaction::None);
        assert_eq!(decide(85, 100, true), Compaction::Summarise);
        assert_eq!(decide(99, 100, false), Compaction::Summarise);
        assert_eq!(decide(5, 0, false), Compaction::None);
        assert_eq!(fill_percent(50, 200), 25);
    }

    #[test]
    fn stage_one_clears_old_results_but_keeps_the_last_four() {
        let messages = convo(7);
        let (view, cleared) = clear_old_results(&messages, KEEP_RESULTS);
        assert_eq!(cleared, 3);
        assert_eq!(view.len(), messages.len());
        let first = view[2].content[0].raw_json();
        assert!(first.contains("[read_file f0.typ 1-400: cleared]"), "{first}");
        let last = view[view.len() - 1].content[0].raw_json();
        assert!(last.contains("line of text"));
        // A second pass finds nothing more to clear.
        let (_, again) = clear_old_results(&view, KEEP_RESULTS);
        assert_eq!(again, 0);
    }

    #[test]
    fn edits_and_questions_are_never_cleared() {
        let mut messages = vec![Message::user_text("start").expect("msg")];
        messages.push(call("e1", "edit_text", json!({"path": "a.typ"})));
        messages.push(result("e1", &"Proposal p1 queued. ".repeat(30)));
        messages.extend(convo(6).into_iter().skip(1));
        let (view, _) = clear_old_results(&messages, KEEP_RESULTS);
        assert!(view[2].content[0].raw_json().contains("Proposal p1 queued"));
    }

    #[test]
    fn the_summary_split_keeps_the_last_two_author_turns() {
        let mut messages = convo(2);
        messages.push(Message::user_text("second").expect("msg"));
        messages.push(Message::new(Role::Assistant, vec![ContentBlock::text("ok").expect("b")]));
        messages.push(Message::user_text("third").expect("msg"));
        let (old, recent) = split_for_summary(&messages, KEEP_TURNS).expect("split");
        assert_eq!(old.len(), 5);
        assert_eq!(recent.len(), 3);
        assert!(is_author_turn(&recent[0]));
        let rebuilt = rebuild_after_summary("<project/>", "the note", recent).expect("view");
        assert_eq!(rebuilt.len(), 5);
        assert!(rebuilt[0].text().contains("the note"));
        assert!(rebuilt[0].text().starts_with("<project/>"));
        assert!(split_for_summary(&messages[..2], KEEP_TURNS).is_none());
    }

    #[test]
    fn the_summary_input_is_plain_text() {
        let text = render_for_summary(&convo(2), 100_000);
        assert!(text.contains("Author: start"));
        assert!(text.contains("Tool call: read_file f0.typ 1-400"));
        assert!(text.contains("Result of read_file: line of text"));
        let cut = render_for_summary(&convo(40), 500);
        assert!(cut.starts_with("[earlier part omitted]"));
    }
}
