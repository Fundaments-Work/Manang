//! Manang's first-party project plugin.
//!
//! Owns everything about a project directory: the `manang.toml` config, the
//! file tree, confined file operations, recent projects, change watching and
//! git status. See [`plugin`] for the command surface.

mod config;
mod error;
mod paths;

pub use config::{
    Bibliography, CONFIG_FILE, Engine, ProjectConfig, ProjectSection, TemplateBinding,
};
pub use error::{ConfigError, Issue, ProjectError};
pub use paths::{MAX_PATH_BYTES, ProjectRoot, lexical_relative, to_slash};
