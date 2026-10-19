//! The seam for an organisation's plugin policy (allowlist, blocklist,
//! "signed only"). Nothing enforces anything yet: the default gate allows
//! everything. The manager asks the gate before installing and before
//! enabling, so a later enterprise layer plugs in without schema changes.

use thiserror::Error;

/// What the manager is about to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    Install,
    Enable,
    /// Reading the plugin registry (Browse, update checks). The request's
    /// `name` is `registry`.
    Browse,
}

/// What the gate may judge a plugin on.
#[derive(Debug, Clone, Copy)]
pub struct PolicyRequest<'a> {
    pub action: PolicyAction,
    pub name: &'a str,
    pub version: &'a str,
    /// The publisher the plugin declares, unverified.
    pub publisher: Option<&'a str>,
    /// Whether a valid `plugin.sig` vouches for the files. Always `false`
    /// until signature verification lands.
    pub signed: bool,
}

/// A veto, with the reason shown to the user.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{reason}")]
pub struct PolicyDenied {
    pub reason: String,
}

/// Decides whether a plugin may be installed or enabled.
pub trait PolicyGate: Send + Sync {
    fn check(&self, request: &PolicyRequest<'_>) -> Result<(), PolicyDenied>;
}

/// Allows everything. The default.
#[derive(Debug, Clone, Copy, Default)]
pub struct Permissive;

impl PolicyGate for Permissive {
    fn check(&self, _request: &PolicyRequest<'_>) -> Result<(), PolicyDenied> {
        Ok(())
    }
}
