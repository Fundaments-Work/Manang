//! Git interplay: the rules that keep a live session and a git repository
//! from fighting, expressed as data and pure functions.
//!
//! The decisions (ADR 0013, "Git interplay"):
//!
//! 1. **The file on disk is the source of truth.** A live session *seeds* from
//!    the file's current text and autosave writes the merged text back to the
//!    same file, as it does for any edit. Git only ever sees ordinary files.
//! 2. **CRDT state is a disposable sidecar**, stored in the app data directory
//!    (never in the working tree, never committed): see [`sidecar_name`]. It is
//!    a cache for fast rejoin; deleting it loses nothing.
//! 3. **A commit made during a live session** commits the file on disk (flush
//!    autosave first) and lists the people who edited since the previous
//!    commit as `Co-authored-by:` trailers: see [`ContributionLog`] and
//!    [`coauthor_trailers`].
//! 4. **"Get updates" is refused for files in a live session**: a merge
//!    rewriting a file underneath a CRDT would diverge every peer. The shell
//!    applies the rule (`versionActions.ts`); [`live_paths_touched_by`] is the
//!    pure form of it.
//!
//! Nothing here touches git or the filesystem; `manang-git` and the live
//! plugin apply the results.

use std::collections::{BTreeMap, BTreeSet};

/// A person who contributed text to the live document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contributor {
    /// Display name (a claim; see the identity notes in the collaboration plan).
    pub name: String,
    /// Email from the person's git identity, if they chose to share it.
    pub email: Option<String>,
}

/// What a commit made during a live session should carry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrailerPlan {
    /// Ready-to-append `Co-authored-by: Name <email>` lines.
    pub trailers: Vec<String>,
    /// Contributors with no shared email: git trailers need an address, so the
    /// UI asks the committer whether to name them in the message body.
    pub without_email: Vec<String>,
}

/// Remove characters that would break a trailer line or inject another.
fn clean(s: &str) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>'))
        .collect();
    filtered.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A conservative email check: one `@`, something either side, no spaces.
fn plausible_email(s: &str) -> Option<String> {
    let s = s.trim();
    let (local, domain) = s.split_once('@')?;
    let ok = !local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && s.len() <= 254
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>'));
    ok.then(|| s.to_owned())
}

/// Build the trailer lines for a commit.
///
/// The committer is excluded (git records them as the author already),
/// duplicates collapse (same email, case-insensitively), and the order is
/// stable (by name, then email) so the message is reproducible.
#[must_use]
pub fn coauthor_trailers(
    contributors: &[Contributor],
    committer_email: Option<&str>,
    committer_name: Option<&str>,
) -> TrailerPlan {
    let committer_email = committer_email.map(str::to_lowercase);
    let committer_name = committer_name.map(|n| clean(n).to_lowercase());
    let mut with_email: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut without: BTreeSet<String> = BTreeSet::new();
    for c in contributors {
        let name = clean(&c.name);
        if name.is_empty() {
            continue;
        }
        match c.email.as_deref().and_then(plausible_email) {
            Some(email) => {
                let key = email.to_lowercase();
                if committer_email.as_deref() == Some(key.as_str()) {
                    continue;
                }
                with_email.entry(key).or_insert((name, email));
            }
            None => {
                if committer_name.as_deref() == Some(name.to_lowercase().as_str()) {
                    continue;
                }
                without.insert(name);
            }
        }
    }
    let mut lines: Vec<(String, String, String)> = with_email
        .into_values()
        .map(|(name, email)| {
            (
                name.to_lowercase(),
                email.to_lowercase(),
                format!("Co-authored-by: {name} <{email}>"),
            )
        })
        .collect();
    lines.sort();
    TrailerPlan {
        trailers: lines.into_iter().map(|(_, _, line)| line).collect(),
        without_email: without.into_iter().collect(),
    }
}

/// Who edited since the last commit, with how much.
#[derive(Debug, Default)]
pub struct ContributionLog {
    entries: BTreeMap<u64, (Contributor, usize)>,
}

impl ContributionLog {
    /// Record `units` changed characters by `peer`. The latest name and email
    /// seen for the peer win.
    pub fn record(&mut self, peer: u64, name: &str, email: Option<&str>, units: usize) {
        if units == 0 {
            return;
        }
        let entry = self.entries.entry(peer).or_insert_with(|| {
            (
                Contributor {
                    name: name.to_owned(),
                    email: None,
                },
                0,
            )
        });
        entry.0.name = name.to_owned();
        if let Some(e) = email {
            entry.0.email = Some(e.to_owned());
        }
        entry.1 += units;
    }

    /// Contributors since the last call, and reset (call when a commit is made).
    pub fn take(&mut self) -> Vec<Contributor> {
        std::mem::take(&mut self.entries)
            .into_values()
            .map(|(c, _)| c)
            .collect()
    }

    /// Whether anyone has contributed since the last [`ContributionLog::take`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// File name of the disposable CRDT sidecar for a room, to be placed under
/// the app data directory (`<data>/live-snapshots/<name>`), never in the
/// working tree. (The plugin writes its own `.snap` container, which adds a
/// text digest to the engine snapshot; this name is the portable form.)
#[must_use]
pub fn sidecar_name(room_id_hex: &str) -> String {
    format!("{room_id_hex}.crdt")
}

/// Paths in `incoming` (what a merge or pull would rewrite) that are open in a
/// live session. A non-empty result means "Get updates" must be refused with
/// an explanation: leave the live session first. Paths compare exactly; the
/// caller normalises them to project-relative `/` form.
#[must_use]
pub fn live_paths_touched_by(live: &BTreeSet<String>, incoming: &[String]) -> Vec<String> {
    incoming
        .iter()
        .filter(|p| live.contains(p.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, email: Option<&str>) -> Contributor {
        Contributor {
            name: name.to_owned(),
            email: email.map(str::to_owned),
        }
    }

    #[test]
    fn trailers_exclude_the_committer_dedupe_and_sort() {
        let plan = coauthor_trailers(
            &[
                c("Grace Hopper", Some("grace@navy.mil")),
                c("Ada Lovelace", Some("Ada@Example.org")),
                c("ada lovelace (laptop)", Some("ada@example.org")), // same address
                c("Me", Some("me@example.org")),                     // the committer
                c("Linus", None),
            ],
            Some("ME@example.org"),
            Some("Me"),
        );
        assert_eq!(
            plan.trailers,
            vec![
                "Co-authored-by: Ada Lovelace <Ada@Example.org>".to_owned(),
                "Co-authored-by: Grace Hopper <grace@navy.mil>".to_owned(),
            ]
        );
        assert_eq!(plan.without_email, vec!["Linus".to_owned()]);
    }

    #[test]
    fn hostile_names_and_emails_cannot_inject_lines() {
        let plan = coauthor_trailers(
            &[
                c("Evil\nSigned-off-by: root <r@x.org>", Some("e@x.org")),
                c("Bad <x@y.z>", Some("a b@x.org")),
                c("<>", Some("good@x.org")),
                c("", Some("nobody@x.org")),
            ],
            None,
            None,
        );
        for line in &plan.trailers {
            assert!(!line.contains('\n'));
            assert_eq!(line.matches('<').count(), 1);
            assert_eq!(line.matches('>').count(), 1);
            assert!(line.starts_with("Co-authored-by: "));
        }
        // "a b@x.org" is not an address: that contributor has no trailer.
        assert!(plan.without_email.contains(&"Bad x@y.z".to_owned()));
        assert_eq!(plan.trailers.len(), 1);
    }

    #[test]
    fn log_accumulates_and_resets_on_take() {
        let mut log = ContributionLog::default();
        assert!(log.is_empty());
        log.record(1, "Ada", None, 5);
        log.record(1, "Ada L.", Some("ada@x.org"), 3);
        log.record(2, "Grace", None, 0); // nothing changed: not a contributor
        log.record(3, "Linus", None, 1);
        let got = log.take();
        assert_eq!(got, vec![c("Ada L.", Some("ada@x.org")), c("Linus", None)]);
        assert!(log.is_empty());
        assert!(log.take().is_empty());
    }

    #[test]
    fn sidecar_lives_outside_the_tree_by_name_only() {
        assert_eq!(sidecar_name("00ff"), "00ff.crdt");
    }

    #[test]
    fn get_updates_is_blocked_only_for_live_files() {
        let live: BTreeSet<String> = ["paper/main.typ".to_owned()].into();
        let incoming = vec!["README.md".to_owned(), "paper/main.typ".to_owned()];
        assert_eq!(
            live_paths_touched_by(&live, &incoming),
            vec!["paper/main.typ".to_owned()]
        );
        assert!(live_paths_touched_by(&live, &["README.md".to_owned()]).is_empty());
        assert!(live_paths_touched_by(&BTreeSet::new(), &incoming).is_empty());
    }
}
