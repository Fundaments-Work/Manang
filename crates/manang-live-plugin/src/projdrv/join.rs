//! Joining a project room: the plan, the decisions, and taking the host's
//! files.
//!
//! Phase 1 ends when the host's manifest arrives: [`Driver::on_manifest`]
//! classifies the guest's folder (`manang_live_plugin::disk::classify_local`)
//! and emits `live.plan`; nothing on disk changes. Phase 2 is `live.resolve`:
//! [`decide`] turns the guest's choices into one decision per file (a pure
//! function, unit tested here) and [`Driver::resolve`] applies them.
//!
//! # Choices
//!
//! * `identical` and `missing` files need no choice: they are taken.
//! * A `differing` file needs one: `take-host` replaces the guest's copy with
//!   the host's (the guest's copy goes to the project trash first, when the
//!   host's content lands), `keep-mine` keeps it out of the session. A
//!   differing file with no choice (and no wildcard) is refused with
//!   `unresolved`.
//! * The wildcard path `"*"` is the single all-files confirmation. Its action
//!   covers every differing file without an explicit choice (explicit
//!   per-file choices win). **More than one replacement needs `"*"` with the
//!   action `take-host`**, even when every file was chosen explicitly:
//!   otherwise the join is refused with `needs-confirmation`.
//! * A `guest-only` file is shared only with an explicit `share`; anything
//!   else (including no choice) keeps it private.

use std::collections::BTreeMap;

use manang_live::bridge::LiveDoc;
use manang_live::envelope::seed_fingerprint;
use manang_live::manifest::ManifestEntry;
use manang_live::project::{Entry, NodeKind};
use manang_live::session::Output;

use super::{Driver, Phase};
use crate::disk::{JoinPlan, Observed, PlanClass, classify_local, sha256};
use crate::error::LiveHostError;

/// The guest's answer for one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    TakeHost,
    KeepMine,
    Share,
    KeepPrivate,
}

impl Action {
    /// Parse the spelling used on the wire.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "take-host" => Some(Self::TakeHost),
            "keep-mine" => Some(Self::KeepMine),
            "share" => Some(Self::Share),
            "keep-private" => Some(Self::KeepPrivate),
            _ => None,
        }
    }
}

/// One entry of `live.resolve {choices}`; the path `"*"` is the all-files
/// confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Choice {
    pub path: String,
    pub action: Action,
}

/// What `live.resolve` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolveOutcome {
    /// Files taken from the room, plus guest-only files that were shared.
    pub applied: usize,
    /// Paths whose local copy is replaced (saved to the trash first, when the
    /// host's content arrives).
    pub trashed: Vec<String>,
}

/// A manifest waiting for the guest's decision.
pub(crate) struct PendingPlan {
    pub entries: Vec<ManifestEntry>,
    pub plan: JoinPlan,
}

/// What happens to one file of the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Join the file as it is (identical or missing).
    Take,
    /// Replace the guest's differing copy with the host's.
    Replace,
    /// Keep the guest's copy out of the session.
    KeepOut,
    /// Share a guest-only file.
    Share,
    /// Keep a guest-only file private.
    Private,
}

/// The wildcard path of the all-files confirmation.
pub(crate) const ALL_FILES: &str = "*";

/// Turn choices into decisions, enforcing the owner decisions: a differing
/// file is never left unanswered, and replacing more than one file needs the
/// all-files confirmation.
pub(crate) fn decide(
    plan: &JoinPlan,
    choices: &[Choice],
) -> Result<BTreeMap<String, Decision>, LiveHostError> {
    let mut explicit: BTreeMap<&str, Action> = BTreeMap::new();
    let mut wildcard: Option<Action> = None;
    for choice in choices {
        if choice.path == ALL_FILES {
            wildcard = Some(choice.action);
        } else {
            explicit.insert(choice.path.as_str(), choice.action);
        }
    }
    let mut out = BTreeMap::new();
    let mut unresolved = 0usize;
    let mut replaced = 0usize;
    for file in &plan.files {
        let decision = match file.class {
            PlanClass::Identical | PlanClass::Missing => Decision::Take,
            PlanClass::Differing => match explicit.get(file.path.as_str()).copied().or(wildcard) {
                None => {
                    unresolved += 1;
                    continue;
                }
                Some(Action::TakeHost) => {
                    replaced += 1;
                    Decision::Replace
                }
                Some(Action::KeepMine | Action::KeepPrivate) => Decision::KeepOut,
                Some(Action::Share) => {
                    return Err(LiveHostError::Invalid(format!(
                        "{} exists on both sides: choose take-host or keep-mine",
                        file.path
                    )));
                }
            },
            PlanClass::GuestOnly => match explicit.get(file.path.as_str()) {
                Some(Action::Share) => Decision::Share,
                _ => Decision::Private,
            },
        };
        out.insert(file.path.clone(), decision);
    }
    if unresolved > 0 {
        return Err(LiveHostError::Unresolved(unresolved));
    }
    if replaced > 1 && wildcard != Some(Action::TakeHost) {
        return Err(LiveHostError::NeedsConfirmation(replaced));
    }
    Ok(out)
}

impl Driver {
    /// The host's manifest arrived: classify the folder and ask the guest.
    pub(super) fn on_manifest(&mut self, entries: Vec<ManifestEntry>) {
        if !matches!(self.phase, Phase::AwaitingManifest) {
            return;
        }
        match classify_local(self.disk.policy(), &entries) {
            Ok(plan) => {
                for rejected in &plan.rejected {
                    self.skip(&rejected.path, &rejected.reason.to_string());
                }
                self.emit("live.plan", plan.to_event());
                self.status.lock().phase = "plan";
                self.phase = Phase::AwaitingResolve(Box::new(PendingPlan { entries, plan }));
            }
            Err(e) => self.fail(&format!("Your folder could not be compared: {e}")),
        }
    }

    /// Phase 2 of a join: apply the choices.
    pub(super) fn resolve(&mut self, choices: &[Choice]) -> Result<ResolveOutcome, LiveHostError> {
        let Phase::AwaitingResolve(pending) = &self.phase else {
            return Err(LiveHostError::NoPlan);
        };
        let decisions = decide(&pending.plan, choices)?;
        let entries = pending.entries.clone();
        let guest_only: Vec<String> = pending
            .plan
            .of(PlanClass::GuestOnly)
            .map(|f| f.path.clone())
            .collect();
        self.phase = Phase::Live;
        self.status.lock().phase = "live";

        let view = self.view();
        let mut out: Vec<Output> = Vec::new();
        let mut applied = 0usize;
        let mut trashed: Vec<String> = Vec::new();
        let mut known = std::collections::BTreeSet::new();
        for listed in &entries {
            known.insert(listed.id);
            let Some(entry) = view.get(listed.id) else {
                continue; // deleted while the guest was deciding
            };
            if listed.kind == NodeKind::Dir {
                if self.disk.policy().is_shareable(&entry.path) {
                    let _ = self.disk.create_dir(&entry.path);
                }
                continue;
            }
            let Some(decision) = decisions.get(&listed.path).copied() else {
                continue; // refused by the policy: never written
            };
            match decision {
                Decision::Take => {
                    applied += 1;
                    out.extend(self.take_entry(entry, false));
                }
                Decision::Replace => {
                    applied += 1;
                    trashed.push(entry.path.clone());
                    out.extend(self.take_entry(entry, true));
                }
                Decision::KeepOut => self.exclude_local(entry.id, &entry.path),
                Decision::Share | Decision::Private => {}
            }
        }
        for path in guest_only {
            match decisions.get(&path) {
                Some(Decision::Share) => match self.create_from_disk(&path) {
                    Ok(more) => {
                        applied += 1;
                        out.extend(more);
                    }
                    Err(e) => self.skip(&path, &e.to_string()),
                },
                _ => {
                    // Private: never offered, and a later edit must not
                    // quietly share it either.
                    self.excluded_paths.insert(path.clone());
                    self.fs.exclude(&path);
                }
            }
        }
        // Whatever appeared after the manifest was taken is handled like a
        // remote creation (the host's version wins, the old copy is saved).
        for entry in view.entries() {
            if !known.contains(&entry.id) {
                out.extend(self.on_created(entry.clone()));
            }
        }
        out.extend(self.session.flush_project());
        self.perform(out);
        self.announce_focus();
        self.peers_dirty = true;
        Ok(ResolveOutcome { applied, trashed })
    }

    /// Keep `path` (a host node the guest kept their own copy of) out of the
    /// session on this side.
    pub(super) fn exclude_local(&mut self, id: manang_live::project::FileId, path: &str) {
        self.file(id).excluded = true;
        self.excluded_paths.insert(path.to_owned());
        self.fs.exclude(path);
        self.arriving.remove(&id);
        self.pending_text.remove(&id);
        self.dirty_docs.remove(&id);
        self.drop_doc(id);
    }

    /// Join a file of the room: seed from the identical local copy, or fetch
    /// the host's. `replace` marks a differing local copy to be saved to the
    /// trash when the host's content lands.
    pub(super) fn take_entry(&mut self, entry: &Entry, replace: bool) -> Vec<Output> {
        let path = entry.path.as_str();
        if let Err(reason) = self.disk.policy().check_rel(path) {
            self.skip(path, &reason.to_string());
            return Vec::new();
        }
        if let Err(reason) = self
            .disk
            .policy()
            .check_size(entry.meta.kind, entry.meta.size)
        {
            self.skip(path, &reason.to_string());
            return Vec::new();
        }
        match entry.meta.kind {
            NodeKind::Dir => {
                let _ = self.disk.create_dir(path);
                Vec::new()
            }
            NodeKind::Text => self.fetch_text(entry, replace),
            NodeKind::Blob => self.fetch_blob(entry, replace),
        }
    }

    fn fetch_text(&mut self, entry: &Entry, replace: bool) -> Vec<Output> {
        let id = entry.id;
        let now = self.now;
        if !replace && let Some(text) = self.identical_text(entry) {
            // Same text and same seed: an identical document, nothing to move.
            return match LiveDoc::seeded(self.peer, &text) {
                Ok(doc) => {
                    let out = self.session.provide_doc(now, id, doc, false);
                    let state = self.file(id);
                    state.local_origin = true;
                    state.synced_sha = None;
                    self.touch(id);
                    out
                }
                Err(e) => {
                    self.skip(&entry.path, &e.to_string());
                    Vec::new()
                }
            };
        }
        self.residency.pin(id);
        self.touch(id);
        {
            let state = self.file(id);
            state.local_origin = false;
            state.replace_on_arrival = replace;
        }
        self.arriving.insert(id);
        self.session.want_doc(now, id, false)
    }

    /// The local text of `entry`, when it equals the room's seed exactly.
    fn identical_text(&self, entry: &Entry) -> Option<String> {
        let bytes = crate::disk::read_capped(
            &self.disk.policy().root().path().join(&entry.path),
            self.disk.policy().max_text(),
        )
        .ok()?;
        if sha256(&bytes) != entry.meta.sha256 {
            return None;
        }
        let text = String::from_utf8(bytes).ok()?;
        (seed_fingerprint(&text) == entry.meta.seed_fp).then_some(text)
    }

    fn fetch_blob(&mut self, entry: &Entry, replace: bool) -> Vec<Output> {
        let id = entry.id;
        let hash = entry.meta.sha256;
        if !replace
            && matches!(self.disk.observe(&entry.path), Ok(Observed::Content(h)) if h == hash)
        {
            return Vec::new(); // already here
        }
        if entry.meta.size == 0 {
            // Nothing to fetch: an empty file.
            let result = if replace {
                self.disk.take_host(&entry.path, &[]).map(|_| ())
            } else {
                self.disk.write_blob(&entry.path, &[]).map(|_| ())
            };
            if let Err(e) = result {
                self.skip(&entry.path, &e.to_string());
            }
            return Vec::new();
        }
        self.file(id).replace_on_arrival = replace;
        self.arriving_blobs.entry(hash).or_default().insert(id);
        match self.session.want_blob(self.now, hash, entry.meta.size) {
            Ok(out) => out,
            Err(e) => {
                self.skip(&entry.path, &e.to_string());
                Vec::new()
            }
        }
    }

    /// A node appeared in the room (after the join, or while the guest was
    /// deciding): fetch it. A different local file at the same path is
    /// replaced by the host's, saved to the trash first.
    pub(super) fn on_created(&mut self, entry: Entry) -> Vec<Output> {
        if self.file(entry.id).excluded || self.excluded_paths.contains(&entry.path) {
            return Vec::new();
        }
        if entry.meta.kind == NodeKind::Dir {
            if self.disk.policy().is_shareable(&entry.path)
                && let Err(e) = self.disk.create_dir(&entry.path)
            {
                self.skip(&entry.path, &e.to_string());
            }
            return Vec::new();
        }
        let replace = matches!(
            self.disk.observe(&entry.path),
            Ok(Observed::Content(h)) if h != entry.meta.sha256
        );
        self.take_entry(&entry, replace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::PlanFile;

    fn plan(files: &[(&str, PlanClass)]) -> JoinPlan {
        JoinPlan {
            files: files
                .iter()
                .map(|(p, c)| PlanFile {
                    path: (*p).to_owned(),
                    class: *c,
                })
                .collect(),
            rejected: Vec::new(),
        }
    }

    fn pick(path: &str, action: Action) -> Choice {
        Choice {
            path: path.to_owned(),
            action,
        }
    }

    #[test]
    fn a_differing_file_with_no_answer_is_refused() {
        let p = plan(&[("a", PlanClass::Differing), ("b", PlanClass::Identical)]);
        assert!(matches!(decide(&p, &[]), Err(LiveHostError::Unresolved(1))));
        let ok = decide(&p, &[pick("a", Action::TakeHost)]).expect("one replacement");
        assert_eq!(ok["a"], Decision::Replace);
        assert_eq!(ok["b"], Decision::Take);
    }

    #[test]
    fn more_than_one_replacement_needs_the_all_files_confirmation() {
        let p = plan(&[("a", PlanClass::Differing), ("b", PlanClass::Differing)]);
        let both = [pick("a", Action::TakeHost), pick("b", Action::TakeHost)];
        assert!(matches!(
            decide(&p, &both),
            Err(LiveHostError::NeedsConfirmation(2))
        ));
        let mut confirmed = both.to_vec();
        confirmed.push(pick(ALL_FILES, Action::TakeHost));
        assert!(decide(&p, &confirmed).is_ok());
        // The wildcard with another action is no confirmation.
        let mut wrong = both.to_vec();
        wrong.push(pick(ALL_FILES, Action::KeepMine));
        assert!(matches!(
            decide(&p, &wrong),
            Err(LiveHostError::NeedsConfirmation(2))
        ));
        // Keeping both replaces nothing: no confirmation needed.
        let keep = [pick("a", Action::KeepMine), pick("b", Action::KeepMine)];
        let kept = decide(&p, &keep).expect("keeping needs none");
        assert_eq!(kept["a"], Decision::KeepOut);
    }

    #[test]
    fn explicit_choices_win_over_the_wildcard_which_covers_the_rest() {
        let p = plan(&[
            ("a", PlanClass::Differing),
            ("b", PlanClass::Differing),
            ("c", PlanClass::Differing),
        ]);
        let choices = [
            pick(ALL_FILES, Action::TakeHost),
            pick("b", Action::KeepMine),
        ];
        let d = decide(&p, &choices).expect("covered");
        assert_eq!(d["a"], Decision::Replace);
        assert_eq!(d["b"], Decision::KeepOut);
        assert_eq!(d["c"], Decision::Replace);
    }

    #[test]
    fn guest_only_files_are_private_unless_shared() {
        let p = plan(&[("x", PlanClass::GuestOnly), ("y", PlanClass::GuestOnly)]);
        let d = decide(&p, &[pick("x", Action::Share)]).expect("ok");
        assert_eq!(d["x"], Decision::Share);
        assert_eq!(d["y"], Decision::Private);
        // The wildcard never shares a private file.
        let d = decide(&p, &[pick(ALL_FILES, Action::TakeHost)]).expect("ok");
        assert_eq!(d["x"], Decision::Private);
    }

    #[test]
    fn sharing_a_differing_file_over_the_hosts_is_invalid() {
        let p = plan(&[("a", PlanClass::Differing)]);
        assert!(matches!(
            decide(&p, &[pick("a", Action::Share)]),
            Err(LiveHostError::Invalid(_))
        ));
    }
}
