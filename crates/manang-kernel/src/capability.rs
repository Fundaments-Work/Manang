//! Capability model: what a plugin may touch, declared up front.

use std::collections::HashSet;
use std::fmt;

use crate::error::CapabilityError;
use crate::manifest::Permissions;

/// Direction of access. `Write` implies `Read` on the same scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    Read,
    Write,
}

/// One concrete permission.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    /// A directory scope relative to the project, e.g. `templates`.
    Filesystem { access: Access, scope: String },
    /// An exact host name. Wildcards are deliberately unsupported.
    Network { host: String },
    /// A document buffer target, e.g. `active`.
    Buffer { access: Access, target: String },
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let access = |a: &Access| match a {
            Access::Read => "read",
            Access::Write => "write",
        };
        match self {
            Self::Filesystem { access: a, scope } => write!(f, "filesystem {}:{scope}", access(a)),
            Self::Network { host } => write!(f, "network {host}"),
            Self::Buffer { access: a, target } => write!(f, "buffer {}:{target}", access(a)),
        }
    }
}

fn parse_access_scope(entry: &str, path_like: bool) -> Result<(Access, String), CapabilityError> {
    let (access, scope) = entry
        .split_once(':')
        .ok_or_else(|| CapabilityError::Malformed(entry.to_owned()))?;
    let access = match access {
        "read" => Access::Read,
        "write" => Access::Write,
        other => {
            return Err(CapabilityError::UnknownAccess(
                entry.to_owned(),
                other.to_owned(),
            ));
        }
    };
    let unsafe_path = path_like
        && (scope.starts_with('/')
            || scope.starts_with('\\')
            || scope.split(['/', '\\']).any(|s| s == ".."));
    if scope.is_empty() || unsafe_path {
        return Err(CapabilityError::UnsafeScope(entry.to_owned()));
    }
    Ok((access, scope.to_owned()))
}

impl Capability {
    pub fn parse_filesystem(entry: &str) -> Result<Self, CapabilityError> {
        let (access, scope) = parse_access_scope(entry, true)?;
        Ok(Self::Filesystem { access, scope })
    }

    pub fn parse_buffer(entry: &str) -> Result<Self, CapabilityError> {
        let (access, target) = parse_access_scope(entry, false)?;
        Ok(Self::Buffer { access, target })
    }

    pub fn parse_network(entry: &str) -> Result<Self, CapabilityError> {
        let ok = !entry.is_empty()
            && entry
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if ok {
            Ok(Self::Network {
                host: entry.to_ascii_lowercase(),
            })
        } else {
            Err(CapabilityError::BadHost(entry.to_owned()))
        }
    }
}

/// The set of capabilities granted to one plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    granted: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn from_permissions(permissions: &Permissions) -> Result<Self, CapabilityError> {
        let mut granted = HashSet::new();
        for e in &permissions.filesystem {
            granted.insert(Capability::parse_filesystem(e)?);
        }
        for e in &permissions.network {
            granted.insert(Capability::parse_network(e)?);
        }
        for e in &permissions.buffer {
            granted.insert(Capability::parse_buffer(e)?);
        }
        Ok(Self { granted })
    }

    /// Whether `wanted` is covered by a grant. `Write` covers `Read`.
    pub fn allows(&self, wanted: &Capability) -> bool {
        if self.granted.contains(wanted) {
            return true;
        }
        match wanted {
            Capability::Filesystem {
                access: Access::Read,
                scope,
            } => self.granted.contains(&Capability::Filesystem {
                access: Access::Write,
                scope: scope.clone(),
            }),
            Capability::Buffer {
                access: Access::Read,
                target,
            } => self.granted.contains(&Capability::Buffer {
                access: Access::Write,
                target: target.clone(),
            }),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs(access: Access, scope: &str) -> Capability {
        Capability::Filesystem {
            access,
            scope: scope.to_owned(),
        }
    }

    #[test]
    fn parses_filesystem_read() {
        assert_eq!(
            Capability::parse_filesystem("read:templates"),
            Ok(fs(Access::Read, "templates"))
        );
    }

    #[test]
    fn rejects_malformed_and_unsafe_entries() {
        assert!(matches!(
            Capability::parse_filesystem("templates"),
            Err(CapabilityError::Malformed(_))
        ));
        assert!(matches!(
            Capability::parse_filesystem("exec:templates"),
            Err(CapabilityError::UnknownAccess(..))
        ));
        for bad in ["read:", "read:../secrets", "read:/etc", "write:a/../b"] {
            assert!(
                matches!(
                    Capability::parse_filesystem(bad),
                    Err(CapabilityError::UnsafeScope(_))
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn network_hosts_are_bare_and_lowercased() {
        assert_eq!(
            Capability::parse_network("API.Crossref.org"),
            Ok(Capability::Network {
                host: "api.crossref.org".to_owned()
            })
        );
        assert!(Capability::parse_network("*").is_err());
        assert!(Capability::parse_network("https://x.org").is_err());
    }

    #[test]
    fn write_implies_read_but_not_the_reverse() {
        let p = Permissions {
            filesystem: vec!["write:out".into(), "read:templates".into()],
            ..Permissions::default()
        };
        let set = CapabilitySet::from_permissions(&p).expect("valid");
        assert!(set.allows(&fs(Access::Read, "out")));
        assert!(set.allows(&fs(Access::Write, "out")));
        assert!(set.allows(&fs(Access::Read, "templates")));
        assert!(!set.allows(&fs(Access::Write, "templates")));
        assert!(!set.allows(&fs(Access::Read, "other")));
    }

    #[test]
    fn empty_set_allows_nothing() {
        let set = CapabilitySet::default();
        assert!(!set.allows(&Capability::Network {
            host: "x.org".into()
        }));
    }
}
