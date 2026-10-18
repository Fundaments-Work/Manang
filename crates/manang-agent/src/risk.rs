//! Risk classes and approval rules, enforced by the host. The model never chooses them, and a
//! tool without a class is denied.

use manang_agent_core::RiskClass;
use std::collections::{BTreeMap, BTreeSet};

/// What the host knows about one tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolInfo {
    pub risk: RiskClass,
    /// The tool changes data when it runs (v1 figure tools edit the open figure in place). It is
    /// treated like a network or exec tool: the user approves it.
    pub direct_write: bool,
    /// What the tool returns comes from outside the project (a web registry, a fetched page).
    pub untrusted_output: bool,
    /// The host a network tool contacts, shown in the approval row and part of its scope.
    pub host: Option<&'static str>,
}

impl ToolInfo {
    pub const fn read() -> Self {
        Self {
            risk: RiskClass::Read,
            direct_write: false,
            untrusted_output: false,
            host: None,
        }
    }

    pub const fn propose() -> Self {
        Self {
            risk: RiskClass::Propose,
            direct_write: false,
            untrusted_output: false,
            host: None,
        }
    }

    pub const fn direct_write() -> Self {
        Self {
            risk: RiskClass::Propose,
            direct_write: true,
            untrusted_output: false,
            host: None,
        }
    }

    pub const fn network() -> Self {
        Self {
            risk: RiskClass::Network,
            direct_write: false,
            untrusted_output: true,
            host: None,
        }
    }

    pub const fn interaction() -> Self {
        Self {
            risk: RiskClass::Interaction,
            direct_write: false,
            untrusted_output: false,
            host: None,
        }
    }

    /// A network tool that contacts one known host.
    pub const fn network_to(host: &'static str) -> Self {
        Self {
            risk: RiskClass::Network,
            direct_write: false,
            untrusted_output: true,
            host: Some(host),
        }
    }

    pub const fn exec() -> Self {
        Self {
            risk: RiskClass::Exec,
            direct_write: false,
            untrusted_output: false,
            host: None,
        }
    }
}

/// Tool name to class.
#[derive(Debug, Clone)]
pub struct RiskTable {
    map: BTreeMap<String, ToolInfo>,
}

impl Default for RiskTable {
    fn default() -> Self {
        let mut map = BTreeMap::new();
        for name in [
            "list_files",
            "read_file",
            "search",
            "outline",
            "diagnostics",
            "bib_search",
            "figure_describe",
            "figure_to_mermaid",
        ] {
            map.insert(name.to_owned(), ToolInfo::read());
        }
        map.insert("edit_text".to_owned(), ToolInfo::propose());
        map.insert("ask_user".to_owned(), ToolInfo::interaction());
        for name in [
            "figure_add_shapes",
            "figure_connect",
            "figure_update",
            "figure_remove",
            "figure_layout",
            "figure_from_mermaid",
            "figure_style",
        ] {
            map.insert(name.to_owned(), ToolInfo::direct_write());
        }
        map.insert("figure_export".to_owned(), ToolInfo::exec());
        // Registered by the citations plugin; it asks Crossref (and OpenAlex or arXiv).
        map.insert(
            "verify_citation".to_owned(),
            ToolInfo::network_to("api.crossref.org"),
        );
        Self { map }
    }
}

impl RiskTable {
    /// Adds or replaces a class (first-party plugins and tests).
    pub fn with(mut self, name: &str, info: ToolInfo) -> Self {
        self.map.insert(name.to_owned(), info);
        self
    }

    pub fn get(&self, name: &str) -> Option<&ToolInfo> {
        self.map.get(name)
    }
}

/// The user's answer to an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// This call only.
    Allow,
    /// This call and the rest of the session (except after untrusted content, see [`gate`]).
    AllowSession,
    Deny,
}

impl Decision {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "allow" => Some(Self::Allow),
            "allow_session" => Some(Self::AllowSession),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// What the orchestrator does before running a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Auto,
    /// Ask the user. `scope` is remembered on `allow_session`.
    Ask {
        scope: String,
    },
    Deny(String),
}

/// Decides the gate for a call.
///
/// * `read`, `interaction` and `propose` run on their own.
/// * Tools that change data, `network` and `exec` ask once per session.
/// * A `network` call after the session has seen untrusted external content asks every time:
///   no session both reads untrusted content and reaches the network without an approval.
pub fn gate(info: Option<&ToolInfo>, remembered: &BTreeSet<String>, tainted: bool) -> Gate {
    let Some(info) = info else {
        return Gate::Deny("this tool has no risk class, so it is not allowed".to_owned());
    };
    let (scope, always) = match info.risk {
        RiskClass::Read | RiskClass::Interaction => return Gate::Auto,
        RiskClass::Propose if !info.direct_write => return Gate::Auto,
        RiskClass::Propose => ("write".to_owned(), false),
        RiskClass::Network => (
            info.host
                .map_or_else(|| "network".to_owned(), |h| format!("network:{h}")),
            tainted,
        ),
        RiskClass::Exec => ("exec".to_owned(), false),
    };
    if !always && remembered.contains(&scope) {
        Gate::Auto
    } else {
        Gate::Ask { scope }
    }
}
