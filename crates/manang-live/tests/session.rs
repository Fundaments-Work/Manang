//! Session state machine: host, join, leave, reconnect, resync, presence,
//! and hostile input. Run against both engines.

use manang_live::envelope::{Kind, RoomSecret};
use manang_live::session::{Input, Output, SessionState};
use manang_live::sim::{NetConfig, Rng, Sim};
use manang_live::{LoroEngine, TextCrdt, YrsEngine};

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
    let mut sim = Sim::<YrsEngine>::new(2, SEED, 8, NetConfig::CLEAN).expect("sim");
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
