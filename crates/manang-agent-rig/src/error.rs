//! Rig's `ProviderError` to our `ProviderError` (code, message, retryable, request id).
//!
//! The codes match the in-house adapters: `authentication_error` (401), `insufficient_balance`
//! (402), `invalid_parameters` (422), the provider's own `error.type` when the body has one,
//! `http_<status>` otherwise, plus `offline_blocked`, `connect_failed`, `stream_interrupted`,
//! `stream_truncated` and `invalid_request`. `retryable` follows our retry rules (429, 500,
//! 502, 503, 504, 529), not Rig's (which also retries 408 and 425).

use crate::http::Blocked;
use manang_agent_core::ProviderError as OurError;
use rig_core::ProviderError as RigError;
use rig_core::http_client::Error as HttpError;
use serde_json::Value;

/// What the user can do about a status. Appended to the provider's message.
fn hint(status: u16) -> Option<&'static str> {
    match status {
        401 => Some("authentication failed: check the API key"),
        402 => Some("insufficient balance: add funds to the provider account"),
        403 => Some("this key is not allowed to use that resource"),
        422 => Some("the provider rejected a request parameter"),
        429 => Some("rate limited: slow down or retry later"),
        500 | 503 | 529 => Some("the provider is failing or overloaded: retry shortly"),
        _ => None,
    }
}

fn error_retryable(kind: &str) -> bool {
    matches!(kind, "overloaded_error" | "api_error" | "timeout_error")
}

fn from_body(status: Option<u16>, body: &str, request_id: Option<String>) -> OurError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let error = parsed.as_ref().and_then(|v| v.get("error"));
    let text = |key: &str| {
        error
            .and_then(|e| e.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let canonical = match status {
        Some(401) => Some("authentication_error"),
        Some(402) => Some("insufficient_balance"),
        Some(422) => Some("invalid_parameters"),
        _ => None,
    };
    let code = text("type")
        .filter(|t| t != "unknown_error")
        .or_else(|| canonical.map(str::to_owned))
        .or_else(|| text("code"))
        .unwrap_or_else(|| match status {
            Some(s) => format!("http_{s}"),
            None => "stream_error".to_owned(),
        });
    let provider_message = text("message").unwrap_or_else(|| match status {
        Some(s) => format!("HTTP status {s}"),
        None => "the provider reported an error".to_owned(),
    });
    let message = match status.and_then(hint) {
        Some(hint) => format!("{provider_message} ({hint})"),
        None => provider_message,
    };
    let retryable = match status {
        Some(s) => matches!(s, 429 | 500 | 502 | 503 | 504 | 529),
        None => error_retryable(&code),
    };
    let request_id = parsed
        .as_ref()
        .and_then(|v| v.get("request_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or(request_id);
    OurError {
        code,
        message,
        retryable,
        request_id,
    }
}

fn simple(code: &str, message: impl Into<String>, retryable: bool) -> OurError {
    OurError {
        code: code.to_owned(),
        message: message.into(),
        retryable,
        request_id: None,
    }
}

/// Maps Rig's error. `streaming` is true once the response began (a transport failure then is
/// a broken stream, not a failed connect).
pub(crate) fn map_error(error: &RigError, streaming: bool) -> OurError {
    let request_id = error.provider_request_id().map(str::to_owned);
    match error {
        RigError::ProviderResponse(response) | RigError::InvalidAuthentication(response) => {
            let status = response.status.map(|s| s.as_u16());
            from_body(status, &response.body, request_id)
        }
        RigError::Http(http) => match http.as_ref() {
            HttpError::InvalidStatusCodeWithDetails { status, body, .. } => {
                from_body(Some(status.as_u16()), body, request_id)
            }
            HttpError::Instance(inner) => {
                if let Some(blocked) = inner.downcast_ref::<Blocked>() {
                    simple("offline_blocked", blocked.0.clone(), false)
                } else if streaming {
                    simple(
                        "stream_interrupted",
                        "the connection failed mid-response",
                        true,
                    )
                } else {
                    simple("connect_failed", "could not connect to the provider", true)
                }
            }
            HttpError::StreamEnded => simple(
                "stream_interrupted",
                "the connection ended mid-response",
                true,
            ),
            other => simple("http_error", other.to_string(), false),
        },
        RigError::Truncated => simple(
            "stream_truncated",
            "the response ended before it was complete",
            true,
        ),
        RigError::Request(_) | RigError::UnsupportedOption(_) | RigError::Url(_) => {
            simple("invalid_request", error.to_string(), false)
        }
        RigError::Json(_) | RigError::Response(_) | RigError::Provider(_) => {
            simple("bad_stream", error.to_string(), false)
        }
        _ => simple("provider_error", error.to_string(), error.is_retryable()),
    }
}
