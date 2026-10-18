//! End to end, no UI: a real relay on a random localhost port, two clients in
//! an encrypted room typing concurrently, one dropping off and reconnecting,
//! and a check that the relay only ever saw ciphertext.
//!
//! The CRDT work runs on the test thread; the Tokio runtime does I/O only.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_live::bridge::LiveDoc;
use manang_live::envelope::{OsEntropy, RoomSecret};
use manang_live::session::{Input, Output, Session, SessionConfig, SessionState, pump};
use manang_live::transport::SyncTransport;
use manang_live::{LoroEngine, TextCrdt, YrsEngine};
use manang_relay::{RelayClient, RelayConfig};
use parking_lot::Mutex;
use ropey::Rope;
use tokio::runtime::Runtime;

const SEED: &str = "Abstract. We study the méthode of confidential reviewers 😀.\n";

struct Peer<E: TextCrdt> {
    session: Session<E, OsEntropy>,
    transport: RelayClient,
    rope: Rope,
}

impl<E: TextCrdt> Peer<E> {
    fn new(
        rt: &Runtime,
        addr: SocketAddr,
        room_hex: &str,
        secret: &RoomSecret,
        doc: LiveDoc<E>,
        rope: Rope,
        name: &str,
    ) -> Self {
        let keys = secret.derive().expect("keys");
        let mut session = Session::new(
            doc,
            keys,
            OsEntropy,
            SessionConfig {
                name: name.to_owned(),
                sync_timeout_ms: 500,
                ..SessionConfig::default()
            },
        );
        let mut transport =
            RelayClient::new(format!("ws://{addr}/room/{room_hex}"), rt.handle().clone());
        for o in session.start() {
            if o == Output::Connect {
                transport.connect().expect("connect");
            }
        }
        Self {
            session,
            transport,
            rope,
        }
    }

    fn pump(&mut self, now_ms: u64) {
        pump(
            &mut self.session,
            &mut self.transport,
            &mut self.rope,
            now_ms,
        )
        .expect("pump");
    }

    fn type_at(&mut self, from: usize, to: usize, text: &str) {
        self.session
            .doc_mut()
            .local_replace_chars(&mut self.rope, from, to, text)
            .expect("edit");
    }

    /// Simulate the network dropping: close the socket and tell the session.
    fn drop_link(&mut self, now_ms: u64) {
        self.transport.close();
        let out = self.session.handle(now_ms, Input::Down, &mut self.rope);
        assert!(matches!(
            self.session.state(),
            SessionState::Reconnecting { .. }
        ));
        drop(out);
    }
}

fn run_until<F: FnMut(u64) -> bool>(start: Instant, limit: Duration, mut done: F) -> bool {
    while start.elapsed() < limit {
        let now = start.elapsed().as_millis() as u64;
        if done(now) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn scenario<E: TextCrdt>() {
    let rt = Runtime::new().expect("runtime");
    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let tap_store = Arc::clone(&captured);
    let config = RelayConfig {
        tap: Some(Arc::new(move |frame: &[u8]| {
            tap_store.lock().push(frame.to_vec());
        })),
        ..RelayConfig::default()
    };
    let relay = rt
        .block_on(manang_relay::spawn(
            "127.0.0.1:0".parse().expect("addr"),
            config,
        ))
        .expect("relay");

    let secret = RoomSecret::generate(&mut OsEntropy).expect("secret");
    let room_hex = secret.derive().expect("keys").room_id_hex();

    let mut host = Peer::<E>::new(
        &rt,
        relay.addr,
        &room_hex,
        &secret,
        LiveDoc::<E>::seeded(1001, SEED).expect("host doc"),
        Rope::from_str(SEED),
        "host",
    );
    let mut guest = Peer::<E>::new(
        &rt,
        relay.addr,
        &room_hex,
        &secret,
        LiveDoc::<E>::empty(1002).expect("guest doc"),
        Rope::new(),
        "guest",
    );

    let t0 = Instant::now();
    // 1. The guest receives the host's seed through the encrypted room.
    assert!(
        run_until(t0, Duration::from_secs(10), |now| {
            host.pump(now);
            guest.pump(now);
            guest.rope == SEED
        }),
        "guest never received the seed; has {:?}",
        guest.rope.to_string()
    );

    // 2. Concurrent typing: host at the start, guest at the end.
    host.type_at(0, 0, "HOSTPHRASE ");
    let end = guest.rope.len_chars();
    guest.type_at(end, end, " GUESTPHRASE");
    let converged = |host: &Peer<E>, guest: &Peer<E>| {
        host.rope == guest.rope && host.session.doc().engine().text() == host.rope
    };
    assert!(run_until(t0, Duration::from_secs(10), |now| {
        host.pump(now);
        guest.pump(now);
        converged(&host, &guest)
    }));
    let merged = host.rope.to_string();
    assert!(merged.starts_with("HOSTPHRASE Abstract"));
    assert!(merged.ends_with(" GUESTPHRASE"));

    // 3. The guest's link drops; both keep typing while apart.
    let now = t0.elapsed().as_millis() as u64;
    guest.drop_link(now);
    host.type_at(0, 0, "OFFLINE-HOST ");
    let end = guest.rope.len_chars();
    guest.type_at(end, end, " OFFLINE-GUEST");
    run_until(t0, Duration::from_millis(300), |now| {
        host.pump(now);
        // The guest's session keeps ticking; its retry timer is 500 ms.
        false
    });
    assert_ne!(host.rope.to_string(), guest.rope.to_string());

    // 4. The guest reconnects on its own backoff timer and everything merges.
    assert!(
        run_until(t0, Duration::from_secs(15), |now| {
            host.pump(now);
            guest.pump(now);
            converged(&host, &guest) && guest.session.state() == SessionState::Live
        }),
        "no convergence after reconnect: host {:?} guest {:?}",
        host.rope.to_string(),
        guest.rope.to_string()
    );
    let final_text = host.rope.to_string();
    assert!(final_text.starts_with("OFFLINE-HOST HOSTPHRASE Abstract"));
    assert!(final_text.ends_with(" GUESTPHRASE OFFLINE-GUEST"));
    assert_eq!(final_text, guest.rope.to_string());

    // 5. The relay saw only ciphertext.
    host.session.handle(
        t0.elapsed().as_millis() as u64,
        Input::Leave,
        &mut host.rope,
    );
    let frames = captured.lock().clone();
    assert!(frames.len() >= 6, "expected traffic, saw {}", frames.len());
    let secrets = [
        "HOSTPHRASE",
        "GUESTPHRASE",
        "OFFLINE-HOST",
        "OFFLINE-GUEST",
        "Abstract",
        "confidential",
        "reviewers",
        "méthode",
        "host",
        "guest",
    ];
    for frame in &frames {
        assert!(frame.len() >= 42 && frame[0] == 1 && (frame[1] == 1 || frame[1] == 2));
        for s in secrets {
            assert!(
                !frame.windows(s.len()).any(|w| w == s.as_bytes()),
                "relay saw plaintext {s:?}"
            );
        }
    }
    println!(
        "{}: relay forwarded {} frames, {} bytes, none containing plaintext",
        E::name(),
        frames.len(),
        frames.iter().map(Vec::len).sum::<usize>()
    );

    rt.block_on(relay.stop());
}

#[test]
fn two_clients_loro_through_a_real_relay() {
    scenario::<LoroEngine>();
}

#[test]
fn two_clients_yrs_through_a_real_relay() {
    scenario::<YrsEngine>();
}
