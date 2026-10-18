//! A deterministic in-memory network for tests and benchmarks.
//!
//! Clients are full [`Session`]s over [`ropey::Rope`]s. The network delivers
//! frames after random delays (which reorders them), duplicates some, drops
//! some, and can cut a client off entirely (a partition). Time is virtual, so
//! runs are reproducible from a seed. Not for production use: the entropy
//! source is a seeded PRNG.

use ropey::Rope;

use crate::bridge::{LiveDoc, TextTarget};
use crate::engine::TextCrdt;
use crate::envelope::{Entropy, RoomKeys, RoomSecret};
use crate::error::LiveError;
use crate::session::{Input, Output, Session, SessionConfig};

/// SplitMix64: tiny, fast, good enough for test scheduling.
#[derive(Debug, Clone)]
pub struct Rng(pub u64);

impl Rng {
    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`; returns 0 for `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }
}

impl Entropy for Rng {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), LiveError> {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(())
    }
}

/// Network behaviour.
#[derive(Debug, Clone, Copy)]
pub struct NetConfig {
    /// Percent of frames silently lost.
    pub drop_pct: u64,
    /// Percent of frames delivered twice.
    pub dup_pct: u64,
    /// Maximum one-way delay in ms; delays are uniform in `0..=max_delay_ms`.
    pub max_delay_ms: u64,
}

impl NetConfig {
    /// A perfect network.
    pub const CLEAN: Self = Self {
        drop_pct: 0,
        dup_pct: 0,
        max_delay_ms: 0,
    };
    /// Reordering, duplication and loss.
    pub const HOSTILE: Self = Self {
        drop_pct: 10,
        dup_pct: 15,
        max_delay_ms: 400,
    };
}

/// One simulated participant.
pub struct SimClient<E: TextCrdt> {
    /// The session under test.
    pub session: Session<E, Rng>,
    /// The editor's rope.
    pub rope: Rope,
    /// Whether the link is up.
    pub online: bool,
}

enum Delivery {
    Frame(Vec<u8>),
}

/// The whole simulated room.
pub struct Sim<E: TextCrdt> {
    /// Participants.
    pub clients: Vec<SimClient<E>>,
    /// Every frame any client put on the wire (what a relay would see).
    pub wire: Vec<Vec<u8>>,
    inflight: Vec<(u64, u64, usize, Delivery)>,
    seq: u64,
    now: u64,
    /// Network behaviour; may be changed between steps.
    pub net: NetConfig,
    /// Scheduling randomness.
    pub rng: Rng,
}

impl<E: TextCrdt> Sim<E> {
    /// A room of `n` peers. Peer 0 hosts, seeding `seed_text`; the others
    /// join empty and receive it through the sync exchange.
    pub fn new(n: usize, seed_text: &str, seed: u64, net: NetConfig) -> Result<Self, LiveError> {
        let mut rng = Rng(seed);
        let secret = RoomSecret::generate(&mut rng)?;
        let mut clients = Vec::new();
        for i in 0..n {
            let keys: RoomKeys = secret.derive()?;
            let peer = 100 + i as u64;
            let (doc, rope) = if i == 0 {
                (
                    LiveDoc::<E>::seeded(peer, seed_text)?,
                    Rope::from_str(seed_text),
                )
            } else {
                (LiveDoc::<E>::empty(peer)?, Rope::new())
            };
            let session = Session::new(
                doc,
                keys,
                Rng(seed ^ ((i as u64 + 1) * 0x1234_5678_9abc)),
                SessionConfig {
                    name: format!("peer{i}"),
                    slot: (i % 8) as u8,
                    ..SessionConfig::default()
                },
            );
            clients.push(SimClient {
                session,
                rope,
                online: false,
            });
        }
        let mut sim = Self {
            clients,
            wire: Vec::new(),
            inflight: Vec::new(),
            seq: 0,
            now: 0,
            net,
            rng,
        };
        for i in 0..n {
            let out = sim.clients[i].session.start();
            sim.route(i, out);
            sim.connect(i);
        }
        Ok(sim)
    }

    /// Virtual time in ms.
    #[must_use]
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Bring client `i` online (a fresh connection).
    pub fn connect(&mut self, i: usize) {
        self.clients[i].online = true;
        let out = {
            let c = &mut self.clients[i];
            c.session.handle(self.now, Input::Up, &mut c.rope)
        };
        self.route(i, out);
    }

    /// Cut client `i` off: its pending inbound frames are lost.
    pub fn disconnect(&mut self, i: usize) {
        self.clients[i].online = false;
        self.inflight.retain(|(_, _, to, _)| *to != i);
        let out = {
            let c = &mut self.clients[i];
            c.session.handle(self.now, Input::Down, &mut c.rope)
        };
        self.route(i, out);
    }

    /// Client `i` types: replace characters `from..to` with `text`.
    pub fn edit(&mut self, i: usize, from: usize, to: usize, text: &str) -> Result<(), LiveError> {
        {
            let c = &mut self.clients[i];
            c.session
                .doc_mut()
                .local_replace_chars(&mut c.rope, from, to, text)?;
        }
        let out = self.clients[i].session.flush_local();
        self.route(i, out);
        Ok(())
    }

    fn route(&mut self, from: usize, outputs: Vec<Output>) {
        for output in outputs {
            let Output::Send(frame) = output else {
                continue;
            };
            if !self.clients[from].online {
                continue; // sent while down: lost, resync recovers it
            }
            self.wire.push(frame.clone());
            for to in 0..self.clients.len() {
                if to == from || !self.clients[to].online {
                    continue;
                }
                if self.rng.below(100) < self.net.drop_pct {
                    continue;
                }
                let copies = if self.rng.below(100) < self.net.dup_pct {
                    2
                } else {
                    1
                };
                for _ in 0..copies {
                    let delay = self.rng.below(self.net.max_delay_ms + 1);
                    self.seq += 1;
                    self.inflight.push((
                        self.now + delay,
                        self.seq,
                        to,
                        Delivery::Frame(frame.clone()),
                    ));
                }
            }
        }
    }

    /// Advance virtual time by `ms`, delivering due frames and ticking
    /// sessions every 100 ms.
    pub fn advance(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 100).min(end);
            loop {
                let due: Vec<usize> = {
                    let mut idx: Vec<usize> = (0..self.inflight.len())
                        .filter(|&k| self.inflight[k].0 <= self.now)
                        .collect();
                    idx.sort_by_key(|&k| (self.inflight[k].0, self.inflight[k].1));
                    idx
                };
                if due.is_empty() {
                    break;
                }
                // Take the earliest only, then re-scan: delivery can enqueue more.
                let k = due[0];
                let (_, _, to, Delivery::Frame(frame)) = self.inflight.remove(k);
                if !self.clients[to].online {
                    continue;
                }
                let out = {
                    let c = &mut self.clients[to];
                    c.session
                        .handle(self.now, Input::Frame(&frame), &mut c.rope)
                };
                self.route(to, out);
            }
            for i in 0..self.clients.len() {
                // Offline clients tick too: their reconnect timers run.
                let out = {
                    let c = &mut self.clients[i];
                    c.session.handle(self.now, Input::Tick, &mut c.rope)
                };
                self.route(i, out);
            }
        }
    }

    /// Heal the network, reconnect everyone and run until all ropes are
    /// equal (or `budget_ms` of virtual time passes). Returns whether they are.
    pub fn heal(&mut self, budget_ms: u64) -> bool {
        self.net = NetConfig::CLEAN;
        for i in 0..self.clients.len() {
            if !self.clients[i].online {
                self.connect(i);
            }
        }
        let end = self.now + budget_ms;
        while self.now < end {
            self.advance(1_000);
            if self.converged() {
                return true;
            }
        }
        self.converged()
    }

    /// Whether every rope and every CRDT text are byte-equal.
    #[must_use]
    pub fn converged(&self) -> bool {
        let first = self.clients[0].rope.to_text();
        self.clients
            .iter()
            .all(|c| c.rope.to_text() == first && c.session.doc().engine().text() == first)
    }
}
