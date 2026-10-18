//! Property tests for context compaction: whatever the conversation looks like, clearing old tool
//! results keeps its structure (every call still has its result, the author's words are untouched)
//! and the summary split never loses or reorders a message.

use manang_agent_core::compact::{
    KEEP_RESULTS, KEEP_TURNS, clear_old_results, is_author_turn, rebuild_after_summary,
    split_for_summary,
};
use manang_agent_core::{BlockKind, ContentBlock, Message, Role};
use proptest::prelude::*;
use serde_json::json;

/// A conversation: author turns, each followed by some rounds of (assistant calls, user results).
fn conversation(turns: Vec<Vec<usize>>) -> Vec<Message> {
    let mut out = Vec::new();
    let mut id = 0;
    for (t, rounds) in turns.into_iter().enumerate() {
        out.push(Message::user_text(&format!("author message {t}")).expect("message"));
        for calls in rounds {
            let mut uses = Vec::new();
            let mut results = Vec::new();
            for _ in 0..calls.max(1) {
                id += 1;
                let name = if id % 5 == 0 {
                    "edit_text"
                } else {
                    "read_file"
                };
                uses.push(
                    ContentBlock::tool_use(
                        &format!("c{id}"),
                        name,
                        &json!({"path": format!("f{id}.typ")}),
                    )
                    .expect("block"),
                );
                results.push(
                    ContentBlock::tool_result(&format!("c{id}"), &"text ".repeat(100), false)
                        .expect("block"),
                );
            }
            out.push(Message::new(Role::Assistant, uses));
            out.push(Message::new(Role::User, results));
        }
        out.push(Message::new(
            Role::Assistant,
            vec![ContentBlock::text(&format!("answer {t}")).expect("block")],
        ));
    }
    out
}

fn ids(messages: &[Message], kind: BlockKind) -> Vec<String> {
    messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|b| b.kind() == kind)
        .filter_map(|b| {
            let v: serde_json::Value = serde_json::from_str(b.raw_json()).ok()?;
            v.get("id")
                .or_else(|| v.get("tool_use_id"))
                .and_then(|x| x.as_str())
                .map(str::to_owned)
        })
        .collect()
}

proptest! {
    #[test]
    fn clearing_keeps_the_structure(
        turns in proptest::collection::vec(proptest::collection::vec(1usize..4, 0..6), 1..6),
        keep in 0usize..8,
    ) {
        let messages = conversation(turns);
        let (view, cleared) = clear_old_results(&messages, keep);
        prop_assert_eq!(view.len(), messages.len());
        prop_assert_eq!(ids(&view, BlockKind::ToolUse), ids(&messages, BlockKind::ToolUse));
        prop_assert_eq!(ids(&view, BlockKind::ToolResult), ids(&messages, BlockKind::ToolResult));
        // The author's words and the assistant's text never change.
        for (a, b) in messages.iter().zip(&view) {
            if is_author_turn(a) || a.role == Role::Assistant {
                prop_assert_eq!(a, b);
            }
        }
        // Clearing is idempotent: a second pass has nothing left to do.
        let (again, more) = clear_old_results(&view, keep);
        prop_assert_eq!(more, 0);
        prop_assert_eq!(again, view);
        prop_assert!(cleared <= messages.len());
    }

    #[test]
    fn the_summary_split_loses_nothing(
        turns in proptest::collection::vec(proptest::collection::vec(1usize..3, 0..4), 1..7),
    ) {
        let messages = conversation(turns);
        match split_for_summary(&messages, KEEP_TURNS) {
            None => prop_assert!(messages.iter().filter(|m| is_author_turn(m)).count() <= KEEP_TURNS),
            Some((old, recent)) => {
                prop_assert_eq!(old.len() + recent.len(), messages.len());
                prop_assert!(is_author_turn(&recent[0]));
                prop_assert_eq!(recent.iter().filter(|m| is_author_turn(m)).count(), KEEP_TURNS);
                let rebuilt = rebuild_after_summary("<project/>", "note", recent).expect("view");
                prop_assert_eq!(rebuilt.len(), recent.len() + 2);
                prop_assert_eq!(&rebuilt[2..], recent);
            }
        }
    }
}

#[test]
fn the_default_keeps_four_results() {
    assert_eq!(KEEP_RESULTS, 4);
}
