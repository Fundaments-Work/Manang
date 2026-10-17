//! The project registry: every project the user has opened or registered, with
//! metadata, persisted as human-readable TOML in the user config dir.
//!
//! ```toml
//! [[projects]]
//! path = "/home/ada/papers/thesis"
//! name = "Thesis"
//! engine = "typst"
//! created = 1760000000       # unix seconds
//! last_opened = 1760600000
//! pinned = true
//! archived = false
//! tags = ["phd", "draft"]
//! ```
//!
//! The registry is a list, not a copy: forgetting a project never touches its
//! folder, and a folder that has since gone is reported as missing, not
//! removed. Writes are atomic. A missing or corrupt file reads as empty.
//! Thumbnails live in the user cache dir, keyed by a hash of the path.

use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::config::Engine;
use crate::error::ProjectError;
use crate::fsops::atomic_write;

/// File name inside the config directory.
pub const REGISTRY_FILE: &str = "projects.toml";
/// Most projects the registry holds.
pub const MAX_PROJECTS: usize = 2000;
/// Longest display name, in characters.
pub const MAX_NAME_CHARS: usize = 120;
/// Most tags on one project, and the longest tag in characters.
pub const MAX_TAGS: usize = 16;
pub const MAX_TAG_CHARS: usize = 32;
/// Largest thumbnail kept, in bytes.
pub const MAX_THUMBNAIL_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub path: String,
    pub name: String,
    pub engine: Engine,
    pub created: u64,
    pub last_opened: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RegistryFile {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
}

/// Fields `project.update` may change; `None` leaves a field alone.
#[derive(Debug, Default, Clone)]
pub struct Patch {
    pub name: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Most recently opened first.
    #[default]
    Modified,
    Name,
    /// Newest first.
    Created,
}

#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub query: Option<String>,
    pub sort: Sort,
    pub include_archived: bool,
}

/// Keep `path` as the registry key: no trailing separators.
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    let stripped = trimmed.trim_end_matches(['/', '\\']);
    if stripped.is_empty() && !trimmed.is_empty() {
        trimmed[..1].to_owned()
    } else {
        stripped.to_owned()
    }
}

fn invalid(message: impl Into<String>) -> ProjectError {
    ProjectError::io(
        "invalid project metadata",
        std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into()),
    )
}

/// Validate and tidy a display name.
pub fn clean_name(name: &str) -> Result<String, ProjectError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("the name cannot be empty"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "the name is longer than {MAX_NAME_CHARS} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(invalid("the name cannot contain control characters"));
    }
    Ok(name.to_owned())
}

/// Validate tags: trimmed, non-empty, unique (case-insensitively), bounded.
pub fn clean_tags(tags: &[String]) -> Result<Vec<String>, ProjectError> {
    let mut out: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > MAX_TAG_CHARS || tag.chars().any(char::is_control) {
            return Err(invalid(format!(
                "a tag must be at most {MAX_TAG_CHARS} characters with no control characters"
            )));
        }
        if !out.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            out.push(tag.to_owned());
        }
    }
    if out.len() > MAX_TAGS {
        return Err(invalid(format!("at most {MAX_TAGS} tags per project")));
    }
    Ok(out)
}

/// Filter and order entries for `project.list`.
pub fn select(entries: &[ProjectEntry], q: &ListQuery) -> Vec<ProjectEntry> {
    let needle = q
        .query
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let mut out: Vec<ProjectEntry> = entries
        .iter()
        .filter(|e| q.include_archived || !e.archived)
        .filter(|e| {
            needle.as_deref().is_none_or(|n| {
                e.name.to_lowercase().contains(n)
                    || e.path.to_lowercase().contains(n)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(n))
            })
        })
        .cloned()
        .collect();
    match q.sort {
        Sort::Modified => out.sort_by(|a, b| {
            b.last_opened
                .cmp(&a.last_opened)
                .then_with(|| a.path.cmp(&b.path))
        }),
        Sort::Name => out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        }),
        Sort::Created => {
            out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.path.cmp(&b.path)));
        }
    }
    out
}

/// The persisted registry.
#[derive(Debug)]
pub struct Registry {
    file: PathBuf,
    lock: Mutex<()>,
}

impl Registry {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            file: config_dir.as_ref().join(REGISTRY_FILE),
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> Vec<ProjectEntry> {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| toml::from_str::<RegistryFile>(&text).ok())
            .map(|f| f.projects)
            .unwrap_or_default()
    }

    fn write(&self, projects: Vec<ProjectEntry>) -> Result<(), ProjectError> {
        let text = toml::to_string_pretty(&RegistryFile { projects })
            .map_err(|e| ProjectError::io("serialising the registry", std::io::Error::other(e)))?;
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ProjectError::io("creating the config directory", e))?;
        }
        atomic_write(&self.file, text.as_bytes())
    }

    pub fn all(&self) -> Vec<ProjectEntry> {
        let _guard = self.lock.lock();
        self.read()
    }

    pub fn get(&self, path: &str) -> Option<ProjectEntry> {
        let key = normalize_path(path);
        let _guard = self.lock.lock();
        self.read().into_iter().find(|e| e.path == key)
    }

    /// Add `path` if it is not known (keeping everything if it is).
    pub fn register(
        &self,
        path: &str,
        name: &str,
        engine: Engine,
        now: u64,
    ) -> Result<ProjectEntry, ProjectError> {
        self.upsert(path, name, engine, now, false)
    }

    /// Record that `path` was just opened; registers it first when needed.
    /// A name the user chose is kept.
    pub fn opened(
        &self,
        path: &str,
        name: &str,
        engine: Engine,
        now: u64,
    ) -> Result<ProjectEntry, ProjectError> {
        self.upsert(path, name, engine, now, true)
    }

    fn upsert(
        &self,
        path: &str,
        name: &str,
        engine: Engine,
        now: u64,
        touch: bool,
    ) -> Result<ProjectEntry, ProjectError> {
        let key = normalize_path(path);
        if key.is_empty() {
            return Err(invalid("the path is empty"));
        }
        let _guard = self.lock.lock();
        let mut projects = self.read();
        if let Some(entry) = projects.iter_mut().find(|e| e.path == key) {
            let changed = entry.engine != engine || touch;
            entry.engine = engine;
            if touch {
                entry.last_opened = now;
            }
            let found = entry.clone();
            if changed {
                self.write(projects)?;
            }
            return Ok(found);
        }
        if projects.len() >= MAX_PROJECTS {
            return Err(invalid(format!(
                "the registry is full ({MAX_PROJECTS} projects); remove some first"
            )));
        }
        let entry = ProjectEntry {
            path: key,
            name: clean_name(name).unwrap_or_else(|_| "project".to_owned()),
            engine,
            created: now,
            last_opened: now,
            pinned: false,
            archived: false,
            tags: Vec::new(),
        };
        projects.push(entry.clone());
        self.write(projects)?;
        Ok(entry)
    }

    /// Apply `patch`; `Ok(None)` when the path is not registered.
    pub fn update(&self, path: &str, patch: Patch) -> Result<Option<ProjectEntry>, ProjectError> {
        let name = patch.name.as_deref().map(clean_name).transpose()?;
        let tags = patch.tags.as_deref().map(clean_tags).transpose()?;
        let key = normalize_path(path);
        let _guard = self.lock.lock();
        let mut projects = self.read();
        let Some(entry) = projects.iter_mut().find(|e| e.path == key) else {
            return Ok(None);
        };
        if let Some(name) = name {
            entry.name = name;
        }
        if let Some(pinned) = patch.pinned {
            entry.pinned = pinned;
        }
        if let Some(archived) = patch.archived {
            entry.archived = archived;
        }
        if let Some(tags) = tags {
            entry.tags = tags;
        }
        let updated = entry.clone();
        self.write(projects)?;
        Ok(Some(updated))
    }

    /// Drop `path` from the list. Never touches the folder. Returns whether it
    /// was present.
    pub fn forget(&self, path: &str) -> Result<bool, ProjectError> {
        let key = normalize_path(path);
        let _guard = self.lock.lock();
        let mut projects = self.read();
        let before = projects.len();
        projects.retain(|e| e.path != key);
        let removed = projects.len() != before;
        if removed {
            self.write(projects)?;
        }
        Ok(removed)
    }

    /// Seed from entries that already exist (the legacy recent list). Does
    /// nothing once the registry file exists.
    pub fn import(&self, entries: Vec<ProjectEntry>) -> Result<(), ProjectError> {
        let _guard = self.lock.lock();
        if self.file.is_file() || entries.is_empty() {
            return Ok(());
        }
        self.write(entries)
    }
}

/// Thumbnails: small SVG files in the user cache dir.
#[derive(Debug)]
pub struct Thumbnails {
    dir: PathBuf,
}

/// FNV-1a, 64 bit: stable across platforms and Rust versions, unlike
/// `DefaultHasher`, so cached files stay findable.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

impl Thumbnails {
    pub fn new(cache_dir: impl AsRef<Path>) -> Self {
        Self {
            dir: cache_dir.as_ref().join("thumbnails"),
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        let key = normalize_path(path);
        self.dir.join(format!("{:016x}.svg", fnv1a(key.as_bytes())))
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        fs::read(self.file(path)).ok()
    }

    /// Store `svg` for `path`. Rejects anything that is not a bounded SVG
    /// document without scripts.
    pub fn set(&self, path: &str, svg: &str) -> Result<(), ProjectError> {
        validate_svg(svg)?;
        fs::create_dir_all(&self.dir)
            .map_err(|e| ProjectError::io("creating the thumbnail cache", e))?;
        atomic_write(&self.file(path), svg.as_bytes())
    }

    pub fn remove(&self, path: &str) {
        let _ = fs::remove_file(self.file(path));
    }

    pub fn exists(&self, path: &str) -> bool {
        self.file(path).is_file()
    }
}

/// Thumbnails are shown as images, which already blocks scripts; this keeps
/// junk and active content out of the cache anyway.
pub fn validate_svg(svg: &str) -> Result<(), ProjectError> {
    if svg.len() > MAX_THUMBNAIL_BYTES {
        return Err(invalid(format!(
            "the thumbnail is {} bytes, over the {MAX_THUMBNAIL_BYTES} byte limit",
            svg.len()
        )));
    }
    let head = svg.trim_start();
    if !(head.starts_with("<svg") || head.starts_with("<?xml")) {
        return Err(invalid("the thumbnail is not an SVG document"));
    }
    let lower = svg.to_ascii_lowercase();
    if lower.contains("<script") || lower.contains("<foreignobject") {
        return Err(invalid("the thumbnail contains active content"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, name: &str, opened: u64) -> ProjectEntry {
        ProjectEntry {
            path: path.to_owned(),
            name: name.to_owned(),
            engine: Engine::Typst,
            created: opened / 2,
            last_opened: opened,
            pinned: false,
            archived: false,
            tags: Vec::new(),
        }
    }

    #[test]
    fn select_filters_and_sorts() {
        let mut a = entry("/p/alpha", "Alpha", 30);
        a.tags = vec!["thesis".into()];
        let mut b = entry("/p/Beta", "beta", 10);
        b.archived = true;
        let c = entry("/q/gamma", "Gamma", 20);
        let all = vec![a, b, c];
        let names = |q: &ListQuery| -> Vec<String> {
            select(&all, q).into_iter().map(|e| e.name).collect()
        };
        assert_eq!(names(&ListQuery::default()), ["Alpha", "Gamma"]);
        let archived = ListQuery {
            include_archived: true,
            sort: Sort::Name,
            ..ListQuery::default()
        };
        assert_eq!(names(&archived), ["Alpha", "beta", "Gamma"]);
        let by_tag = ListQuery {
            query: Some(" THES ".into()),
            ..ListQuery::default()
        };
        assert_eq!(names(&by_tag), ["Alpha"]);
        let by_path = ListQuery {
            query: Some("/q/".into()),
            ..ListQuery::default()
        };
        assert_eq!(names(&by_path), ["Gamma"]);
        let created = ListQuery {
            sort: Sort::Created,
            include_archived: true,
            ..ListQuery::default()
        };
        assert_eq!(names(&created), ["Alpha", "Gamma", "beta"]);
    }

    #[test]
    fn register_update_forget_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reg = Registry::new(dir.path());
        assert!(reg.all().is_empty());
        let e = reg
            .register("/p/a/", "A", Engine::Latex, 5)
            .expect("register");
        assert_eq!(e.path, "/p/a");
        reg.update(
            "/p/a",
            Patch {
                pinned: Some(true),
                tags: Some(vec![" x ".into(), "X".into(), String::new()]),
                ..Patch::default()
            },
        )
        .expect("update");
        // Registering twice keeps the metadata.
        let again = reg
            .register("/p/a", "Other", Engine::Latex, 9)
            .expect("again");
        assert!(again.pinned && again.name == "A" && again.created == 5);
        assert_eq!(again.tags, ["x"]);
        let opened = reg
            .opened("/p/a", "Renamed in toml", Engine::Latex, 50)
            .expect("opened");
        assert_eq!((opened.last_opened, opened.name.as_str()), (50, "A"));
        assert!(
            reg.update("/nope", Patch::default())
                .expect("update")
                .is_none()
        );
        assert!(reg.forget("/p/a").expect("forget"));
        assert!(!reg.forget("/p/a").expect("forget"));
    }

    #[test]
    fn validation_rejects_bad_metadata() {
        assert!(clean_name("  ").is_err());
        assert!(clean_name("a\nb").is_err());
        assert!(clean_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert_eq!(clean_name("  Thesis ").expect("ok"), "Thesis");
        assert!(clean_tags(&["x".repeat(MAX_TAG_CHARS + 1)]).is_err());
        let many: Vec<String> = (0..=MAX_TAGS).map(|i| format!("t{i}")).collect();
        assert!(clean_tags(&many).is_err());
    }

    #[test]
    fn corrupt_file_reads_empty_and_recovers() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(REGISTRY_FILE), "not = [toml").expect("write");
        let reg = Registry::new(dir.path());
        assert!(reg.all().is_empty());
        reg.register("/a", "a", Engine::Typst, 1).expect("recovers");
        assert_eq!(reg.all().len(), 1);
        let text = fs::read_to_string(dir.path().join(REGISTRY_FILE)).expect("read");
        assert!(text.contains("[[projects]]") && text.contains("engine = \"typst\""));
    }

    #[test]
    fn thumbnails_are_validated_keyed_and_removable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let thumbs = Thumbnails::new(dir.path());
        assert!(thumbs.get("/a").is_none());
        thumbs.set("/a", "<svg xmlns=\"x\"/>").expect("set");
        assert_eq!(
            thumbs.get("/a/").as_deref(),
            Some(&b"<svg xmlns=\"x\"/>"[..])
        );
        assert!(thumbs.get("/b").is_none());
        assert!(thumbs.set("/a", "hello").is_err());
        assert!(thumbs.set("/a", "<svg><script>1</script></svg>").is_err());
        let big = format!("<svg>{}</svg>", "x".repeat(MAX_THUMBNAIL_BYTES));
        assert!(thumbs.set("/a", &big).is_err());
        thumbs.remove("/a");
        assert!(!thumbs.exists("/a"));
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn engine() -> impl Strategy<Value = Engine> {
            prop_oneof![
                Just(Engine::Typst),
                Just(Engine::Latex),
                Just(Engine::Markdown)
            ]
        }

        fn arb_entry() -> impl Strategy<Value = ProjectEntry> {
            (
                "\\PC{1,40}",
                "\\PC{1,40}",
                engine(),
                0..=i64::MAX as u64,
                0..=i64::MAX as u64,
                any::<bool>(),
                any::<bool>(),
                proptest::collection::vec("\\PC{0,20}", 0..6),
            )
                .prop_map(
                    |(path, name, engine, created, last_opened, pinned, archived, tags)| {
                        ProjectEntry {
                            path,
                            name,
                            engine,
                            created,
                            last_opened,
                            pinned,
                            archived,
                            tags,
                        }
                    },
                )
        }

        proptest! {
            #[test]
            fn toml_round_trip(projects in proptest::collection::vec(arb_entry(), 0..12)) {
                let text = toml::to_string_pretty(&RegistryFile { projects: projects.clone() })
                    .expect("serialise");
                let back: RegistryFile = toml::from_str(&text).expect("parse");
                prop_assert_eq!(back.projects, projects);
            }

            #[test]
            fn select_never_invents_or_duplicates(
                projects in proptest::collection::vec(arb_entry(), 0..12),
                q in "\\PC{0,6}",
            ) {
                let query = ListQuery { query: Some(q), include_archived: true, sort: Sort::Name };
                let out = select(&projects, &query);
                prop_assert!(out.len() <= projects.len());
                prop_assert!(out.iter().all(|e| projects.contains(e)));
            }
        }
    }
}
