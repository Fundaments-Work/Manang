//! A deterministic provider driven by recorded cassettes.

use crate::error::CoreError;
use crate::event::{ProviderError, ProviderEvent};
use crate::provider::{EventStream, Provider};
use crate::request::ChatRequest;
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

/// A recording: one list of events per model turn, replayed in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cassette {
    pub name: String,
    pub turns: Vec<Vec<ProviderEvent>>,
}

impl Cassette {
    /// Parses a cassette from its JSON file contents.
    pub fn from_json(text: &str) -> Result<Self, CoreError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Serialises the cassette as pretty JSON.
    pub fn to_json(&self) -> Result<String, CoreError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[derive(Debug, Default)]
struct State {
    next: usize,
    requests: Vec<ChatRequest>,
}

/// Replays a [`Cassette`]: each `stream` call plays the next turn. Running
/// past the recording yields a terminal `cassette_exhausted` error.
#[derive(Debug)]
pub struct MockProvider {
    cassette: Cassette,
    state: Mutex<State>,
}

impl MockProvider {
    pub fn new(cassette: Cassette) -> Self {
        Self {
            cassette,
            state: Mutex::new(State::default()),
        }
    }

    /// The requests received so far, in order (for assertions).
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.state
            .lock()
            .map(|s| s.requests.clone())
            .unwrap_or_default()
    }

    fn fail(code: &str, message: &str) -> VecDeque<ProviderEvent> {
        VecDeque::from([ProviderEvent::Error {
            error: ProviderError {
                code: code.to_owned(),
                message: message.to_owned(),
                retryable: false,
                request_id: None,
            },
        }])
    }
}

/// A finished list of events presented as a stream.
struct Replay(VecDeque<ProviderEvent>);

impl Stream for Replay {
    type Item = ProviderEvent;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<ProviderEvent>> {
        Poll::Ready(self.0.pop_front())
    }
}

impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn stream<'a>(&'a self, request: &'a ChatRequest) -> EventStream<'a> {
        let events = match self.state.lock() {
            Ok(mut state) => {
                state.requests.push(request.clone());
                let turn = state.next;
                state.next += 1;
                match self.cassette.turns.get(turn) {
                    Some(events) => events.iter().cloned().collect(),
                    None => Self::fail(
                        "cassette_exhausted",
                        &format!("cassette `{}` has no turn {turn}", self.cassette.name),
                    ),
                }
            }
            Err(_) => Self::fail("mock_poisoned", "mock provider state is poisoned"),
        };
        Box::pin(Replay(events))
    }
}
