//! Convergence under a hostile network, for both engines.
//!
//! Property 1: with reordering, duplication, loss and partitions, once the
//! network heals every client ends with byte-equal text (rope and CRDT).
//! Property 2: when edits are strictly sequential (one writer at a time,
//! full settle in between) the result equals a plain `String` oracle.

#[cfg(feature = "yrs-engine")]
use manang_live::YrsEngine;
use manang_live::sim::{NetConfig, Sim};
use manang_live::{LoroEngine, TextCrdt};
use proptest::prelude::*;

const SNIPPETS: &[&str] = &[
    "a",
    "hello ",
    "é",
    "😀",
    "日本語",
    "\n",
    "x😀y",
    "👩‍👩‍👧",
    "\\section{Intro}\n",
    "𝔘𝔫𝔦",
    " ",
];

#[derive(Debug, Clone)]
enum Op {
    Type {
        client: usize,
        at: u16,
        del: u8,
        snippet: u8,
    },
    Offline(usize),
    Online(usize),
    Wait(u16),
}

fn op_strategy(clients: usize) -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => (0..clients, 0u16..=1000, 0u8..6, any::<u8>()).prop_map(|(client, at, del, snippet)| Op::Type { client, at, del, snippet }),
        1 => (0..clients).prop_map(Op::Offline),
        1 => (0..clients).prop_map(Op::Online),
        2 => (1u16..1500).prop_map(Op::Wait),
    ]
}

fn char_edit(len: usize, at: u16, del: u8) -> (usize, usize) {
    let from = len * usize::from(at) / 1000;
    let to = (from + usize::from(del)).min(len);
    (from, to)
}

fn run_hostile<E: TextCrdt>(ops: &[Op], seed: u64) -> Result<(), TestCaseError> {
    let mut sim = Sim::<E>::new(3, "seed text é😀\n", seed, NetConfig::HOSTILE)
        .map_err(|e| TestCaseError::fail(e.to_string()))?;
    for op in ops {
        match op {
            Op::Type {
                client,
                at,
                del,
                snippet,
            } => {
                let len = sim.clients[*client].rope.len_chars();
                let (from, to) = char_edit(len, *at, *del);
                let text = SNIPPETS[usize::from(*snippet) % SNIPPETS.len()];
                sim.edit(*client, from, to, text)
                    .map_err(|e| TestCaseError::fail(e.to_string()))?;
            }
            Op::Offline(c) => {
                if sim.clients[*c].online {
                    sim.disconnect(*c);
                }
            }
            Op::Online(c) => {
                if !sim.clients[*c].online {
                    sim.connect(*c);
                }
            }
            Op::Wait(ms) => sim.advance(u64::from(*ms)),
        }
    }
    prop_assert!(sim.heal(300_000), "did not converge");
    let first = sim.clients[0].rope.to_string();
    for c in &sim.clients {
        prop_assert_eq!(c.rope.to_string(), first.clone());
        prop_assert_eq!(c.session.doc().engine().text(), first.clone());
    }
    Ok(())
}

fn run_sequential<E: TextCrdt>(
    ops: &[(usize, u16, u8, u8)],
    seed: u64,
) -> Result<(), TestCaseError> {
    let seedtext = "seed é😀\n";
    let mut sim = Sim::<E>::new(3, seedtext, seed, NetConfig::CLEAN)
        .map_err(|e| TestCaseError::fail(e.to_string()))?;
    sim.advance(5_000); // let the joiners receive the seed
    let mut oracle: Vec<char> = seedtext.chars().collect();
    for (client, at, del, snippet) in ops {
        let len = oracle.len();
        let (from, to) = char_edit(len, *at, *del);
        let text = SNIPPETS[usize::from(*snippet) % SNIPPETS.len()];
        sim.edit(*client, from, to, text)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        oracle.splice(from..to, text.chars());
        sim.advance(500);
        let want: String = oracle.iter().collect();
        for c in &sim.clients {
            prop_assert_eq!(c.rope.to_string(), want.clone());
        }
    }
    let want: String = oracle.iter().collect();
    for c in &sim.clients {
        prop_assert_eq!(c.session.doc().engine().text(), want.clone());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, .. ProptestConfig::default() })]

    #[test]
    fn hostile_network_converges_loro(ops in proptest::collection::vec(op_strategy(3), 1..60), seed in any::<u64>()) {
        run_hostile::<LoroEngine>(&ops, seed)?;
    }

    #[cfg(feature = "yrs-engine")]
    #[test]
    fn hostile_network_converges_yrs(ops in proptest::collection::vec(op_strategy(3), 1..60), seed in any::<u64>()) {
        run_hostile::<YrsEngine>(&ops, seed)?;
    }

    #[test]
    fn sequential_edits_match_oracle_loro(
        ops in proptest::collection::vec((0usize..3, 0u16..=1000, 0u8..6, any::<u8>()), 1..40),
        seed in any::<u64>(),
    ) {
        run_sequential::<LoroEngine>(&ops, seed)?;
    }

    #[cfg(feature = "yrs-engine")]
    #[test]
    fn sequential_edits_match_oracle_yrs(
        ops in proptest::collection::vec((0usize..3, 0u16..=1000, 0u8..6, any::<u8>()), 1..40),
        seed in any::<u64>(),
    ) {
        run_sequential::<YrsEngine>(&ops, seed)?;
    }
}
