//! The encrypted update envelope.
//!
//! The relay (or any transport) must see only ciphertext. A room is defined by
//! a 256-bit **room secret** that travels in the invite link's URL fragment,
//! which browsers and HTTP clients never send to a server. Two independent
//! values are derived from it with HKDF-SHA256:
//!
//! * the **AEAD key** (never leaves the participants), and
//! * the **room id** the relay uses to route frames (a one-way function of the
//!   secret, so knowing the room id does not reveal the key).
//!
//! Wire format, version 1:
//!
//! ```text
//! byte 0        version (1)
//! byte 1        kind (see [`Kind`]); visible to the relay by design
//! bytes 2..26   XChaCha20 nonce, 24 random bytes
//! bytes 26..    ciphertext || 16-byte Poly1305 tag
//! ```
//!
//! The associated data is `version || kind || room_id`, so a frame cannot be
//! replayed into another room, downgraded to another kind, or re-versioned.
//! 192-bit random nonces make accidental reuse negligible without any counter
//! state, which matters because replicas restart and reconnect freely.
//!
//! All participants share the key, so the scheme provides confidentiality and
//! integrity against the relay and outsiders, not authentication between
//! participants: anyone holding the invite can speak as anyone in the room.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::error::LiveError;

/// Envelope version this build writes and reads.
pub const VERSION: u8 = 1;
/// Nonce length of XChaCha20-Poly1305.
pub const NONCE_LEN: usize = 24;
/// Poly1305 tag length.
pub const TAG_LEN: usize = 16;
/// Header length: version, kind, nonce.
pub const HEADER_LEN: usize = 2 + NONCE_LEN;
/// Largest plaintext accepted, to bound allocation on hostile input.
pub const MAX_PLAINTEXT: usize = 32 * 1024 * 1024;
/// Length of the room secret.
pub const SECRET_LEN: usize = 32;
/// Length of the derived room id.
pub const ROOM_ID_LEN: usize = 16;

const HKDF_SALT: &[u8] = b"manang-live/v1";
const INFO_KEY: &[u8] = b"aead-key";
const INFO_ROOM: &[u8] = b"room-id";

/// What an envelope carries. Sent in the clear (and authenticated) so a relay
/// can apply different rate limits to chatty ephemeral traffic and to updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// Document synchronisation and updates.
    Data = 1,
    /// Ephemeral presence (cursors, names). Never persisted anywhere.
    Presence = 2,
}

impl Kind {
    fn from_byte(b: u8) -> Result<Self, LiveError> {
        match b {
            1 => Ok(Self::Data),
            2 => Ok(Self::Presence),
            _ => Err(LiveError::Envelope("unknown kind")),
        }
    }
}

/// A source of random bytes. The core never reaches for the OS itself so it
/// stays usable in a browser; native code passes [`OsEntropy`].
pub trait Entropy {
    /// Fill `buf` with unpredictable bytes.
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), LiveError>;
}

/// Operating-system randomness (not available on `wasm32`; browsers inject
/// `crypto.getRandomValues` through their own [`Entropy`]).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct OsEntropy;

#[cfg(not(target_arch = "wasm32"))]
impl Entropy for OsEntropy {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), LiveError> {
        getrandom::fill(buf).map_err(|e| LiveError::Engine(format!("entropy: {e}")))
    }
}

/// The 256-bit secret that defines a room.
pub struct RoomSecret([u8; SECRET_LEN]);

impl Drop for RoomSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl RoomSecret {
    /// A fresh random secret.
    pub fn generate(entropy: &mut impl Entropy) -> Result<Self, LiveError> {
        let mut bytes = [0u8; SECRET_LEN];
        entropy.fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Wrap existing secret bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LiveError> {
        let arr: [u8; SECRET_LEN] = bytes.try_into().map_err(|_| LiveError::BadSecret)?;
        Ok(Self(arr))
    }

    /// The raw bytes, for building an invite.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; SECRET_LEN] {
        &self.0
    }

    /// Derive the AEAD key and the routing id.
    pub fn derive(&self) -> Result<RoomKeys, LiveError> {
        let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), &self.0);
        let mut key = [0u8; 32];
        let mut room_id = [0u8; ROOM_ID_LEN];
        hk.expand(INFO_KEY, &mut key)
            .map_err(|_| LiveError::Envelope("key derivation failed"))?;
        hk.expand(INFO_ROOM, &mut room_id)
            .map_err(|_| LiveError::Envelope("room id derivation failed"))?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| LiveError::Envelope("bad key length"))?;
        key.zeroize();
        Ok(RoomKeys { cipher, room_id })
    }
}

/// Keys derived from a room secret. Cheap to clone the id, not the cipher.
pub struct RoomKeys {
    cipher: XChaCha20Poly1305,
    room_id: [u8; ROOM_ID_LEN],
}

impl RoomKeys {
    /// The routing id the relay sees.
    #[must_use]
    pub fn room_id(&self) -> [u8; ROOM_ID_LEN] {
        self.room_id
    }

    /// The routing id as lowercase hex, for URLs and logs.
    #[must_use]
    pub fn room_id_hex(&self) -> String {
        hex(&self.room_id)
    }

    fn aad(&self, kind: Kind) -> [u8; 2 + ROOM_ID_LEN] {
        let mut aad = [0u8; 2 + ROOM_ID_LEN];
        aad[0] = VERSION;
        aad[1] = kind as u8;
        aad[2..].copy_from_slice(&self.room_id);
        aad
    }

    /// Encrypt `plaintext` into a version-1 envelope with a random nonce.
    pub fn seal(
        &self,
        kind: Kind,
        plaintext: &[u8],
        entropy: &mut impl Entropy,
    ) -> Result<Vec<u8>, LiveError> {
        let mut nonce = [0u8; NONCE_LEN];
        entropy.fill(&mut nonce)?;
        self.seal_with_nonce(kind, plaintext, &nonce)
    }

    /// Encrypt with a caller-chosen nonce. A nonce must never be reused with
    /// the same key; this exists for known-answer tests.
    pub fn seal_with_nonce(
        &self,
        kind: Kind,
        plaintext: &[u8],
        nonce: &[u8; NONCE_LEN],
    ) -> Result<Vec<u8>, LiveError> {
        if plaintext.len() > MAX_PLAINTEXT {
            return Err(LiveError::Envelope("plaintext too large"));
        }
        let aad = self.aad(kind);
        let ct = self
            .cipher
            .encrypt(
                &XNonce::from(*nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| LiveError::Envelope("encryption failed"))?;
        let mut out = Vec::with_capacity(HEADER_LEN + ct.len());
        out.push(VERSION);
        out.push(kind as u8);
        out.extend_from_slice(nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Authenticate and decrypt an envelope.
    pub fn open(&self, envelope: &[u8]) -> Result<(Kind, Vec<u8>), LiveError> {
        if envelope.len() < HEADER_LEN + TAG_LEN {
            return Err(LiveError::Envelope("too short"));
        }
        if envelope[0] != VERSION {
            return Err(LiveError::Envelope("unsupported version"));
        }
        let kind = Kind::from_byte(envelope[1])?;
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&envelope[2..HEADER_LEN]);
        let aad = self.aad(kind);
        let plaintext = self
            .cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &envelope[HEADER_LEN..],
                    aad: &aad,
                },
            )
            .map_err(|_| LiveError::Envelope("authentication failed"))?;
        Ok((kind, plaintext))
    }
}

/// A short fingerprint of seed text, carried in the invite so a joiner can
/// tell whether its file already equals the host's seed.
#[must_use]
pub fn seed_fingerprint(text: &str) -> [u8; 16] {
    let digest = Sha256::digest(text.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(DIGITS[usize::from(b >> 4)]));
        s.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    s
}

/// An invite: everything a joiner needs that must not reach the relay.
/// Rendered as `base64url(secret || seed_fingerprint)` for the URL fragment.
pub struct Invite {
    /// The room secret.
    pub secret: RoomSecret,
    /// Fingerprint of the host's seed text.
    pub seed: [u8; 16],
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

impl Invite {
    /// Encode for a URL fragment (no padding).
    #[must_use]
    pub fn to_fragment(&self) -> String {
        let mut raw = Vec::with_capacity(SECRET_LEN + 16);
        raw.extend_from_slice(&self.secret.0);
        raw.extend_from_slice(&self.seed);
        let mut out = String::with_capacity(raw.len().div_ceil(3) * 4);
        for chunk in raw.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            let chars = chunk.len() + 1;
            for i in 0..chars {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            }
        }
        raw.zeroize();
        out
    }

    /// Parse a fragment produced by [`Invite::to_fragment`].
    pub fn from_fragment(fragment: &str) -> Result<Self, LiveError> {
        let mut raw = Vec::with_capacity(48);
        let mut acc = 0u32;
        let mut bits = 0u32;
        for c in fragment.bytes() {
            let v = B64
                .iter()
                .position(|&x| x == c)
                .ok_or(LiveError::Envelope("invalid invite character"))?;
            acc = (acc << 6) | v as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                raw.push(((acc >> bits) & 0xff) as u8);
            }
        }
        if raw.len() != SECRET_LEN + 16 {
            raw.zeroize();
            return Err(LiveError::Envelope("invite has the wrong length"));
        }
        let secret = RoomSecret::from_bytes(&raw[..SECRET_LEN])?;
        let mut seed = [0u8; 16];
        seed.copy_from_slice(&raw[SECRET_LEN..]);
        raw.zeroize();
        Ok(Self { secret, seed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic entropy for tests only.
    struct Counter(u8);
    impl Entropy for Counter {
        fn fill(&mut self, buf: &mut [u8]) -> Result<(), LiveError> {
            for b in buf {
                self.0 = self.0.wrapping_add(1);
                *b = self.0;
            }
            Ok(())
        }
    }

    fn keys(seed: u8) -> RoomKeys {
        RoomSecret::generate(&mut Counter(seed))
            .expect("secret")
            .derive()
            .expect("derive")
    }

    #[test]
    fn roundtrip_both_kinds() {
        let k = keys(1);
        let mut e = Counter(100);
        for kind in [Kind::Data, Kind::Presence] {
            let sealed = k.seal(kind, b"hello world", &mut e).expect("seal");
            assert_eq!(sealed.len(), HEADER_LEN + 11 + TAG_LEN);
            let (got_kind, pt) = k.open(&sealed).expect("open");
            assert_eq!(got_kind, kind);
            assert_eq!(pt, b"hello world");
        }
    }

    #[test]
    fn plaintext_absent_from_ciphertext() {
        let k = keys(1);
        let msg = b"the quick brown fox jumps over the lazy dog";
        let sealed = k.seal(Kind::Data, msg, &mut Counter(7)).expect("seal");
        assert!(!sealed.windows(msg.len()).any(|w| w == msg));
        assert!(!sealed.windows(9).any(|w| w == &msg[..9]));
    }

    #[test]
    fn every_single_bit_flip_is_rejected() {
        let k = keys(1);
        let sealed = k
            .seal(Kind::Data, b"payload", &mut Counter(7))
            .expect("seal");
        for i in 0..sealed.len() {
            for bit in 0..8 {
                let mut t = sealed.clone();
                t[i] ^= 1 << bit;
                assert!(k.open(&t).is_err(), "flip byte {i} bit {bit} accepted");
            }
        }
    }

    #[test]
    fn wrong_room_wrong_key_and_truncation_rejected() {
        let a = keys(1);
        let b = keys(50);
        let sealed = a.seal(Kind::Data, b"x", &mut Counter(7)).expect("seal");
        assert!(b.open(&sealed).is_err());
        assert!(a.open(&sealed[..sealed.len() - 1]).is_err());
        assert!(a.open(&sealed[..HEADER_LEN]).is_err());
        assert!(a.open(&[]).is_err());
        // Appended bytes break the tag too.
        let mut longer = sealed.clone();
        longer.push(0);
        assert!(a.open(&longer).is_err());
    }

    #[test]
    fn kind_and_version_are_authenticated() {
        let k = keys(1);
        let sealed = k.seal(Kind::Data, b"x", &mut Counter(7)).expect("seal");
        let mut as_presence = sealed.clone();
        as_presence[1] = Kind::Presence as u8;
        assert!(k.open(&as_presence).is_err());
        let mut v2 = sealed;
        v2[0] = 2;
        assert_eq!(
            k.open(&v2).err(),
            Some(LiveError::Envelope("unsupported version"))
        );
    }

    #[test]
    fn room_id_is_not_the_key_and_differs_per_secret() {
        let a = keys(1);
        let b = keys(2);
        assert_ne!(a.room_id(), b.room_id());
        assert_eq!(a.room_id_hex().len(), 32);
    }

    #[test]
    fn nonces_differ_between_messages() {
        let k = keys(1);
        let mut e = Counter(0);
        let a = k.seal(Kind::Data, b"same", &mut e).expect("a");
        let b = k.seal(Kind::Data, b"same", &mut e).expect("b");
        assert_ne!(a, b);
    }

    #[test]
    fn known_answer_is_stable() {
        // Pins the derivation and the wire format: a change here is a
        // protocol break and must bump VERSION.
        let secret = RoomSecret::from_bytes(&[7u8; 32]).expect("secret");
        let k = secret.derive().expect("derive");
        let sealed = k
            .seal_with_nonce(Kind::Data, b"kat", &[9u8; NONCE_LEN])
            .expect("seal");
        assert_eq!(&sealed[..2], &[1, 1]);
        assert_eq!(&sealed[2..26], &[9u8; 24]);
        assert_eq!(sealed.len(), 26 + 3 + 16);
        let (_, pt) = k.open(&sealed).expect("open");
        assert_eq!(pt, b"kat");
        assert_eq!(k.room_id_hex(), "5c5eea1f2c9d73a01fcb5d0e9655d2d7");
        assert_eq!(
            hex(&sealed),
            "01010909090909090909090909090909090909090909090909098bcfa06dc1d9e4972e0bbde1e89ecf94167fb0"
        );
    }

    #[test]
    fn invite_roundtrip_and_rejects_garbage() {
        let inv = Invite {
            secret: RoomSecret::from_bytes(&[0xabu8; 32]).expect("s"),
            seed: seed_fingerprint("hello"),
        };
        let frag = inv.to_fragment();
        assert_eq!(frag.len(), 64);
        assert!(!frag.contains('=') && !frag.contains('+') && !frag.contains('/'));
        let back = Invite::from_fragment(&frag).expect("parse");
        assert_eq!(back.secret.as_bytes(), inv.secret.as_bytes());
        assert_eq!(back.seed, inv.seed);
        assert!(Invite::from_fragment("short").is_err());
        assert!(Invite::from_fragment(&frag.replace('A', "!")).is_err() || !frag.contains('A'));
        assert!(RoomSecret::from_bytes(&[0u8; 31]).is_err());
    }
}
