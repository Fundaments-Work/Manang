//! The kernel-facing command surface. See `NOTES.md` for argument shapes.

use std::path::Path;
use std::sync::Arc;

use manang_kernel::{Context, EventBus, Manifest, ManifestError, Payload, Plugin, PluginError};
use manang_project::ProjectRoot;
use parking_lot::RwLock;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::ctx::Ctx;
use crate::diff;
use crate::error::GitError;
use crate::history::{self, Shown};
use crate::identity;
use crate::remote::{self, Strategy};
use crate::restore::{self, RestoreRequest};
use crate::status;
use crate::versions;
use crate::writes::{self, CommitRequest, SyncAction};

const MANIFEST: &str = include_str!("../manifest.toml");

/// Topic emitted after every successful write.
pub const TOPIC_CHANGED: &str = "git.changed";

/// The plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the git plugin. `events` receives `git.changed {root}` after writes.
pub fn plugin(events: Option<EventBus>) -> impl Plugin {
    GitPlugin::new(events)
}

/// Implements [`Plugin`] for the `git.*` commands.
pub struct GitPlugin {
    root: RwLock<Option<Arc<ProjectRoot>>>,
    events: Option<EventBus>,
}

mod args {
    use serde::Deserialize;

    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    pub struct Base {
        pub root: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Open {
        pub root: String,
    }

    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    pub struct Diff {
        pub root: Option<String>,
        pub path: Option<String>,
        #[serde(default)]
        pub staged: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DiffText {
        pub root: Option<String>,
        pub path: String,
        #[serde(default)]
        pub staged: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Log {
        pub root: Option<String>,
        pub path: Option<String>,
        pub limit: Option<usize>,
        #[serde(default)]
        pub offset: usize,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PathOnly {
        pub root: Option<String>,
        pub path: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Show {
        pub root: Option<String>,
        pub rev: String,
        pub path: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Paths {
        pub root: Option<String>,
        pub paths: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Discard {
        pub root: Option<String>,
        pub paths: Vec<String>,
        #[serde(default)]
        pub confirm: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Commit {
        pub root: Option<String>,
        pub message: String,
        #[serde(default)]
        pub amend: bool,
        #[serde(default)]
        pub hooks: bool,
        #[serde(default)]
        pub trailers: Vec<String>,
        #[serde(default)]
        pub all: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Restore {
        pub root: Option<String>,
        pub rev: String,
        #[serde(default)]
        pub paths: Vec<String>,
        #[serde(default)]
        pub whole: bool,
        #[serde(default)]
        pub save_first: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct IdentitySet {
        pub root: Option<String>,
        pub name: String,
        pub email: String,
        #[serde(default)]
        pub global: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SaveVersion {
        pub root: Option<String>,
        pub name: Option<String>,
        pub message: Option<String>,
        /// Extra `Key: value` trailer lines, e.g. `Co-authored-by: ...`.
        #[serde(default)]
        pub trailers: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Versions {
        pub root: Option<String>,
        pub path: Option<String>,
        pub limit: Option<usize>,
        pub cursor: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RevOnly {
        pub root: Option<String>,
        pub rev: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Checkout {
        pub root: Option<String>,
        pub name: String,
        #[serde(default)]
        pub create: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RemoteSet {
        pub root: Option<String>,
        pub url: String,
    }

    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    pub struct SyncStatus {
        pub root: Option<String>,
        #[serde(default)]
        pub refresh: bool,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct GetUpdates {
        pub root: Option<String>,
        pub strategy: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Sync {
        pub root: Option<String>,
        pub action: String,
        pub remote: Option<String>,
        pub timeout_secs: Option<u64>,
        #[serde(default)]
        pub hooks: bool,
    }
}

fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, GitError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => return Err(GitError::BadArgs("expected a JSON payload".to_owned())),
    };
    serde_json::from_value(value).map_err(|e| GitError::BadArgs(e.to_string()))
}

fn json_payload<T: Serialize>(value: &T) -> Result<Payload, GitError> {
    serde_json::to_value(value)
        .map(Payload::Json)
        .map_err(GitError::backend)
}

impl GitPlugin {
    pub fn new(events: Option<EventBus>) -> Self {
        Self {
            root: RwLock::new(None),
            events,
        }
    }

    fn set_root(&self, dir: &str) -> Result<Arc<ProjectRoot>, GitError> {
        let root = Arc::new(ProjectRoot::open(Path::new(dir))?);
        *self.root.write() = Some(Arc::clone(&root));
        Ok(root)
    }

    fn root(&self, explicit: Option<&str>) -> Result<Arc<ProjectRoot>, GitError> {
        match explicit {
            Some(dir) => {
                let current = self.root.read().clone();
                match current {
                    Some(r) if r.path() == Path::new(dir) => Ok(r),
                    _ => self.set_root(dir),
                }
            }
            None => self.root.read().clone().ok_or(GitError::NoRoot),
        }
    }

    fn ctx(&self, explicit: Option<&str>) -> Result<Ctx, GitError> {
        Ctx::open(self.root(explicit)?.as_ref())
    }

    fn changed(&self, ctx_root: &ProjectRoot) {
        if let Some(bus) = &self.events {
            bus.emit(
                TOPIC_CHANGED,
                Payload::Json(json!({ "root": ctx_root.path().to_string_lossy() })),
            );
        }
    }

    /// Run a write against a repo context and announce it on success.
    fn write<T: Serialize>(
        &self,
        root: Option<&str>,
        f: impl FnOnce(&Ctx) -> Result<T, GitError>,
    ) -> Result<Payload, GitError> {
        let ctx = self.ctx(root)?;
        let out = f(&ctx)?;
        self.changed(&ctx.root);
        json_payload(&out)
    }

    fn dispatch(&self, command: &str, input: Payload) -> Result<Payload, GitError> {
        match command {
            "git.open" => {
                let a: args::Open = parse(input)?;
                let root = self.set_root(&a.root)?;
                let is_repo = Ctx::open(&root).is_ok();
                json_payload(&json!({ "root": root.path().to_string_lossy(), "is_repo": is_repo }))
            }
            "git.status" => {
                let a: args::Base = parse(input)?;
                json_payload(&status::status(&self.ctx(a.root.as_deref())?)?)
            }
            "git.diff" => {
                let a: args::Diff = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let scope = ctx.rel(a.path.as_deref().unwrap_or(""))?;
                let files: Vec<_> = diff::collect(&ctx, &scope, a.staged)?
                    .iter()
                    .map(diff::diff_sides)
                    .collect();
                json_payload(&json!({ "staged": a.staged, "files": files }))
            }
            "git.diff-text" => {
                let a: args::DiffText = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let scope = ctx.rel(&a.path)?;
                let text: String = diff::collect(&ctx, &scope, a.staged)?
                    .iter()
                    .map(diff::unified)
                    .collect();
                json_payload(&json!({ "text": text }))
            }
            "git.log" => {
                let a: args::Log = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let rel = ctx.rel(a.path.as_deref().unwrap_or(""))?;
                json_payload(&history::log(&ctx, &rel, a.limit.unwrap_or(50), a.offset)?)
            }
            "git.blame" => {
                let a: args::PathOnly = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let rel = ctx.rel(&a.path)?;
                json_payload(&history::blame(&ctx, &rel)?)
            }
            "git.branches" => {
                let a: args::Base = parse(input)?;
                json_payload(&history::branches(&self.ctx(a.root.as_deref())?)?)
            }
            "git.show" => {
                let a: args::Show = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let rel = ctx.rel(&a.path)?;
                match history::show(&ctx, &a.rev, &rel)? {
                    Shown::Text(text) => {
                        json_payload(&json!({ "rev": a.rev, "path": rel, "text": text }))
                    }
                    Shown::Binary(bytes) => Ok(Payload::Binary(Arc::from(bytes))),
                }
            }
            "git.stage" => {
                let a: args::Paths = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    writes::stage(c, &a.paths).map(|n| json!({ "staged": n }))
                })
            }
            "git.unstage" => {
                let a: args::Paths = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    writes::unstage(c, &a.paths).map(|n| json!({ "unstaged": n }))
                })
            }
            "git.discard" => {
                let a: args::Discard = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    writes::discard(c, &a.paths, a.confirm)
                })
            }
            "git.commit" => {
                let a: args::Commit = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    writes::commit_request(
                        c,
                        &CommitRequest {
                            message: &a.message,
                            amend: a.amend,
                            hooks: a.hooks,
                            trailers: &a.trailers,
                            all: a.all,
                        },
                    )
                })
            }
            "git.restore" => {
                let a: args::Restore = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    restore::restore(
                        c,
                        &RestoreRequest {
                            rev: &a.rev,
                            paths: &a.paths,
                            whole: a.whole,
                            save_first: a.save_first,
                        },
                    )
                })
            }
            "git.identity-get" => {
                let a: args::Base = parse(input)?;
                json_payload(&identity::get(&self.ctx(a.root.as_deref())?)?)
            }
            "git.identity-set" => {
                let a: args::IdentitySet = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                identity::set(&ctx, &a.name, &a.email, a.global)?;
                json_payload(&json!({ "ok": true }))
            }
            "git.save-version" => {
                let a: args::SaveVersion = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    versions::save_version(c, a.name.as_deref(), a.message.as_deref(), &a.trailers)
                })
            }
            "git.versions" => {
                let a: args::Versions = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let rel = ctx.rel(a.path.as_deref().unwrap_or(""))?;
                json_payload(&versions::versions(
                    &ctx,
                    &rel,
                    a.limit.unwrap_or(50),
                    a.cursor.as_deref(),
                )?)
            }
            "git.version-files" => {
                let a: args::RevOnly = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                json_payload(
                    &json!({ "rev": a.rev, "files": versions::version_files(&ctx, &a.rev)? }),
                )
            }
            "git.version-diff" => {
                let a: args::Show = parse(input)?;
                let ctx = self.ctx(a.root.as_deref())?;
                let rel = ctx.rel(&a.path)?;
                json_payload(&versions::version_diff(&ctx, &a.rev, &rel)?)
            }
            "git.checkout-branch" => {
                let a: args::Checkout = parse(input)?;
                self.write(a.root.as_deref(), |c| {
                    writes::checkout_branch(c, &a.name, a.create)
                        .map(|()| json!({ "branch": a.name, "created": a.create }))
                })
            }
            "git.init" => {
                let a: args::Base = parse(input)?;
                let root = self.root(a.root.as_deref())?;
                writes::init(&root)?;
                self.changed(&root);
                let ctx = Ctx::open(&root)?;
                json_payload(&status::status(&ctx)?)
            }
            "git.remote-get" => {
                let a: args::Base = parse(input)?;
                json_payload(&remote::remote_get(&self.ctx(a.root.as_deref())?)?)
            }
            "git.remote-set" => {
                let a: args::RemoteSet = parse(input)?;
                self.write(a.root.as_deref(), |c| remote::remote_set(c, &a.url))
            }
            "git.sync-status" => {
                let a: args::SyncStatus = parse(input)?;
                json_payload(&remote::sync_status(
                    &self.ctx(a.root.as_deref())?,
                    a.refresh,
                )?)
            }
            "git.share" => {
                let a: args::Base = parse(input)?;
                self.write(a.root.as_deref(), remote::share)
            }
            "git.get-updates" => {
                let a: args::GetUpdates = parse(input)?;
                let strategy = match a.strategy.as_deref() {
                    None => Strategy::Merge,
                    Some(s) => Strategy::parse(s).ok_or_else(|| {
                        GitError::BadArgs("`strategy` must be ff-only or merge".to_owned())
                    })?,
                };
                self.write(a.root.as_deref(), |c| remote::get_updates(c, strategy))
            }
            "git.sync" => {
                let a: args::Sync = parse(input)?;
                let action = SyncAction::parse(&a.action).ok_or_else(|| {
                    GitError::BadArgs("`action` must be fetch, pull or push".to_owned())
                })?;
                self.write(a.root.as_deref(), |c| {
                    writes::sync(c, action, a.remote.as_deref(), a.timeout_secs, a.hooks)
                })
            }
            other => Err(GitError::BadArgs(format!("unknown command `{other}`"))),
        }
    }
}

impl Plugin for GitPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        if !command.starts_with("git.") {
            return Err(PluginError::UnknownCommand(command.to_owned()));
        }
        self.dispatch(command, input).map_err(PluginError::from)
    }
}
