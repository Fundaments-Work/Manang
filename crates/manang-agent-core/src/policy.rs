//! Policy layers (enterprise, user, project) merged tighten-only.
//!
//! A lower layer can never enable what a higher layer disabled. Layers are
//! passed highest priority first: `[enterprise, user, project]`.

use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One layer as written in a policy file. Absent keys leave the setting alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PolicyLayer {
    pub enabled: Option<bool>,
    pub allowed_providers: Option<Vec<String>>,
    /// Locks the provider base URL to exactly this value.
    pub base_url: Option<String>,
    pub require_audit_log: Option<bool>,
    pub allow_mcp_client: Option<bool>,
    pub max_context_tokens: Option<u64>,
}

/// The merged result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectivePolicy {
    pub enabled: bool,
    /// `None`: any provider is allowed.
    pub allowed_providers: Option<BTreeSet<String>>,
    /// `Some`: the base URL must equal this.
    pub locked_base_url: Option<String>,
    pub require_audit_log: bool,
    /// Whether policy permits the MCP client. The feature itself stays off
    /// until the user enables it in settings.
    pub allow_mcp_client: bool,
    pub max_context_tokens: Option<u64>,
}

impl Default for EffectivePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_providers: None,
            locked_base_url: None,
            require_audit_log: false,
            allow_mcp_client: true,
            max_context_tokens: None,
        }
    }
}

impl EffectivePolicy {
    /// Merges layers, highest priority first. Every rule only tightens:
    /// `enabled` and `allow_mcp_client` are ANDed, provider sets intersect,
    /// `require_audit_log` is ORed, the context limit takes the minimum, and
    /// the first base-URL lock wins.
    pub fn merge(layers: &[PolicyLayer]) -> Self {
        let mut out = Self::default();
        for layer in layers {
            if let Some(false) = layer.enabled {
                out.enabled = false;
            }
            if let Some(false) = layer.allow_mcp_client {
                out.allow_mcp_client = false;
            }
            if let Some(true) = layer.require_audit_log {
                out.require_audit_log = true;
            }
            if let Some(list) = &layer.allowed_providers {
                let set: BTreeSet<String> = list.iter().cloned().collect();
                out.allowed_providers = Some(match out.allowed_providers.take() {
                    Some(prev) => prev.intersection(&set).cloned().collect(),
                    None => set,
                });
            }
            if out.locked_base_url.is_none() {
                out.locked_base_url.clone_from(&layer.base_url);
            }
            if let Some(max) = layer.max_context_tokens {
                out.max_context_tokens = Some(out.max_context_tokens.map_or(max, |m| m.min(max)));
            }
        }
        out
    }

    /// Whether `self` permits nothing that `other` forbids.
    pub fn is_at_most_as_permissive_as(&self, other: &Self) -> bool {
        let providers_ok = match (&self.allowed_providers, &other.allowed_providers) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => a.is_subset(b),
        };
        let url_ok = match (&self.locked_base_url, &other.locked_base_url) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => a == b,
        };
        let ctx_ok = match (self.max_context_tokens, other.max_context_tokens) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => a <= b,
        };
        (!self.enabled || other.enabled)
            && (!self.allow_mcp_client || other.allow_mcp_client)
            && (self.require_audit_log || !other.require_audit_log)
            && providers_ok
            && url_ok
            && ctx_ok
    }

    /// Fails unless the whole agent is enabled.
    pub fn check_enabled(&self) -> Result<(), CoreError> {
        if self.enabled {
            Ok(())
        } else {
            Err(CoreError::Policy(
                "the AI agent is disabled by policy".into(),
            ))
        }
    }

    pub fn check_provider(&self, provider: &str) -> Result<(), CoreError> {
        match &self.allowed_providers {
            Some(set) if !set.contains(provider) => Err(CoreError::Policy(format!(
                "provider `{provider}` is not allowed by policy"
            ))),
            _ => Ok(()),
        }
    }

    pub fn check_base_url(&self, url: &str) -> Result<(), CoreError> {
        match &self.locked_base_url {
            Some(locked) if locked != url => Err(CoreError::Policy(
                "the provider base URL is locked by policy".into(),
            )),
            _ => Ok(()),
        }
    }

    pub fn check_context_tokens(&self, tokens: u64) -> Result<(), CoreError> {
        match self.max_context_tokens {
            Some(max) if tokens > max => Err(CoreError::Policy(format!(
                "context of {tokens} tokens exceeds the policy limit of {max}"
            ))),
            _ => Ok(()),
        }
    }
}
