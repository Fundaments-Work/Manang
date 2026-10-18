//! Manang's first-party project plugin.
//!
//! Owns everything about a project directory: the `manang.toml` config, the
//! file tree, confined file operations, recent projects, change watching and
//! git status. See [`plugin`] for the command surface.

mod config;
mod error;
pub mod export;
pub mod fsops;
pub mod git;
mod paths;
mod plugin;
pub mod recent;
pub mod registry;
pub mod scaffold;
pub mod template;
pub mod tree;
pub mod watch;

pub use config::{
    Bibliography, CONFIG_FILE, Engine, ProjectConfig, ProjectSection, TemplateBinding,
};
pub use error::{ConfigError, Issue, ProjectError};
pub use paths::{MAX_PATH_BYTES, ProjectRoot, lexical_relative, to_slash};
pub use plugin::{ProjectPlugin, manifest, plugin};
pub use template::{TemplateManifest, list_templates};
