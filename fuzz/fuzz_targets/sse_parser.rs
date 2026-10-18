#![no_main]
//! The SSE parser must never panic and must give the same events however the
//! input is chunked. Run with a cap: `cargo fuzz run sse_parser -- -max_total_time=60`.

use libfuzzer_sys::fuzz_target;
use manang_agent_core::SseParser;

fuzz_target!(|data: &[u8]| {
    let whole = SseParser::new().feed(data);
    // Split at a data-derived point and feed in two chunks.
    let cut = data.first().map_or(0, |b| usize::from(*b) % (data.len() + 1));
    let mut parser = SseParser::new();
    let first = parser.feed(&data[..cut]);
    let second = parser.feed(&data[cut..]);
    if let (Ok(whole), Ok(mut a), Ok(b)) = (whole, first, second) {
        a.extend(b);
        assert_eq!(whole, a);
    }
});
