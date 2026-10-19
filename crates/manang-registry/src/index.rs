//! The hosted index (`index.json`, schema 1; see
//! `docs/plans/2026-10-19-plugin-marketplace.md`): types, parsing and
//! validation. Nothing here trusts the bytes: signature verification happens
//! first (see [`crate::keys`]), and the structure is checked anyway so a
//! registry bug cannot reach the UI or the install path.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

/// The only index format this build reads.
pub const SCHEMA: u64 = 1;

pub const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_PLUGINS: usize = 2_000;
pub const MAX_VERSIONS: usize = 200;
pub const MAX_TAGS: usize = 12;
pub const MAX_PERMISSION_ENTRIES: usize = 64;
/// The largest plugin archive Manang downloads.
pub const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

/// What a plugin may touch: the manifest's `[permissions]` table, verbatim
/// (`filesystem = ["read:project"]`, `network = ["api.example.org"]`,
/// `buffer = [...]`). Unknown keys are refused: a permission the app cannot
/// show to the user must not be installable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub filesystem: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub buffer: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: String,
    #[serde(default)]
    pub published: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app: Option<String>,
    /// Path of the `.manangplugin` under the registry base URL.
    pub download: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub changelog: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub license: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub tier: String,
    /// Path of the readme under the registry base URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,
    pub latest: String,
    pub versions: Vec<VersionEntry>,
    /// Versions the registry withdrew: not offered for install or update,
    /// shown as withdrawn for an installed copy. (A takedown does not remove
    /// an installed copy: there is no revocation list yet, see ADR 0016.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yanked: Vec<String>,
}

impl PluginEntry {
    pub fn version(&self, version: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|v| v.version == version)
    }

    pub fn is_yanked(&self, version: &str) -> bool {
        self.yanked.iter().any(|v| v == version)
    }

    /// The version `latest` names (validated to exist).
    pub fn latest_version(&self) -> Option<&VersionEntry> {
        self.version(&self.latest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub schema: u64,
    pub generated: String,
    pub plugins: Vec<PluginEntry>,
}

impl Index {
    pub fn plugin(&self, id: &str) -> Option<&PluginEntry> {
        self.plugins.iter().find(|p| p.id == id)
    }
}

#[derive(Deserialize)]
struct Probe {
    schema: Option<serde_json::Value>,
}

/// Parse and validate `bytes` as an index. Unknown schema versions are
/// refused with their own error so the UI can ask the person to update Manang.
/// Entries of a tier this build does not know (anything but `wasm`) are left
/// out: a newer registry may list kinds this app cannot run.
pub fn parse_index(bytes: &[u8]) -> Result<Index, RegistryError> {
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err(RegistryError::TooLarge {
            what: "the plugin list",
            limit: MAX_INDEX_BYTES,
        });
    }
    let bad = |e: serde_json::Error| RegistryError::BadIndex(e.to_string());
    let probe: Probe = serde_json::from_slice(bytes).map_err(bad)?;
    match probe.schema.as_ref().and_then(serde_json::Value::as_u64) {
        Some(SCHEMA) => {}
        Some(found) => return Err(RegistryError::UnsupportedSchema { found }),
        None => return Err(RegistryError::BadIndex("it has no `schema`".to_owned())),
    }
    let mut index: Index = serde_json::from_slice(bytes).map_err(bad)?;
    index.plugins.retain(|p| p.tier == "wasm");
    validate(&index)?;
    Ok(index)
}

fn invalid(what: impl std::fmt::Display) -> RegistryError {
    RegistryError::BadIndex(what.to_string())
}

fn check_text(id: &str, field: &str, value: &str, max: usize) -> Result<(), RegistryError> {
    if value.chars().count() > max {
        return Err(invalid(format!("`{id}`: `{field}` is too long")));
    }
    if value.chars().any(|c| c.is_control() && c != '\n') {
        return Err(invalid(format!("`{id}`: `{field}` has control characters")));
    }
    Ok(())
}

/// A `https://` link to open outside the app.
fn check_link(id: &str, field: &str, value: &str) -> Result<(), RegistryError> {
    let ok = value.starts_with("https://")
        && value.len() <= 300
        && !value.chars().any(|c| c.is_whitespace() || c.is_control());
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("`{id}`: `{field}` is not an https link")))
    }
}

/// A path below the registry base URL: no scheme, no `..`, no absolute path.
pub fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 300
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '?', '#', '%'])
        && !path.chars().any(|c| c.is_control() || c.is_whitespace())
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn is_plugin_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.ends_with('-')
        && id.len() <= 64
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn check_permissions(id: &str, p: &Permissions) -> Result<(), RegistryError> {
    for (field, list) in [
        ("filesystem", &p.filesystem),
        ("network", &p.network),
        ("buffer", &p.buffer),
    ] {
        if list.len() > MAX_PERMISSION_ENTRIES {
            return Err(invalid(format!("`{id}`: too many `{field}` permissions")));
        }
        for entry in list {
            check_text(id, field, entry, 256)?;
        }
    }
    Ok(())
}

fn validate(index: &Index) -> Result<(), RegistryError> {
    if index.plugins.len() > MAX_PLUGINS {
        return Err(invalid("it lists too many plugins"));
    }
    check_text("index", "generated", &index.generated, 40)?;
    let mut ids = BTreeSet::new();
    for p in &index.plugins {
        let id = p.id.as_str();
        if !is_plugin_id(id) {
            return Err(invalid(format!("`{}` is not a valid plugin id", clip(id))));
        }
        if !ids.insert(id) {
            return Err(invalid(format!("`{id}` is listed twice")));
        }
        check_text(id, "name", &p.name, 80)?;
        check_text(id, "author", &p.author, 80)?;
        check_text(id, "description", &p.description, 1_000)?;
        check_text(id, "license", &p.license, 64)?;
        check_link(id, "repo", &p.repo)?;
        if let Some(home) = &p.homepage {
            check_link(id, "homepage", home)?;
        }
        if p.name.trim().is_empty() {
            return Err(invalid(format!("`{id}` has no name")));
        }
        if p.tags.len() > MAX_TAGS {
            return Err(invalid(format!("`{id}` has too many tags")));
        }
        for tag in &p.tags {
            check_text(id, "tags", tag, 32)?;
        }
        if let Some(readme) = &p.readme
            && !is_safe_relative_path(readme)
        {
            return Err(invalid(format!("`{id}`: `readme` is not a safe path")));
        }
        if p.versions.is_empty() || p.versions.len() > MAX_VERSIONS {
            return Err(invalid(format!("`{id}` has no versions or too many")));
        }
        let mut seen = BTreeSet::new();
        for v in &p.versions {
            if Version::parse(&v.version).is_err() {
                return Err(invalid(format!(
                    "`{id}`: `{}` is not a version",
                    clip(&v.version)
                )));
            }
            if !seen.insert(v.version.as_str()) {
                return Err(invalid(format!("`{id}` lists {} twice", v.version)));
            }
            check_text(id, "published", &v.published, 40)?;
            check_text(id, "changelog", &v.changelog, 4_000)?;
            if let Some(min) = &v.min_app
                && Version::parse(min).is_err()
            {
                return Err(invalid(format!("`{id}`: `min_app` is not a version")));
            }
            if !is_safe_relative_path(&v.download) {
                return Err(invalid(format!(
                    "`{id}` {}: `download` is not a safe path",
                    v.version
                )));
            }
            if let Some(source) = &v.source {
                check_link(id, "source", source)?;
            }
            if v.size == 0 || v.size > MAX_ARTIFACT_BYTES {
                return Err(invalid(format!("`{id}` {}: bad `size`", v.version)));
            }
            if !is_sha256_hex(&v.sha256) {
                return Err(invalid(format!(
                    "`{id}` {}: `sha256` is not 64 hex digits",
                    v.version
                )));
            }
            check_permissions(id, &v.permissions)?;
        }
        if p.yanked.len() > MAX_VERSIONS || p.yanked.iter().any(|v| Version::parse(v).is_err()) {
            return Err(invalid(format!("`{id}`: `yanked` must list versions")));
        }
        if p.latest_version().is_none() {
            return Err(invalid(format!(
                "`{id}`: `latest` is not one of its versions"
            )));
        }
    }
    Ok(())
}

fn clip(s: &str) -> String {
    s.chars().take(40).collect()
}

/// Compare two version strings; `None` when either is not a version.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    Some(Version::parse(a).ok()?.cmp(&Version::parse(b).ok()?))
}

/// Whether `latest` is newer than `installed` (false when either is unreadable).
pub fn is_newer(latest: &str, installed: &str) -> bool {
    compare_versions(latest, installed) == Some(Ordering::Greater)
}

/// Whether `app` is at least `min` (true when either is unreadable: a build
/// with an odd version string must not lock people out).
pub fn app_satisfies(app: &str, min: &str) -> bool {
    compare_versions(app, min) != Some(Ordering::Less)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample_json() -> String {
        r#"{
          "schema": 1,
          "generated": "2026-10-19T08:00:00Z",
          "plugins": [{
            "id": "word-count", "name": "Word count", "author": "Fundaments.work",
            "description": "Counts words.", "repo": "https://github.com/x/y",
            "license": "MIT", "tags": ["writing"], "tier": "wasm",
            "readme": "plugins/word-count/README.md", "latest": "0.3.1",
            "versions": [{
              "version": "0.3.1", "published": "2026-10-10T12:00:00Z",
              "download": "plugins/word-count/0.3.1/word-count-0.3.1.manangplugin",
              "size": 100, "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
              "permissions": { "filesystem": ["read:project"], "network": ["api.example.org"] },
              "changelog": "Fixes CJK."
            }]
          }]
        }"#
        .to_owned()
    }

    #[test]
    fn parses_the_sample() {
        let index = parse_index(sample_json().as_bytes()).expect("valid");
        assert_eq!(index.plugins.len(), 1);
        let p = &index.plugins[0];
        assert_eq!(p.latest_version().map(|v| v.size), Some(100));
        assert_eq!(p.versions[0].permissions.network, ["api.example.org"]);
    }

    #[test]
    fn schema_two_is_refused_with_its_own_error() {
        let json = sample_json().replace("\"schema\": 1", "\"schema\": 2");
        let err = parse_index(json.as_bytes()).expect_err("schema 2");
        assert!(matches!(err, RegistryError::UnsupportedSchema { found: 2 }));
        assert!(err.to_string().contains("update Manang"));
    }

    #[test]
    fn missing_schema_and_garbage_are_bad_indexes() {
        assert!(matches!(
            parse_index(br#"{"plugins":[]}"#),
            Err(RegistryError::BadIndex(_))
        ));
        assert!(matches!(
            parse_index(b"not json"),
            Err(RegistryError::BadIndex(_))
        ));
    }

    #[test]
    fn unknown_permission_keys_are_refused() {
        let json = sample_json().replace("\"network\"", "\"fs.read\": \"project\", \"network\"");
        assert!(parse_index(json.as_bytes()).is_err());
    }

    #[test]
    fn unsafe_paths_and_links_are_refused() {
        for (from, to) in [
            ("plugins/word-count/README.md", "../README.md"),
            ("plugins/word-count/README.md", "/etc/passwd"),
            ("plugins/word-count/README.md", "https://evil.example/x"),
            ("https://github.com/x/y", "javascript:alert(1)"),
            ("\"latest\": \"0.3.1\"", "\"latest\": \"9.9.9\""),
            ("\"size\": 100", "\"size\": 0"),
            (
                "0000000000000000000000000000000000000000000000000000000000000000",
                "abc",
            ),
        ] {
            let json = sample_json().replace(from, to);
            assert!(parse_index(json.as_bytes()).is_err(), "{from} -> {to}");
        }
    }

    #[test]
    fn unknown_tiers_are_left_out() {
        let json = sample_json().replace("\"tier\": \"wasm\"", "\"tier\": \"future\"");
        let index = parse_index(json.as_bytes()).expect("valid");
        assert!(index.plugins.is_empty());
    }

    #[test]
    fn duplicate_ids_and_oversized_lists_are_refused() {
        let one = sample_json();
        let start = one.find("\"plugins\": [").expect("start") + "\"plugins\": [".len();
        let end = one.rfind(']').expect("end");
        let entry = &one[start..end];
        let twice = format!("{}{entry},{entry}]}}", &one[..start]);
        assert!(parse_index(twice.as_bytes()).is_err());
        let huge = vec![b' '; MAX_INDEX_BYTES as usize + 1];
        assert!(matches!(
            parse_index(&huge),
            Err(RegistryError::TooLarge { .. })
        ));
    }

    #[test]
    fn versions_compare_as_semver_not_as_text() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
        assert!(!is_newer("nonsense", "1.0.0"));
        assert!(app_satisfies("0.2.0", "0.1.0"));
        assert!(!app_satisfies("0.1.0", "0.2.0"));
    }
}
