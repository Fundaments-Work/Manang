//! Lossless content blocks and messages.
//!
//! Providers return blocks we must replay *byte-exact* (thinking blocks carry
//! signatures that are validated on the next turn). A [`ContentBlock`] is
//! therefore the raw JSON text plus a classified kind; nothing is re-encoded.

use crate::error::CoreError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use std::borrow::Cow;

/// Who sent a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// What a block is, from its `type` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Text,
    ToolUse,
    ToolResult,
    Thinking,
    RedactedThinking,
    /// Anything else (images, documents, server tool blocks...). Kept verbatim.
    Other,
}

/// One provider content block, kept as the exact JSON text received.
#[derive(Debug, Clone)]
pub struct ContentBlock {
    kind: BlockKind,
    raw: Box<RawValue>,
}

/// A decoded `tool_use` block.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseView {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Deserialize)]
struct TypePeek<'a> {
    #[serde(borrow, rename = "type")]
    ty: Cow<'a, str>,
}

impl ContentBlock {
    /// Wraps raw JSON text without altering a byte.
    pub fn from_raw(raw: Box<RawValue>) -> Result<Self, CoreError> {
        let peek: TypePeek<'_> = serde_json::from_str(raw.get())
            .map_err(|e| CoreError::InvalidBlock(format!("no string `type`: {e}")))?;
        let kind = match peek.ty.as_ref() {
            "text" => BlockKind::Text,
            "tool_use" => BlockKind::ToolUse,
            "tool_result" => BlockKind::ToolResult,
            "thinking" => BlockKind::Thinking,
            "redacted_thinking" => BlockKind::RedactedThinking,
            _ => BlockKind::Other,
        };
        Ok(Self { kind, raw })
    }

    /// Parses JSON text into a block, keeping the text verbatim.
    pub fn from_json_str(text: &str) -> Result<Self, CoreError> {
        Self::from_raw(RawValue::from_string(text.to_owned())?)
    }

    /// Builds a block from a value (a fresh encoding; use for blocks we author).
    pub fn from_value(value: &serde_json::Value) -> Result<Self, CoreError> {
        Self::from_raw(serde_json::value::to_raw_value(value)?)
    }

    /// A text block.
    pub fn text(text: &str) -> Result<Self, CoreError> {
        Self::from_value(&serde_json::json!({"type": "text", "text": text}))
    }

    /// A `tool_use` block.
    pub fn tool_use(id: &str, name: &str, input: &serde_json::Value) -> Result<Self, CoreError> {
        Self::from_value(
            &serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": input}),
        )
    }

    /// A `tool_result` block carrying text.
    pub fn tool_result(
        tool_use_id: &str,
        content: &str,
        is_error: bool,
    ) -> Result<Self, CoreError> {
        Self::from_value(&serde_json::json!({
            "type": "tool_result", "tool_use_id": tool_use_id,
            "content": content, "is_error": is_error
        }))
    }

    pub fn kind(&self) -> BlockKind {
        self.kind
    }

    /// The exact JSON text.
    pub fn raw_json(&self) -> &str {
        self.raw.get()
    }

    /// The text of a `text` block.
    pub fn as_text(&self) -> Option<String> {
        #[derive(Deserialize)]
        struct T {
            text: String,
        }
        if self.kind != BlockKind::Text {
            return None;
        }
        serde_json::from_str::<T>(self.raw.get())
            .ok()
            .map(|t| t.text)
    }

    /// The decoded form of a `tool_use` block.
    pub fn as_tool_use(&self) -> Option<ToolUseView> {
        #[derive(Deserialize)]
        struct T {
            id: String,
            name: String,
            #[serde(default)]
            input: serde_json::Value,
        }
        if self.kind != BlockKind::ToolUse {
            return None;
        }
        serde_json::from_str::<T>(self.raw.get())
            .ok()
            .map(|t| ToolUseView {
                id: t.id,
                name: t.name,
                input: t.input,
            })
    }
}

impl PartialEq for ContentBlock {
    fn eq(&self, other: &Self) -> bool {
        self.raw.get() == other.raw.get()
    }
}
impl Eq for ContentBlock {}

impl Serialize for ContentBlock {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.raw.serialize(s)
    }
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(d)?;
        Self::from_raw(raw).map_err(serde::de::Error::custom)
    }
}

/// One conversation turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self { role, content }
    }

    /// A user message with a single text block.
    pub fn user_text(text: &str) -> Result<Self, CoreError> {
        Ok(Self::new(Role::User, vec![ContentBlock::text(text)?]))
    }

    /// Concatenated text of all text blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(ContentBlock::as_text)
            .collect()
    }
}
