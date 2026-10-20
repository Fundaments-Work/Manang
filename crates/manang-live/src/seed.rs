//! Seed agreement: what happens when two people start with different files.
//!
//! The host's file seeds the room; the invite carries the seed's fingerprint
//! ([`crate::envelope::Invite::seed`]). A joiner compares it with the
//! fingerprint of their own copy of the file *before* connecting:
//!
//! | Joiner's file                         | Plan                         |
//! | ------------------------------------- | ---------------------------- |
//! | equals the host's seed                | [`JoinPlan::SeedLocally`]    |
//! | equals the git base (no local edits)  | [`JoinPlan::AdoptHost`]      |
//! | anything else                         | [`JoinPlan::ChooseResolution`] |
//!
//! `SeedLocally` builds the document with [`crate::TextCrdt::seeded`], which
//! produces operations identical to the host's, so nothing is transferred and
//! nothing is duplicated. `AdoptHost` joins with an empty document and lets
//! the host's state replace the buffer: nothing of the joiner's is lost
//! because it was the base. `ChooseResolution` is not decided here: the live
//! plugin asks the person (the join is refused with `differs` until they
//! confirm), and on "use the host's version" it copies the joiner's file to
//! `live-trash` in the app config directory, joins with an empty document and
//! replaces the file only when a host has actually answered (until then the
//! file is untouched). The planned three-way merge (`manang-merge`'s `merge3`)
//! is not built yet.
//!
//! Nothing is ever merged silently, and the host never has to wait.

use crate::envelope::seed_fingerprint;

/// How a joiner should enter the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinPlan {
    /// The joiner's file is byte-equal to the host's seed: seed identically.
    SeedLocally,
    /// The joiner has no edits of their own: take the host's content.
    AdoptHost,
    /// The files really differ: show a diff and let the joiner choose.
    ChooseResolution,
}

/// Decide the join plan.
///
/// `local_text` is the joiner's current file; `git_base` is the same file at
/// the commit the joiner is on (`None` outside a repository or for an
/// untracked file).
#[must_use]
pub fn plan_join(host_seed: [u8; 16], local_text: &str, git_base: Option<&str>) -> JoinPlan {
    if seed_fingerprint(local_text) == host_seed {
        JoinPlan::SeedLocally
    } else if git_base.is_some_and(|base| base == local_text) {
        JoinPlan::AdoptHost
    } else {
        JoinPlan::ChooseResolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::TextCrdt;
    use crate::{LoroEngine, YrsEngine};

    #[test]
    fn plans() {
        let host = seed_fingerprint("A\n");
        assert_eq!(plan_join(host, "A\n", None), JoinPlan::SeedLocally);
        assert_eq!(plan_join(host, "A\n", Some("other")), JoinPlan::SeedLocally);
        assert_eq!(plan_join(host, "B\n", Some("B\n")), JoinPlan::AdoptHost);
        assert_eq!(
            plan_join(host, "B\n", Some("C\n")),
            JoinPlan::ChooseResolution
        );
        assert_eq!(plan_join(host, "B\n", None), JoinPlan::ChooseResolution);
    }

    fn identical_seed_merges_without_duplication<E: TextCrdt>() {
        let text = "Same file é😀\non both machines\n";
        let a = E::seeded(10, text).expect("a");
        let mut b = E::seeded(11, text).expect("b");
        let diff = a.export_since(&b.version()).expect("diff");
        let applied = b.apply_update(&diff).expect("apply");
        assert!(applied.changes.is_empty(), "seed duplicated: {applied:?}");
        assert_eq!(a.text(), text);
        assert_eq!(b.text(), text);
    }

    #[test]
    fn identical_seeds_do_not_duplicate() {
        identical_seed_merges_without_duplication::<LoroEngine>();
        identical_seed_merges_without_duplication::<YrsEngine>();
    }

    fn different_seeds_diverge_silently_at_engine_level<E: TextCrdt>() {
        // Seeding is deterministic (same authoring peer, same operation ids),
        // so two different seeds collide on operation ids: the engines
        // consider them the same operations and never reconcile. This is why
        // a joiner seeds only on a fingerprint match, and why sessions also
        // exchange text digests (`Output::Diverged`) as a safety net.
        let mut a = E::seeded(10, "host text\n").expect("a");
        let b = E::seeded(11, "joiner text\n").expect("b");
        let diff = b.export_since(&a.version()).expect("diff");
        let _ = a.apply_update(&diff);
        let (ta, tb) = (a.text(), b.text());
        assert!(
            ta != tb || a.version() != b.version(),
            "{}: different seeds unexpectedly reconciled: {ta:?}",
            E::name()
        );
    }

    #[test]
    fn different_seeds_must_never_meet() {
        different_seeds_diverge_silently_at_engine_level::<LoroEngine>();
        different_seeds_diverge_silently_at_engine_level::<YrsEngine>();
    }
}
