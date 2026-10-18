//! Live co-editing core for Manang.
//!
//! Pure, synchronous Rust with no OS dependency, so it also builds for
//! `wasm32-unknown-unknown`. The pieces:
//!
//! * [`engine`]: the [`TextCrdt`] trait, with [`LoroEngine`] and [`YrsEngine`]
//!   behind it so the engines can be measured against identical code.
//! * [`bridge`]: keeps the editor's rope and the CRDT in step.
//! * [`envelope`]: versioned XChaCha20-Poly1305 envelope, keys from HKDF.
//! * [`protocol`], [`presence`], [`session`], [`transport`]: the sans-IO
//!   session state machine and the [`SyncTransport`] trait.

pub mod bridge;
pub mod engine;
pub mod envelope;
pub mod error;
pub mod loro_engine;
pub mod presence;
pub mod protocol;
pub mod session;
pub mod sim;
pub mod transport;
pub mod yrs_engine;

pub use engine::{Applied, IndexUnit, RawChange, SEED_PEER, TextCrdt};
pub use error::LiveError;
pub use loro_engine::LoroEngine;
pub use yrs_engine::YrsEngine;

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<E: TextCrdt>() {
        let mut a = E::new(10).expect("a");
        let mut b = E::new(11).expect("b");
        a.insert(0, "a😀b").expect("insert");
        let updates = a.take_local_updates().expect("updates");
        assert_eq!(updates.len(), 1);
        let applied = b.apply_update(&updates[0]).expect("apply");
        assert_eq!(b.text(), "a😀b");
        println!(
            "{} insert unit {:?} {:?}",
            E::name(),
            applied.unit,
            applied.changes
        );
        a.delete(1, 4).expect("delete");
        let updates = a.take_local_updates().expect("updates");
        let applied = b.apply_update(&updates[0]).expect("apply");
        println!(
            "{} delete unit {:?} {:?}",
            E::name(),
            applied.unit,
            applied.changes
        );
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn units_loro() {
        roundtrip::<LoroEngine>();
    }

    #[test]
    fn units_yrs() {
        roundtrip::<YrsEngine>();
    }
}
