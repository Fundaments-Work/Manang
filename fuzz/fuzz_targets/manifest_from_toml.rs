#![no_main]

//! `Manifest::from_toml` must never panic on any input, and anything it
//! accepts must be fully valid and round-trip through serialisation.

use libfuzzer_sys::fuzz_target;
use manang_kernel::Manifest;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(manifest) = Manifest::from_toml(text) else {
        return;
    };
    // Accepted manifests are valid by construction ...
    assert!(manifest.validate().is_ok());
    assert!(manifest.capabilities().is_ok());
    // ... and survive a trip through JSON unchanged.
    let json = serde_json::to_string(&manifest).expect("manifest serialises");
    let back: Manifest = serde_json::from_str(&json).expect("manifest deserialises");
    assert_eq!(back, manifest);
});
