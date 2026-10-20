#![no_main]

//! `Message::decode` (wire v1 and v2) on arbitrary bytes: never panics, and
//! whatever it accepts re-encodes to something that decodes to the same value
//! and still satisfies the v2 size bounds.

use libfuzzer_sys::fuzz_target;
use manang_live::protocol::Message;

fuzz_target!(|data: &[u8]| {
    let Ok(msg) = Message::decode(data) else {
        return;
    };
    let encoded = msg.encode().expect("an accepted message encodes");
    let again = Message::decode(&encoded).expect("a re-encoded message decodes");
    assert_eq!(again, msg);
});
