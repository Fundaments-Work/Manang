//! Session state machine: host, join, leave, reconnect, resync, presence,
//! and hostile input. Run against both engines.

#[cfg(feature = "yrs-engine")]
use manang_live::YrsEngine;
use manang_live::envelope::{Kind, RoomSecret};
use manang_live::session::{Input, Output, SessionState};
use manang_live::sim::{NetConfig, Rng, Sim};
use manang_live::{LoroEngine, TextCrdt};

const SEED: &str = "Dear reviewers,\nthe méthode 😀 works.\n";

fn both(f: impl Fn(&str)) {
    f("loro");
    f("yrs");
}

fn join_and_seed<E: TextCrdt>() {
    let mut sim = Sim::<E>::new(3, SEED, 1, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    for c in &sim.clients {
        assert_eq!(c.rope.to_string(), SEED);
        assert_eq!(c.session.state(), SessionState::Live);
    }
    assert!(sim.converged());
}

#[test]
fn joiners_receive_the_hosts_seed() {
    join_and_seed::<LoroEngine>();
    #[cfg(feature = "yrs-engine")]
    join_and_seed::<YrsEngine>();
}

fn reconnect_flow<E: TextCrdt>() {
    let mut sim = Sim::<E>::new(2, SEED, 2, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    sim.disconnect(1);
    assert!(matches!(
        sim.clients[1].session.state(),
        SessionState::Reconnecting { attempt: 1, .. }
    ));
    // Both type while apart.
    sim.edit(0, 0, 0, "A:").expect("edit");
    sim.edit(1, SEED.chars().count(), SEED.chars().count(), ":B")
        .expect("edit");
    sim.advance(2_000);
    assert_ne!(
        sim.clients[0].rope.to_string(),
        sim.clients[1].rope.to_string()
    );
    // Backoff timer fires, client reconnects, both converge with both edits.
    sim.advance(1_000);
    assert_eq!(sim.clients[1].session.state(), SessionState::Connecting);
    sim.connect(1);
    assert!(sim.heal(30_000));
    let text = sim.clients[0].rope.to_string();
    assert!(text.starts_with("A:Dear"));
    assert!(text.ends_with(":B"));
    assert_eq!(sim.clients[1].session.state(), SessionState::Live);
}

#[test]
fn offline_edits_merge_on_reconnect() {
    reconnect_flow::<LoroEngine>();
    #[cfg(feature = "yrs-engine")]
    reconnect_flow::<YrsEngine>();
}

#[test]
fn backoff_doubles_and_resets() {
    let mut sim = Sim::<LoroEngine>::new(1, SEED, 3, NetConfig::CLEAN).expect("sim");
    let mut delays = Vec::new();
    for _ in 0..4 {
        let now = sim.now();
        // A failed attempt: Down without ever getting Up.
        let c = &mut sim.clients[0];
        let _ = c.session.handle(now, Input::Down, &mut c.rope);
        if let SessionState::Reconnecting { retry_at_ms, .. } = c.session.state() {
            delays.push(retry_at_ms - now);
        }
    }
    assert_eq!(delays, vec![500, 1000, 2000, 4000]);
}

#[test]
fn leaving_announces_and_closes() {
    let mut sim = Sim::<LoroEngine>::new(2, SEED, 4, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    assert_eq!(sim.clients[1].session.peers().len(), 1);
    let now = sim.now();
    let out = {
        let c = &mut sim.clients[0];
        c.session.handle(now, Input::Leave, &mut c.rope)
    };
    assert!(out.contains(&Output::Close));
    assert_eq!(sim.clients[0].session.state(), SessionState::Closed);
    // Deliver the goodbye to peer 1 by hand.
    for o in out {
        if let Output::Send(f) = o {
            let c = &mut sim.clients[1];
            c.session.handle(now, Input::Frame(&f), &mut c.rope);
        }
    }
    assert!(sim.clients[1].session.peers().is_empty());
    // A closed session ignores everything.
    let c = &mut sim.clients[0];
    assert!(c.session.handle(now, Input::Up, &mut c.rope).is_empty());
}

fn presence_flow<E: TextCrdt>() {
    let mut sim = Sim::<E>::new(2, SEED, 5, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    // Peer 0 puts a caret before "méthode" (UTF-16 offset 24).
    let caret = SEED
        .find("méthode")
        .map(|b| SEED[..b].encode_utf16().count())
        .expect("find");
    let out = {
        let c = &mut sim.clients[0];
        c.session.set_selection(&c.rope, Some((caret, caret + 7)))
    };
    assert!(out.iter().any(|o| matches!(o, Output::Send(_))));
    let frames: Vec<Vec<u8>> = out
        .into_iter()
        .filter_map(|o| match o {
            Output::Send(f) => Some(f),
            _ => None,
        })
        .collect();
    let now = sim.now();
    for f in &frames {
        let c = &mut sim.clients[1];
        let o = c.session.handle(now, Input::Frame(f), &mut c.rope);
        assert!(o.contains(&Output::PresenceChanged));
    }
    let sels = {
        let c = &sim.clients[1];
        c.session.remote_selections(&c.rope)
    };
    assert_eq!(sels.len(), 1);
    assert_eq!((sels[0].anchor, sels[0].head), (caret, caret + 7));
    assert_eq!(sels[0].name, "peer0");

    // Peer 1 types before the caret: the stable position moves with the text.
    sim.edit(1, 0, 0, "😀😀").expect("edit"); // 4 UTF-16 units
    sim.advance(1_000);
    let sels = {
        let c = &sim.clients[1];
        c.session.remote_selections(&c.rope)
    };
    assert_eq!((sels[0].anchor, sels[0].head), (caret + 4, caret + 11));
    let sels0 = {
        let c = &sim.clients[0];
        c.session.remote_selections(&c.rope)
    };
    assert!(sels0.is_empty(), "own caret is never listed as remote");

    // Presence is ephemeral: not in the persisted document state.
    let snap = sim.clients[0]
        .session
        .doc()
        .engine()
        .export_snapshot()
        .expect("snapshot");
    assert!(!snap.windows(5).any(|w| w == b"peer0"));

    // Silence expires presence.
    sim.disconnect(0);
    sim.advance(30_000);
    assert!(sim.clients[1].session.peers().is_empty());
}

#[test]
fn presence_is_resolved_stable_and_ephemeral() {
    presence_flow::<LoroEngine>();
    #[cfg(feature = "yrs-engine")]
    presence_flow::<YrsEngine>();
}

#[test]
fn hostile_frames_are_rejected_without_effect() {
    both(|_| {});
    let mut sim = Sim::<LoroEngine>::new(2, SEED, 6, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    let before = sim.clients[1].rope.to_string();
    let good = sim.wire.last().cloned().expect("a frame");
    let now = sim.now();

    // Tampered: every bit position of the first 40 bytes.
    for i in 0..40.min(good.len()) {
        let mut bad = good.clone();
        bad[i] ^= 0x01;
        let c = &mut sim.clients[1];
        let out = c.session.handle(now, Input::Frame(&bad), &mut c.rope);
        assert!(
            out.iter().all(|o| matches!(o, Output::Rejected(_))),
            "byte {i}: {out:?}"
        );
    }
    // Garbage, empty, truncated.
    for junk in [
        vec![],
        vec![1],
        vec![1, 1, 0],
        vec![0xff; 100],
        good[..good.len() - 1].to_vec(),
    ] {
        let c = &mut sim.clients[1];
        let out = c.session.handle(now, Input::Frame(&junk), &mut c.rope);
        assert!(out.iter().all(|o| matches!(o, Output::Rejected(_))));
    }
    // A well-formed frame from a different room.
    let mut rng = Rng(999);
    let other = RoomSecret::generate(&mut rng)
        .and_then(|s| s.derive())
        .expect("keys");
    let foreign = other.seal(Kind::Data, b"whatever", &mut rng).expect("seal");
    let c = &mut sim.clients[1];
    let out = c.session.handle(now, Input::Frame(&foreign), &mut c.rope);
    assert!(out.iter().all(|o| matches!(o, Output::Rejected(_))));
    assert_eq!(sim.clients[1].rope.to_string(), before);
}

#[test]
fn relay_sees_only_ciphertext() {
    let secret_words = ["méthode", "reviewers", "Dear", "works"];
    let mut sim = Sim::<LoroEngine>::new(3, SEED, 7, NetConfig::CLEAN).expect("sim");
    sim.advance(3_000);
    sim.edit(1, 5, 5, "confidential-phrase").expect("edit");
    sim.edit(2, 0, 0, "😀 emoji-phrase").expect("edit");
    sim.advance(3_000);
    assert!(sim.wire.len() > 10);
    for frame in &sim.wire {
        for w in
            secret_words
                .iter()
                .chain(&["confidential-phrase", "emoji-phrase", "peer0", "peer1"])
        {
            assert!(
                !frame.windows(w.len()).any(|x| x == w.as_bytes()),
                "frame leaks {w}"
            );
        }
    }
}

#[test]
fn pending_updates_trigger_a_resync_request() {
    // Drop everything for a while: later updates depend on lost ones.
    let mut sim = Sim::<LoroEngine>::new(2, SEED, 8, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    sim.net = NetConfig {
        drop_pct: 100,
        dup_pct: 0,
        max_delay_ms: 0,
    };
    sim.edit(0, 0, 0, "one ").expect("edit");
    sim.net = NetConfig::CLEAN;
    sim.edit(0, 0, 0, "two ").expect("edit");
    sim.advance(3_000);
    assert!(sim.converged(), "pending update did not trigger a resync");
}

#[test]
fn commits_list_the_people_who_edited() {
    use manang_live::vcs::coauthor_trailers;
    let mut sim = Sim::<LoroEngine>::new(3, SEED, 9, NetConfig::CLEAN).expect("sim");
    sim.advance(5_000);
    sim.edit(1, 0, 0, "from one ").expect("edit");
    sim.edit(2, 0, 0, "from two ").expect("edit");
    sim.advance(1_000);
    let who = sim.clients[0].session.take_contributors();
    let names: Vec<&str> = who.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["peer1", "peer2"]);
    // Nobody shared an email in this room, so no trailer, but they are listed.
    let plan = coauthor_trailers(&who, None, Some("peer0"));
    assert!(plan.trailers.is_empty());
    assert_eq!(plan.without_email, vec!["peer1", "peer2"]);
    // The log resets after a commit.
    assert!(sim.clients[0].session.take_contributors().is_empty());
    sim.edit(2, 0, 0, "again ").expect("edit");
    sim.advance(1_000);
    let who = sim.clients[0].session.take_contributors();
    assert_eq!(who.len(), 1);
}

fn divergence_is_detected<E: TextCrdt>() {
    use manang_live::bridge::LiveDoc;
    use manang_live::session::{Session, SessionConfig};
    use ropey::Rope;

    // Two peers that (wrongly) seeded different files: the deterministic seed
    // collides on operation ids, so the engines cannot reconcile them. The
    // digest exchange must notice.
    let mut rng = Rng(5);
    let secret = RoomSecret::generate(&mut rng).expect("secret");
    let make = |peer: u64, text: &str, seed: u64| {
        Session::new(
            LiveDoc::<E>::seeded(peer, text).expect("doc"),
            secret.derive().expect("keys"),
            Rng(seed),
            SessionConfig::default(),
        )
    };
    let mut a = make(10, "host text\n", 1);
    let mut b = make(11, "joiner text\n", 2);
    let mut ra = Rope::from_str("host text\n");
    let mut rb = Rope::from_str("joiner text\n");
    let mut to_b: Vec<Vec<u8>> = Vec::new();
    let mut to_a: Vec<Vec<u8>> = Vec::new();
    let sends = |out: Vec<Output>, sink: &mut Vec<Vec<u8>>, seen: &mut bool| {
        for o in out {
            match o {
                Output::Send(f) => sink.push(f),
                Output::Diverged { .. } => *seen = true,
                _ => {}
            }
        }
    };
    let mut diverged = false;
    sends(a.handle(0, Input::Up, &mut ra), &mut to_b, &mut diverged);
    sends(b.handle(0, Input::Up, &mut rb), &mut to_a, &mut diverged);
    for round in 0..6 {
        for f in std::mem::take(&mut to_b) {
            sends(
                b.handle(round, Input::Frame(&f), &mut rb),
                &mut to_a,
                &mut diverged,
            );
        }
        for f in std::mem::take(&mut to_a) {
            sends(
                a.handle(round, Input::Frame(&f), &mut ra),
                &mut to_b,
                &mut diverged,
            );
        }
    }
    assert!(
        diverged,
        "{}: texts {:?} vs {:?} but no divergence reported",
        E::name(),
        ra.to_string(),
        rb.to_string()
    );
}

#[test]
fn divergence_is_reported_not_silent() {
    divergence_is_detected::<LoroEngine>();
    #[cfg(feature = "yrs-engine")]
    divergence_is_detected::<YrsEngine>();
}

// ---------------------------------------------------------------------------
// Waiting for a host (S2) and resync (S5)
// ---------------------------------------------------------------------------

mod waiting {
    use manang_live::bridge::LiveDoc;
    use manang_live::envelope::RoomSecret;
    use manang_live::session::{Input, Output, Session, SessionConfig, SessionState};
    use manang_live::sim::Rng;
    use manang_live::{LoroEngine, TextCrdt};
    use ropey::Rope;

    const TEXT: &str = "host text\n";

    type S = Session<LoroEngine, Rng>;

    fn pair(
        host_text: &str,
        joiner_doc: LiveDoc<LoroEngine>,
        joiner_rope: &str,
    ) -> (S, S, Rope, Rope) {
        let secret = RoomSecret::generate(&mut Rng(9)).expect("secret");
        let host = Session::new(
            LiveDoc::<LoroEngine>::seeded(10, host_text).expect("doc"),
            secret.derive().expect("keys"),
            Rng(1),
            SessionConfig::default(),
        );
        let joiner = Session::new(
            joiner_doc,
            secret.derive().expect("keys"),
            Rng(2),
            SessionConfig {
                await_host: true,
                ..SessionConfig::default()
            },
        );
        (
            host,
            joiner,
            Rope::from_str(host_text),
            Rope::from_str(joiner_rope),
        )
    }

    fn sends(out: &[Output]) -> Vec<Vec<u8>> {
        out.iter()
            .filter_map(|o| match o {
                Output::Send(f) => Some(f.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_joiner_alone_in_the_room_waits_and_reports_a_timeout_once() {
        let (_, mut joiner, _, mut rope) = pair(TEXT, LiveDoc::empty(11).expect("doc"), "");
        let _ = joiner.start();
        let up = joiner.handle(0, Input::Up, &mut rope);
        assert!(up.contains(&Output::State(SessionState::Waiting)));
        let mut timeouts = 0;
        let mut requests = 0;
        for t in (100..=60_000u64).step_by(100) {
            let out = joiner.handle(t, Input::Tick, &mut rope);
            timeouts += out
                .iter()
                .filter(|o| matches!(o, Output::HostTimeout))
                .count();
            requests += sends(&out).len();
            assert_eq!(joiner.state(), SessionState::Waiting, "never live alone");
        }
        assert_eq!(timeouts, 1);
        assert!(requests >= 10, "it keeps asking: {requests}");
        assert!(joiner.awaiting_host());
        assert_eq!(rope.to_string(), "");
    }

    #[test]
    fn an_answer_from_the_host_ends_the_wait_even_after_the_timeout() {
        let (mut host, mut joiner, mut hrope, mut jrope) =
            pair(TEXT, LiveDoc::empty(11).expect("doc"), "");
        let _ = host.start();
        let _ = joiner.start();
        let _ = joiner.handle(0, Input::Up, &mut jrope);
        for t in (100..=25_000u64).step_by(100) {
            let _ = joiner.handle(t, Input::Tick, &mut jrope);
        }
        assert_eq!(joiner.state(), SessionState::Waiting);

        // The host shows up late; the joiner's next request reaches it.
        let mut to_joiner = sends(&host.handle(25_000, Input::Up, &mut hrope));
        let mut to_host = sends(&joiner.handle(27_000, Input::Tick, &mut jrope));
        for _ in 0..4 {
            for f in std::mem::take(&mut to_joiner) {
                let out = joiner.handle(25_100, Input::Frame(&f), &mut jrope);
                to_host.extend(sends(&out));
            }
            for f in std::mem::take(&mut to_host) {
                let out = host.handle(25_100, Input::Frame(&f), &mut hrope);
                to_joiner.extend(sends(&out));
            }
        }
        assert_eq!(joiner.state(), SessionState::Live);
        assert!(!joiner.awaiting_host());
        assert_eq!(jrope.to_string(), TEXT);
    }

    #[test]
    fn a_waiting_joiner_does_not_answer_requests() {
        let (mut host, mut joiner, mut hrope, mut jrope) =
            pair(TEXT, LiveDoc::empty(11).expect("doc"), "");
        let _ = host.start();
        let _ = joiner.start();
        let from_host = sends(&host.handle(0, Input::Up, &mut hrope));
        let _ = joiner.handle(0, Input::Up, &mut jrope);
        // The host's request reaches the waiting joiner: it may announce
        // itself, but must not send sync data (an empty document would
        // mislead the host).
        let mut data_frames = 0;
        for f in from_host {
            let out = joiner.handle(10, Input::Frame(&f), &mut jrope);
            for o in &out {
                assert!(!matches!(o, Output::Remote(_)));
            }
            data_frames += sends(&out).len();
        }
        // Only presence frames are allowed: one at most.
        assert!(data_frames <= 1, "sent {data_frames} frames");
        assert_eq!(joiner.state(), SessionState::Waiting);
    }

    #[test]
    fn a_joiner_with_the_same_seed_still_waits_for_an_answer() {
        let (mut host, mut joiner, mut hrope, mut jrope) = pair(
            TEXT,
            LiveDoc::<LoroEngine>::seeded(11, TEXT).expect("doc"),
            TEXT,
        );
        let _ = host.start();
        let _ = joiner.start();
        let _ = joiner.handle(0, Input::Up, &mut jrope);
        for t in (100..=10_000u64).step_by(100) {
            let _ = joiner.handle(t, Input::Tick, &mut jrope);
        }
        assert_eq!(joiner.state(), SessionState::Waiting, "no timer promotion");
        let mut to_joiner = sends(&host.handle(10_000, Input::Up, &mut hrope));
        let mut to_host = sends(&joiner.handle(12_000, Input::Tick, &mut jrope));
        for _ in 0..4 {
            for f in std::mem::take(&mut to_joiner) {
                to_host.extend(sends(&joiner.handle(10_100, Input::Frame(&f), &mut jrope)));
            }
            for f in std::mem::take(&mut to_host) {
                to_joiner.extend(sends(&host.handle(10_100, Input::Frame(&f), &mut hrope)));
            }
        }
        assert_eq!(joiner.state(), SessionState::Live);
        assert_eq!(jrope.to_string(), TEXT);
    }

    #[test]
    fn resync_rebuilds_the_document_from_the_room() {
        let (mut host, mut joiner, mut hrope, mut jrope) = pair(
            TEXT,
            LiveDoc::<LoroEngine>::seeded(11, TEXT).expect("doc"),
            TEXT,
        );
        let _ = host.start();
        let _ = joiner.start();
        let mut to_host = sends(&joiner.handle(0, Input::Up, &mut jrope));
        let mut to_joiner = sends(&host.handle(0, Input::Up, &mut hrope));
        for _ in 0..4 {
            for f in std::mem::take(&mut to_joiner) {
                to_host.extend(sends(&joiner.handle(1, Input::Frame(&f), &mut jrope)));
            }
            for f in std::mem::take(&mut to_host) {
                to_joiner.extend(sends(&host.handle(1, Input::Frame(&f), &mut hrope)));
            }
        }
        assert_eq!(joiner.state(), SessionState::Live);

        // The host types; the joiner is told to rebuild.
        host.doc_mut()
            .local_replace_chars(&mut hrope, 0, 0, "more ")
            .expect("edit");
        let _ = host.flush_local();
        jrope = Rope::new();
        let out = joiner.resync(100, LiveDoc::empty(12).expect("doc"));
        assert!(out.contains(&Output::State(SessionState::Waiting)));
        assert!(joiner.awaiting_host());
        to_host = sends(&out);
        for _ in 0..4 {
            for f in std::mem::take(&mut to_host) {
                to_joiner.extend(sends(&host.handle(110, Input::Frame(&f), &mut hrope)));
            }
            for f in std::mem::take(&mut to_joiner) {
                to_host.extend(sends(&joiner.handle(110, Input::Frame(&f), &mut jrope)));
            }
        }
        assert_eq!(joiner.state(), SessionState::Live);
        assert_eq!(jrope.to_string(), hrope.to_string());
        assert_eq!(jrope.to_string(), format!("more {TEXT}"));
        assert_eq!(joiner.doc().engine().text(), hrope.to_string());
    }
}
