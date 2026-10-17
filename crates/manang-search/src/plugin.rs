//! The kernel-facing command surface.
//!
//! Commands run synchronously on the caller's thread and fan CPU-bound work
//! out to a private Rayon pool; the shell should invoke them from a blocking
//! task, never from the UI thread or a Tokio worker. `search.cancel` is safe
//! to call concurrently with a running search.
//!
//! Index freshness: with an [`EventBus`] the plugin subscribes to
//! `project.changed` and drops a root's cached index whenever files are
//! created, removed or renamed (or an ignore file changes). Plain content
//! modifications do not invalidate, because file contents are always read
//! from disk at search time. Without a bus there is nothing to tell us the
//! tree changed, so cached indexes expire after [`NO_BUS_TTL`].

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_kernel::{
    Context, EventBus, Manifest, ManifestError, Payload, Plugin, PluginError, Subscription,
};
use manang_project::ProjectRoot;
use parking_lot::Mutex;
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::budget::Budget;
use crate::error::SearchError;
use crate::fuzzy;
use crate::index::IndexCache;
use crate::intext;
use crate::limits::Limits;
use crate::pattern::{Mode, PathFilter, PatternSpec, compile};
use crate::registry::Registry;
use crate::replace::{self, PlanStore, PreviewJob, Replacement};
use crate::text::{self, TextJob};

const MANIFEST: &str = include_str!("../manifest.toml");
/// Cached index lifetime when no event bus is available.
pub const NO_BUS_TTL: Duration = Duration::from_secs(2);
/// Upper bound on worker threads in the private pool.
const MAX_THREADS: usize = 8;

/// The plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the search plugin. `events` is the bus carrying `project.changed`;
/// pass `None` to run without it (indexes then expire after [`NO_BUS_TTL`]).
///
/// If the private thread pool cannot be created the plugin still loads and
/// every command reports the failure, so a bad host environment never
/// prevents startup.
pub fn plugin(events: Option<EventBus>) -> impl Plugin {
    SearchPlugin::new(events)
}

struct Watch {
    sub: Subscription,
    seen_dropped: u64,
}

/// Implements [`Plugin`] for the `search.*` commands.
pub struct SearchPlugin {
    pool: Result<Arc<ThreadPool>, String>,
    limits: Limits,
    indexes: IndexCache,
    registry: Registry,
    plans: PlanStore,
    watch: Option<Mutex<Watch>>,
}

impl SearchPlugin {
    pub fn new(events: Option<EventBus>) -> Self {
        Self::with_limits(events, Limits::default())
    }

    pub fn with_limits(events: Option<EventBus>, limits: Limits) -> Self {
        let threads = std::thread::available_parallelism()
            .map_or(2, std::num::NonZeroUsize::get)
            .min(MAX_THREADS);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("manang-search-{i}"))
            .build()
            .map(Arc::new)
            .map_err(|e| e.to_string());
        let ttl = events.is_none().then_some(NO_BUS_TTL);
        let watch = events.map(|bus| {
            Mutex::new(Watch {
                sub: bus.subscribe("project.changed"),
                seen_dropped: 0,
            })
        });
        Self {
            pool,
            indexes: IndexCache::new(ttl, limits.max_index_entries),
            limits,
            registry: Registry::default(),
            plans: PlanStore::default(),
            watch,
        }
    }

    /// Apply queued `project.changed` events to the index cache.
    fn sync_events(&self) {
        let Some(watch) = &self.watch else { return };
        let mut watch = watch.lock();
        let events = watch.sub.drain();
        let dropped = watch.sub.dropped();
        if dropped > watch.seen_dropped {
            // Events were lost; we can no longer trust any snapshot.
            watch.seen_dropped = dropped;
            self.indexes.invalidate_all();
        }
        drop(watch);
        for event in events {
            let Payload::Json(value) = event.payload else {
                self.indexes.invalidate_all();
                continue;
            };
            self.apply_change(&value);
        }
    }

    fn apply_change(&self, value: &Value) {
        let Some(root) = value.get("root").and_then(Value::as_str) else {
            self.indexes.invalidate_all();
            return;
        };
        let structural = value
            .get("changes")
            .and_then(Value::as_array)
            .is_none_or(|changes| {
                changes.iter().any(|c| {
                    let kind = c.get("kind").and_then(Value::as_str).unwrap_or("");
                    let path = c.get("path").and_then(Value::as_str).unwrap_or("");
                    kind != "modified" || path.ends_with(".gitignore") || path.ends_with(".ignore")
                })
            });
        if structural {
            self.indexes.invalidate(Path::new(root));
        }
    }

    fn pool(&self) -> Result<&ThreadPool, SearchError> {
        self.pool
            .as_deref()
            .map_err(|e| SearchError::Pool(e.clone()))
    }

    fn budget(&self, cancel: Arc<std::sync::atomic::AtomicBool>, limit: usize) -> Budget {
        Budget::new(
            cancel,
            Instant::now() + self.limits.max_duration,
            limit,
            self.limits.max_bytes_scanned,
        )
    }

    fn files(&self, a: args::Files) -> Result<Value, SearchError> {
        let started = Instant::now();
        let pool = self.pool()?;
        let root = ProjectRoot::open(Path::new(&a.root))?;
        let limit = self.limits.clamp_results(a.limit);
        let index = self.indexes.get(&root, a.hidden);
        let hits = fuzzy::rank(pool, &index.entries, &a.query, limit);
        let results: Vec<Value> = hits
            .into_iter()
            .map(|h| {
                let e = &index.entries[h.entry];
                json!({
                    "path": e.path,
                    "name_start": e.path[..e.name_start].chars().count(),
                    "score": h.score,
                    "indices": h.indices,
                })
            })
            .collect();
        Ok(json!({
            "results": results,
            "total_indexed": index.entries.len(),
            "index_truncated": index.truncated,
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        }))
    }

    fn text(&self, a: args::Text) -> Result<Value, SearchError> {
        let pool = self.pool()?;
        let root = ProjectRoot::open(Path::new(&a.root))?;
        let spec = a.spec();
        let matcher = compile(&spec, Mode::Lines, &self.limits)?;
        let filter = PathFilter::new(&a.include, &a.exclude, &self.limits)?;
        self.sync_events();
        let ticket = self.registry.begin();
        let limit = self.limits.clamp_results(a.limit);
        let budget = self.budget(ticket.cancel.clone(), limit);
        let index = self.indexes.get(&root, a.hidden);
        let outcome = text::run(
            &TextJob {
                pool,
                root: &root,
                index: &index,
                filter: &filter,
                limits: &self.limits,
                budget: &budget,
                context_lines: a.context_lines,
            },
            &matcher,
        );
        let mut value = to_value(&outcome)?;
        if let Value::Object(map) = &mut value {
            map.insert("id".to_owned(), json!(ticket.id));
        }
        Ok(value)
    }

    fn replace_preview(&self, a: args::Preview) -> Result<Value, SearchError> {
        let (text, replacement) = a.into_parts();
        let pool = self.pool()?;
        let limits = &self.limits;
        if replacement.len() > limits.max_replacement_bytes {
            return Err(SearchError::ReplacementTooLong {
                len: replacement.len(),
                limit: limits.max_replacement_bytes,
            });
        }
        let root = ProjectRoot::open(Path::new(&text.root))?;
        let spec = text.spec();
        let matcher = compile(&spec, Mode::Lines, limits)?;
        let filter = PathFilter::new(&text.include, &text.exclude, limits)?;
        self.sync_events();
        let ticket = self.registry.begin();
        let cap = text
            .limit
            .unwrap_or(limits.max_edits)
            .clamp(1, limits.max_edits);
        let budget = self.budget(ticket.cancel.clone(), cap);
        let index = self.indexes.get(&root, text.hidden);
        let replacement = Replacement {
            text: replacement,
            expand: text.regex,
        };
        let (plan_files, outcome) = replace::preview(
            &PreviewJob {
                pool,
                root: &root,
                index: &index,
                filter: &filter,
                limits,
                budget: &budget,
                replacement: &replacement,
            },
            &matcher,
        );
        let plan_id = self.plans.insert(root, plan_files);
        let mut value = to_value(&outcome)?;
        if let Value::Object(map) = &mut value {
            map.insert("id".to_owned(), json!(ticket.id));
            map.insert("plan_id".to_owned(), json!(plan_id));
        }
        Ok(value)
    }

    fn replace_apply(&self, a: &args::Apply) -> Result<Value, SearchError> {
        let pool = self.pool()?;
        let plan = self.plans.take(&a.plan_id)?;
        let results = replace::apply(pool, &plan, a.files.as_deref(), &self.limits);
        let applied = results.iter().filter(|r| r.ok).count();
        let failed = results.len() - applied;
        // Freshly written files may change what the index should contain.
        self.indexes.invalidate(plan.root.path());
        Ok(json!({ "applied": applied, "failed": failed, "files": results }))
    }

    fn in_text(&self, a: &args::InText) -> Result<Value, SearchError> {
        let spec = PatternSpec {
            query: &a.query,
            regex: a.regex,
            case_sensitive: a.case_sensitive,
            whole_word: a.whole_word,
        };
        let limit = self
            .limits
            .clamp_results(a.limit.or(Some(self.limits.max_results)));
        to_value(&intext::find(&a.text, &spec, limit, &self.limits)?)
    }

    fn cancel(&self, a: &args::Cancel) -> Value {
        let cancelled = match a.id {
            Some(id) => usize::from(self.registry.cancel(id)),
            None => self.registry.cancel_all(),
        };
        json!({ "cancelled": cancelled })
    }
}

fn to_value<T: Serialize>(value: &T) -> Result<Value, SearchError> {
    serde_json::to_value(value)
        .map_err(|e| SearchError::io("serialising a response", std::io::Error::other(e)))
}

fn failed(e: SearchError) -> PluginError {
    PluginError::Failed(e.to_string())
}

/// Decode command arguments; no payload means `{}`.
fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, PluginError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => {
            return Err(PluginError::Failed("expected a JSON payload".to_owned()));
        }
    };
    serde_json::from_value(value).map_err(|e| PluginError::Failed(format!("bad arguments: {e}")))
}

mod args {
    use serde::Deserialize;

    use crate::pattern::PatternSpec;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Files {
        pub root: String,
        pub query: String,
        #[serde(default)]
        pub limit: Option<usize>,
        /// Include dotfiles (`.git` and `.manang` stay excluded).
        #[serde(default)]
        pub hidden: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Text {
        pub root: String,
        pub query: String,
        #[serde(default)]
        pub regex: bool,
        #[serde(default)]
        pub case_sensitive: bool,
        #[serde(default)]
        pub whole_word: bool,
        #[serde(default)]
        pub include: Vec<String>,
        #[serde(default)]
        pub exclude: Vec<String>,
        #[serde(default)]
        pub limit: Option<usize>,
        #[serde(default)]
        pub context_lines: usize,
        #[serde(default)]
        pub hidden: bool,
    }

    impl Text {
        pub fn spec(&self) -> PatternSpec<'_> {
            PatternSpec {
                query: &self.query,
                regex: self.regex,
                case_sensitive: self.case_sensitive,
                whole_word: self.whole_word,
            }
        }
    }

    /// `Text` plus the replacement. Written out in full because serde cannot
    /// combine `flatten` with `deny_unknown_fields`.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Preview {
        pub root: String,
        pub query: String,
        #[serde(default)]
        pub regex: bool,
        #[serde(default)]
        pub case_sensitive: bool,
        #[serde(default)]
        pub whole_word: bool,
        #[serde(default)]
        pub include: Vec<String>,
        #[serde(default)]
        pub exclude: Vec<String>,
        #[serde(default)]
        pub limit: Option<usize>,
        #[serde(default)]
        pub hidden: bool,
        pub replacement: String,
    }

    impl Preview {
        pub fn into_parts(self) -> (Text, String) {
            (
                Text {
                    root: self.root,
                    query: self.query,
                    regex: self.regex,
                    case_sensitive: self.case_sensitive,
                    whole_word: self.whole_word,
                    include: self.include,
                    exclude: self.exclude,
                    limit: self.limit,
                    context_lines: 0,
                    hidden: self.hidden,
                },
                self.replacement,
            )
        }
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Apply {
        pub plan_id: String,
        /// Apply only these project-relative paths of the plan.
        #[serde(default)]
        pub files: Option<Vec<String>>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Cancel {
        /// Omit to cancel every running search.
        #[serde(default)]
        pub id: Option<u64>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct InText {
        pub text: String,
        pub query: String,
        #[serde(default)]
        pub regex: bool,
        #[serde(default)]
        pub case_sensitive: bool,
        #[serde(default)]
        pub whole_word: bool,
        #[serde(default)]
        pub limit: Option<usize>,
    }
}

impl Plugin for SearchPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let value = match command {
            "search.files" => {
                self.sync_events();
                self.files(parse(input)?).map_err(failed)?
            }
            "search.text" => self.text(parse(input)?).map_err(failed)?,
            "search.cancel" => self.cancel(&parse(input)?),
            "search.replace-preview" => self.replace_preview(parse(input)?).map_err(failed)?,
            "search.replace-apply" => self.replace_apply(&parse(input)?).map_err(failed)?,
            "search.in-text" => self.in_text(&parse(input)?).map_err(failed)?,
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        };
        Ok(Payload::Json(value))
    }
}
