//! The provider interface, free of any async runtime.

use crate::event::ProviderEvent;
use crate::request::ChatRequest;
use futures_core::Stream;
use std::pin::Pin;

/// A boxed stream of provider events. Dropping it cancels the request.
pub type EventStream<'a> = Pin<Box<dyn Stream<Item = ProviderEvent> + Send + 'a>>;

/// A model provider. Failures are reported as a terminal
/// [`ProviderEvent::Error`], never by panicking or by ending silently.
pub trait Provider: Send + Sync {
    /// Stable id (`anthropic`, `openai_compat`, `mock`).
    fn id(&self) -> &str;

    /// Starts one streamed response. The request is not retained.
    fn stream<'a>(&'a self, request: &'a ChatRequest) -> EventStream<'a>;
}
