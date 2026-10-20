//! Debounced project watcher that publishes change events on the kernel bus.
//!
//! Topics (payloads are JSON):
//!
//! * `project.changed`: `{ "root": "<abs path>", "changes": [Change, ...] }`
//!   where `Change` is `{ "path": "a/b.typ", "kind": "created" | "modified" |
//!   "removed" | "renamed", "from": "old/path"? }`. Paths are project-relative
//!   and `/`-separated. One event is emitted per debounce window.
//! * `project.watch-error`: `{ "root": "...", "message": "..." }`
//!
//! `.git/`, `.manang/` and anything matched by the root `.gitignore` are
//! filtered out so build output and editor temp files do not cause churn.
//!
//! The watcher runs on notify's own OS threads, never on the UI thread or a
//! Tokio worker; delivery to subscribers is non-blocking (see
//! [`manang_kernel::EventBus`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use manang_kernel::{EventBus, Payload};
use notify_debouncer_full::notify::event::{ModifyKind, RenameMode};
use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::json;

use crate::error::ProjectError;
use crate::paths::{STATE_DIR, to_slash};

pub const TOPIC_CHANGED: &str = "project.changed";
pub const TOPIC_ERROR: &str = "project.watch-error";
/// Default debounce window.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

fn load_ignore(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    // A malformed .gitignore only loses filtering; ignore the parse error.
    let _ = builder.add(root.join(".gitignore"));
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// The watcher's ignore rules as a reusable filter: `.git/`, `.manang/` and
/// everything the root `.gitignore` matches. The live plugin applies the same
/// filter to remote paths so both sides agree on what is "not the project".
#[derive(Debug)]
pub struct IgnoreFilter {
    root: PathBuf,
    ignore: Gitignore,
}

impl IgnoreFilter {
    /// Load the filter for `root` (read the root `.gitignore`; a missing or
    /// malformed file only loses filtering).
    #[must_use]
    pub fn load(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            ignore: load_ignore(root),
        }
    }

    /// Whether the project-relative path `rel` is filtered out. A path that is
    /// not plainly relative counts as ignored.
    #[must_use]
    pub fn is_ignored(&self, rel: &Path, is_dir: bool) -> bool {
        if rel.has_root()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return true;
        }
        is_internal(rel)
            || self
                .ignore
                .matched_path_or_any_parents(self.root.join(rel), is_dir)
                .is_ignore()
    }
}

fn relative(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

fn is_internal(rel: &Path) -> bool {
    rel.components()
        .any(|c| c.as_os_str() == ".git" || c.as_os_str() == STATE_DIR)
}

/// Turn raw debounced events into filtered, project-relative changes.
fn translate(root: &Path, ignore: &Gitignore, events: &[DebouncedEvent]) -> Vec<Change> {
    let keep = |rel: &Path, abs: &Path| {
        !is_internal(rel)
            && !ignore
                .matched_path_or_any_parents(abs, abs.is_dir())
                .is_ignore()
    };
    let mut out: Vec<Change> = Vec::new();
    let mut push = |change: Change| {
        if !out.contains(&change) {
            out.push(change);
        }
    };
    for event in events {
        let paths = &event.event.paths;
        let kind = match event.event.kind {
            EventKind::Create(_) => ChangeKind::Created,
            EventKind::Remove(_) => ChangeKind::Removed,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => ChangeKind::Renamed,
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => ChangeKind::Removed,
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => ChangeKind::Created,
            EventKind::Modify(_) | EventKind::Any => ChangeKind::Modified,
            EventKind::Access(_) | EventKind::Other => continue,
        };
        if kind == ChangeKind::Renamed {
            if let [from, to] = paths.as_slice() {
                let (Some(from_rel), Some(to_rel)) = (relative(root, from), relative(root, to))
                else {
                    continue;
                };
                if let (true, Some(path), Some(from)) = (
                    keep(&to_rel, to) || keep(&from_rel, from),
                    to_slash(&to_rel),
                    to_slash(&from_rel),
                ) {
                    push(Change {
                        path,
                        kind,
                        from: Some(from),
                    });
                }
            }
            continue;
        }
        for abs in paths {
            let Some(rel) = relative(root, abs) else {
                continue;
            };
            if rel.as_os_str().is_empty() || !keep(&rel, abs) {
                continue;
            }
            if let Some(path) = to_slash(&rel) {
                push(Change {
                    path,
                    kind,
                    from: None,
                });
            }
        }
    }
    out
}

/// A running watcher. Dropping it stops watching.
pub struct ProjectWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl std::fmt::Debug for ProjectWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProjectWatcher")
    }
}

impl ProjectWatcher {
    /// Watch `root` (which must be canonical) recursively and publish to `bus`.
    pub fn start(root: &Path, bus: EventBus, debounce: Duration) -> Result<Self, ProjectError> {
        let root = root.to_path_buf();
        let ignore = Arc::new(RwLock::new(load_ignore(&root)));
        let handler_root = root.clone();
        let mut debouncer =
            new_debouncer(
                debounce,
                None,
                move |result: DebounceEventResult| match result {
                    Ok(events) => {
                        let events: Vec<DebouncedEvent> = events;
                        let gitignore_touched = events.iter().any(|e| {
                            e.event
                                .paths
                                .iter()
                                .any(|p| p.file_name().is_some_and(|n| n == ".gitignore"))
                        });
                        if gitignore_touched {
                            *ignore.write() = load_ignore(&handler_root);
                        }
                        let changes = translate(&handler_root, &ignore.read(), &events);
                        if !changes.is_empty() {
                            bus.emit(
                                TOPIC_CHANGED,
                                Payload::Json(json!({
                                    "root": handler_root.to_string_lossy(),
                                    "changes": changes,
                                })),
                            );
                        }
                    }
                    Err(errors) => {
                        let message = errors
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("; ");
                        bus.emit(
                            TOPIC_ERROR,
                            Payload::Json(json!({
                                "root": handler_root.to_string_lossy(),
                                "message": message,
                            })),
                        );
                    }
                },
            )
            .map_err(|e| ProjectError::Watch(e.to_string()))?;
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| ProjectError::Watch(e.to_string()))?;
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use notify_debouncer_full::notify::Event;
    use notify_debouncer_full::notify::event::{CreateKind, DataChange, RemoveKind};

    use super::*;

    fn ev(kind: EventKind, paths: &[&str]) -> DebouncedEvent {
        let mut event = Event::new(kind);
        for p in paths {
            event = event.add_path(PathBuf::from(p));
        }
        DebouncedEvent::new(event, Instant::now())
    }

    #[test]
    fn ignore_filter_matches_the_watcher_rules() {
        let tmp = tempfile::tempdir().expect("tmp");
        std::fs::write(tmp.path().join(".gitignore"), "build/\n*.aux\n").expect("gi");
        let f = IgnoreFilter::load(tmp.path());
        assert!(f.is_ignored(Path::new("build/out.pdf"), false));
        assert!(f.is_ignored(Path::new("x.aux"), false));
        assert!(f.is_ignored(Path::new(".git/config"), false));
        assert!(f.is_ignored(Path::new(".manang/trash/1"), false));
        assert!(f.is_ignored(Path::new("../x"), false));
        assert!(!f.is_ignored(Path::new("a/main.typ"), false));
    }

    #[test]
    fn translates_filters_and_dedupes() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join(".gitignore"), "build/\n*.aux\n").expect("gi");
        let ignore = load_ignore(&root);
        let p = |s: &str| root.join(s).to_string_lossy().into_owned();
        let events = vec![
            ev(EventKind::Create(CreateKind::File), &[&p("a.typ")]),
            ev(EventKind::Create(CreateKind::File), &[&p("a.typ")]),
            ev(
                EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                &[&p("b.typ")],
            ),
            ev(EventKind::Remove(RemoveKind::File), &[&p("c.typ")]),
            ev(EventKind::Create(CreateKind::File), &[&p("build/out.pdf")]),
            ev(EventKind::Create(CreateKind::File), &[&p("x.aux")]),
            ev(
                EventKind::Create(CreateKind::File),
                &[&p(".git/index.lock")],
            ),
            ev(
                EventKind::Create(CreateKind::File),
                &[&p(".manang/trash/1")],
            ),
            ev(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &[&p("old.typ"), &p("new.typ")],
            ),
            ev(
                EventKind::Access(notify_debouncer_full::notify::event::AccessKind::Any),
                &[&p("a.typ")],
            ),
            ev(EventKind::Create(CreateKind::File), &["/elsewhere/z"]),
        ];
        let changes = translate(&root, &ignore, &events);
        let got: Vec<_> = changes
            .iter()
            .map(|c| (c.path.as_str(), c.kind, c.from.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("a.typ", ChangeKind::Created, None),
                ("b.typ", ChangeKind::Modified, None),
                ("c.typ", ChangeKind::Removed, None),
                ("new.typ", ChangeKind::Renamed, Some("old.typ")),
            ]
        );
    }
}
