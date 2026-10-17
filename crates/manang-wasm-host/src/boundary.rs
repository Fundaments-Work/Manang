//! Conversion of payloads across the host/guest boundary, with size caps.
//!
//! Everything a guest returns is untrusted. This module is the only place
//! guest data becomes a kernel [`Payload`], so it is also the fuzz target for
//! the boundary (`fuzz/fuzz_targets/payload_boundary.rs`).

use manang_kernel::{Payload, PluginError};

use crate::limits::{Limits, MAX_MESSAGE_BYTES};

/// A payload as it exists on the guest side of the boundary, before the host
/// has validated it. Mirrors the WIT `payload` variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawPayload {
    Empty,
    Json(String),
    Binary(Vec<u8>),
}

/// Lower a kernel payload for a guest, refusing anything over
/// `limits.max_payload_bytes`.
pub fn encode_input(payload: &Payload, limits: &Limits) -> Result<RawPayload, PluginError> {
    let too_big = |len: usize| {
        PluginError::Failed(format!(
            "input of {len} bytes exceeds the {} byte payload limit",
            limits.max_payload_bytes
        ))
    };
    match payload {
        Payload::Empty => Ok(RawPayload::Empty),
        Payload::Binary(bytes) => {
            if bytes.len() > limits.max_payload_bytes {
                return Err(too_big(bytes.len()));
            }
            Ok(RawPayload::Binary(bytes.to_vec()))
        }
        Payload::Json(value) => {
            let text = value.to_string();
            if text.len() > limits.max_payload_bytes {
                return Err(too_big(text.len()));
            }
            Ok(RawPayload::Json(text))
        }
    }
}

/// Validate and lift a guest's result. Oversized output and malformed JSON
/// (including nesting beyond `serde_json`'s recursion limit) are errors, never
/// panics.
pub fn decode_output(raw: RawPayload, limits: &Limits) -> Result<Payload, PluginError> {
    let too_big = |len: usize| {
        PluginError::Failed(format!(
            "plugin returned {len} bytes, over the {} byte output limit",
            limits.max_output_bytes
        ))
    };
    match raw {
        RawPayload::Empty => Ok(Payload::Empty),
        RawPayload::Binary(bytes) => {
            if bytes.len() > limits.max_output_bytes {
                return Err(too_big(bytes.len()));
            }
            Ok(Payload::Binary(bytes.into()))
        }
        RawPayload::Json(text) => {
            if text.len() > limits.max_output_bytes {
                return Err(too_big(text.len()));
            }
            serde_json::from_str(&text)
                .map(Payload::Json)
                .map_err(|e| PluginError::Failed(format!("plugin returned invalid JSON: {e}")))
        }
    }
}

/// Cut `message` to the error-text cap on a character boundary.
pub(crate) fn truncate_message(mut message: String) -> String {
    if message.len() > MAX_MESSAGE_BYTES {
        let mut end = MAX_MESSAGE_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str("... (truncated)");
    }
    message
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    fn small() -> Limits {
        Limits {
            max_payload_bytes: 16,
            max_output_bytes: 16,
            ..Limits::default()
        }
    }

    #[test]
    fn input_over_the_cap_is_refused_before_copying() {
        let big = Payload::Binary(Arc::from(vec![0u8; 17]));
        assert!(encode_input(&big, &small()).is_err());
        let ok = Payload::Binary(Arc::from(vec![0u8; 16]));
        assert!(encode_input(&ok, &small()).is_ok());
        let big_json = Payload::Json(json!("x".repeat(32)));
        assert!(encode_input(&big_json, &small()).is_err());
    }

    #[test]
    fn output_over_the_cap_or_malformed_is_refused() {
        assert!(decode_output(RawPayload::Binary(vec![0; 17]), &small()).is_err());
        assert!(decode_output(RawPayload::Json("x".repeat(17)), &small()).is_err());
        assert!(decode_output(RawPayload::Json("{not json".into()), &small()).is_err());
        assert_eq!(
            decode_output(RawPayload::Json("[1]".into()), &small()).expect("ok"),
            Payload::Json(json!([1]))
        );
    }

    #[test]
    fn deeply_nested_json_is_an_error_not_a_stack_overflow() {
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        let limits = Limits::default();
        assert!(decode_output(RawPayload::Json(deep), &limits).is_err());
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let s = "é".repeat(MAX_MESSAGE_BYTES);
        let out = truncate_message(s);
        assert!(out.ends_with("(truncated)"));
        assert!(out.len() <= MAX_MESSAGE_BYTES + 20);
    }
}
