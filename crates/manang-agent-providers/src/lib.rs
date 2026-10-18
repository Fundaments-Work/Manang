//! Manang AI agent providers.
//!
//! Two in-house adapters (Anthropic Messages, OpenAI-compatible Chat
//! Completions) over our own [`HttpTransport`] and SSE parser. The crate is
//! the only place in the agent stack that uses Tokio, and only for network
//! I/O. rustls with ring only: no OpenSSL, no native-tls, no C library
//! beyond what the app already links.
//!
//! Dropping the stream returned by [`manang_agent_core::Provider::stream`]
//! closes the connection; a [`tokio_util::sync::CancellationToken`] passed
//! with `with_cancel` ends it with a terminal `cancelled` event.
//!
//! # Trying a provider
//!
//! ```text
//! cargo run -p manang-agent-providers --example chat -- --list
//! DEEPSEEK_API_KEY=... cargo run -p manang-agent-providers --example chat -- --preset deepseek "hello"
//! ```
//!
//! # Live tests (ignored by default; need a key and `MANANG_EVAL_LIVE=1`)
//!
//! ```text
//! DEEPSEEK_API_KEY=... MANANG_EVAL_LIVE=1 cargo test -p manang-agent-providers --locked -- --ignored deepseek --nocapture
//! ```

pub mod anthropic;
mod driver;
pub mod key;
pub mod loopback;
pub mod openai;
pub mod retry;
pub mod transport;

pub use anthropic::{AnthropicConfig, AnthropicProvider};
pub use key::ApiKey;
pub use openai::{OpenAiCompatConfig, OpenAiCompatProvider};
pub use retry::RetryPolicy;
pub use transport::{
    ByteStream, HttpRequest, HttpResponse, HttpTransport, ReqwestTransport, TransportConfig,
    TransportError,
};
