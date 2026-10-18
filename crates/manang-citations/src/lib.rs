pub mod error;
pub mod extract;
pub mod format;
pub mod http;
pub mod index;
pub mod models;
pub mod parser;
pub mod plugin;
pub mod verify;

pub use error::CitationsError;
pub use extract::extract_keys;
pub use format::{format_apa, format_entry, format_numeric, is_supported_style};
pub use http::{BlockingHttpClient, HttpClient, HttpResponse, MockHttpClient};
pub use index::CitationIndex;
pub use models::*;
pub use parser::{parse_bibtex, parse_csl_json, parse_file, parse_source};
pub use plugin::{
    CitationsPlugin, DEFAULT_STYLE, manifest, plugin, plugin_with_client, plugin_with_store,
};
pub use verify::verify_entry;
