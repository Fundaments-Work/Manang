//! The shared streaming driver: connect with retries, feed the SSE parser,
//! hand events to an adapter-specific [`EventParser`].
//!
//! The result is a lazily polled stream: nothing runs between polls, and
//! dropping it drops the response body, which closes the connection.

use crate::loopback::ensure_loopback;
use crate::retry::{RetryPolicy, parse_retry_after};
use crate::transport::{ByteStream, HttpRequest, HttpResponse, HttpTransport, TransportError};
use futures_util::StreamExt;
use manang_agent_core::{EventStream, ProviderError, ProviderEvent, SseEvent, SseParser};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How much of an error body we read (provider error bodies are small).
const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// An adapter's stream decoder.
pub(crate) trait EventParser: Send {
    fn set_request_id(&mut self, id: Option<String>);
    fn on_event(&mut self, event: &SseEvent) -> Vec<ProviderEvent>;
    /// The body ended. Emit whatever closes the response, if the provider's
    /// protocol allows ending without an explicit terminator.
    fn on_end(&mut self) -> Vec<ProviderEvent>;
}

/// Everything needed to perform one logical request.
pub(crate) struct Call {
    pub transport: Arc<dyn HttpTransport>,
    pub request: HttpRequest,
    pub retry: RetryPolicy,
    pub cancel: CancellationToken,
    pub offline: bool,
    /// Provider-specific request id header, besides `request-id`.
    pub request_id_headers: &'static [&'static str],
}

pub(crate) fn error_event(
    code: &str,
    message: impl Into<String>,
    retryable: bool,
    request_id: Option<String>,
) -> ProviderEvent {
    ProviderEvent::Error {
        error: ProviderError {
            code: code.to_owned(),
            message: message.into(),
            retryable,
            request_id,
        },
    }
}

/// A stream that yields one terminal error.
pub(crate) fn failed(code: &str, message: impl Into<String>) -> EventStream<'static> {
    let event = error_event(code, message, false, None);
    Box::pin(futures_util::stream::iter([event]))
}

fn request_id_of(response: &HttpResponse, extra: &[&str]) -> Option<String> {
    ["request-id"]
        .iter()
        .chain(extra.iter())
        .find_map(|name| response.header(name))
        .map(str::to_owned)
}

fn transport_error(error: &TransportError) -> ProviderEvent {
    let (code, retryable) = match error {
        TransportError::Cancelled => ("cancelled", false),
        TransportError::Blocked(_) => ("offline_blocked", false),
        TransportError::Config(_) => ("config", false),
        TransportError::Timeout => ("timeout", true),
        TransportError::Connect(_) => ("connect_failed", true),
        TransportError::Body(_) => ("stream_interrupted", true),
    };
    error_event(code, error.to_string(), retryable, None)
}

/// Reads at most [`ERROR_BODY_LIMIT`] bytes of an error body.
async fn read_error_body(mut body: ByteStream) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = body.next().await {
        out.extend_from_slice(&chunk);
        if out.len() >= ERROR_BODY_LIMIT {
            out.truncate(ERROR_BODY_LIMIT);
            break;
        }
    }
    out
}

/// Turns a non-2xx response into a provider error. Understands the shapes
/// `{"error":{"type","message"}}` (Anthropic) and `{"error":{"code","message"}}`
/// (OpenAI style).
fn http_error(status: u16, body: &[u8], request_id: Option<String>) -> ProviderEvent {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let error = parsed.as_ref().and_then(|v| v.get("error"));
    let text = |key: &str| {
        error
            .and_then(|e| e.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    // DeepSeek answers 402 with `type: "unknown_error"`; name the real cause.
    let canonical = match status {
        401 => Some("authentication_error"),
        402 => Some("insufficient_balance"),
        422 => Some("invalid_parameters"),
        _ => None,
    };
    let code = text("type")
        .filter(|t| t != "unknown_error")
        .or_else(|| canonical.map(str::to_owned))
        .or_else(|| text("code"))
        .unwrap_or_else(|| format!("http_{status}"));
    let provider_message = text("message").unwrap_or_else(|| format!("HTTP status {status}"));
    let message = match hint(status) {
        Some(hint) => format!("{provider_message} ({hint})"),
        None => provider_message,
    };
    let retryable = matches!(status, 429 | 500 | 502 | 503 | 504 | 529);
    error_event(&code, message, retryable, request_id)
}

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

enum Connected {
    Ok(HttpResponse),
    Failed(ProviderEvent),
}

async fn connect(call: &Call) -> Connected {
    if call.offline
        && let Err(reason) = ensure_loopback(&call.request.url).await
    {
        return Connected::Failed(error_event("offline_blocked", reason, false, None));
    }
    let mut retry = 0u32;
    loop {
        let response = match call
            .transport
            .post_stream(call.request.clone(), &call.cancel)
            .await
        {
            Ok(response) => response,
            Err(error) => return Connected::Failed(transport_error(&error)),
        };
        if (200..300).contains(&response.status) {
            return Connected::Ok(response);
        }
        let status = response.status;
        let request_id = request_id_of(&response, call.request_id_headers);
        let retry_after = parse_retry_after(response.header("retry-after"));
        let body = read_error_body(response.body).await;
        retry += 1;
        let wait = if retry < call.retry.max_attempts {
            call.retry.delay_for(status, retry_after, retry)
        } else {
            None
        };
        match wait {
            Some(wait) => {
                if !sleep_or_cancel(wait, &call.cancel).await {
                    return Connected::Failed(error_event(
                        "cancelled",
                        "request was cancelled",
                        false,
                        request_id,
                    ));
                }
            }
            None => return Connected::Failed(http_error(status, &body, request_id)),
        }
    }
}

/// Sleeps; returns false if cancelled first.
async fn sleep_or_cancel(wait: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        () = cancel.cancelled() => false,
        () = tokio::time::sleep(wait) => true,
    }
}

enum Phase {
    Connect(Box<Call>),
    Stream {
        body: ByteStream,
        sse: SseParser,
        cancel: CancellationToken,
    },
    Finished,
}

struct State<P> {
    phase: Phase,
    parser: P,
    queue: VecDeque<ProviderEvent>,
    terminal: bool,
}

impl<P: EventParser> State<P> {
    fn push(&mut self, events: Vec<ProviderEvent>) {
        for event in events {
            if self.terminal {
                // Nothing after a terminal event.
                break;
            }
            self.terminal = matches!(
                event,
                ProviderEvent::Done { .. } | ProviderEvent::Error { .. }
            );
            self.queue.push_back(event);
        }
    }

    fn finish_with(&mut self, event: ProviderEvent) {
        self.push(vec![event]);
        self.phase = Phase::Finished;
    }

    /// Advances until an event is available or the stream ends.
    async fn advance(&mut self) {
        let phase = std::mem::replace(&mut self.phase, Phase::Finished);
        match phase {
            Phase::Finished => {}
            Phase::Connect(call) => match connect(&call).await {
                Connected::Failed(event) => self.finish_with(event),
                Connected::Ok(response) => {
                    self.parser
                        .set_request_id(request_id_of(&response, call.request_id_headers));
                    self.phase = Phase::Stream {
                        body: response.body,
                        sse: SseParser::new(),
                        cancel: call.cancel.clone(),
                    };
                }
            },
            Phase::Stream {
                mut body,
                mut sse,
                cancel,
            } => {
                let next = tokio::select! {
                    () = cancel.cancelled() => {
                        self.finish_with(error_event("cancelled", "request was cancelled", false, None));
                        return;
                    }
                    next = body.next() => next,
                };
                match next {
                    Some(Ok(chunk)) => match sse.feed(&chunk) {
                        Ok(events) => {
                            for event in &events {
                                if self.terminal {
                                    break;
                                }
                                let out = self.parser.on_event(event);
                                self.push(out);
                            }
                            if !self.terminal {
                                self.phase = Phase::Stream { body, sse, cancel };
                            }
                        }
                        Err(error) => {
                            self.finish_with(error_event(
                                "bad_stream",
                                error.to_string(),
                                false,
                                None,
                            ));
                        }
                    },
                    Some(Err(error)) => self.finish_with(transport_error(&error)),
                    None => {
                        let out = self.parser.on_end();
                        self.push(out);
                        if !self.terminal {
                            self.queue.push_back(error_event(
                                "stream_truncated",
                                "the response ended before it was complete",
                                true,
                                None,
                            ));
                        }
                        self.phase = Phase::Finished;
                    }
                }
            }
        }
    }
}

/// Runs a call as a provider event stream.
pub(crate) fn run<P: EventParser + 'static>(call: Call, parser: P) -> EventStream<'static> {
    let state = State {
        phase: Phase::Connect(Box::new(call)),
        parser,
        queue: VecDeque::new(),
        terminal: false,
    };
    Box::pin(futures_util::stream::unfold(
        state,
        |mut state| async move {
            loop {
                if let Some(event) = state.queue.pop_front() {
                    return Some((event, state));
                }
                if matches!(state.phase, Phase::Finished) {
                    return None;
                }
                state.advance().await;
            }
        },
    ))
}
