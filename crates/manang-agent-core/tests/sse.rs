use manang_agent_core::{SseEvent, SseParser};
use proptest::prelude::*;

fn parse_whole(input: &[u8]) -> Vec<SseEvent> {
    SseParser::new().feed(input).unwrap()
}

fn ev(event: Option<&str>, data: &str) -> SseEvent {
    SseEvent {
        event: event.map(str::to_owned),
        data: data.to_owned(),
        id: None,
    }
}

#[test]
fn basic_frames() {
    let got = parse_whole(b"event: a\ndata: {\"x\":1}\n\nevent: b\ndata: 2\n\n");
    assert_eq!(got, vec![ev(Some("a"), "{\"x\":1}"), ev(Some("b"), "2")]);
}

#[test]
fn crlf_cr_and_mixed_endings() {
    let got = parse_whole(b"data: 1\r\n\r\ndata: 2\r\rdata: 3\n\r\n");
    assert_eq!(got, vec![ev(None, "1"), ev(None, "2"), ev(None, "3")]);
}

#[test]
fn comments_and_pings_are_skipped() {
    let got = parse_whole(b": keep-alive\n\nevent: ping\ndata: {}\n\n: x\n");
    assert_eq!(got, vec![ev(Some("ping"), "{}")]);
}

#[test]
fn multi_line_data_joins_with_newline() {
    let got = parse_whole(b"data: a\ndata:b\ndata\ndata: c\n\n");
    assert_eq!(got, vec![ev(None, "a\nb\n\nc")]);
}

#[test]
fn only_one_leading_space_is_stripped_and_bom_ignored() {
    let got = parse_whole("\u{feff}data:  two\n\n".as_bytes());
    assert_eq!(got, vec![ev(None, " two")]);
}

#[test]
fn unterminated_event_is_not_dispatched() {
    assert!(parse_whole(b"data: partial\n").is_empty());
}

#[test]
fn frame_split_across_chunks_byte_by_byte() {
    let input = b"event: content_block_delta\r\ndata: {\"a\":\"\xc3\xa9\"}\r\n\r\n";
    let mut p = SseParser::new();
    let mut got = Vec::new();
    for b in input {
        got.extend(p.feed(&[*b]).unwrap());
    }
    assert_eq!(got, vec![ev(Some("content_block_delta"), "{\"a\":\"é\"}")]);
}

#[test]
fn crlf_split_between_chunks_is_one_line_end() {
    let mut p = SseParser::new();
    let mut got = p.feed(b"data: 1\r").unwrap();
    got.extend(p.feed(b"\n\r").unwrap());
    got.extend(p.feed(b"\ndata: 2\n\n").unwrap());
    assert_eq!(got, vec![ev(None, "1"), ev(None, "2")]);
}

#[test]
fn oversized_event_is_an_error_not_unbounded_memory() {
    let mut p = SseParser::new();
    let chunk = vec![b'a'; 1024 * 1024];
    let mut failed = false;
    for _ in 0..20 {
        if p.feed(&chunk).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
}

proptest! {
    #[test]
    fn any_chunking_gives_the_same_events(
        input in proptest::collection::vec(prop_oneof![
            Just(&b"data: "[..]), Just(&b"event: x"[..]), Just(&b"\n"[..]), Just(&b"\r\n"[..]),
            Just(&b"\r"[..]), Just(&b": c"[..]), Just(&b"{\"k\":1}"[..]), Just(&b"\xc3\xa9"[..]),
        ], 0..40),
        cuts in proptest::collection::vec(0usize..8, 0..10),
    ) {
        let bytes: Vec<u8> = input.concat();
        let whole = SseParser::new().feed(&bytes).unwrap();
        let mut p = SseParser::new();
        let mut got = Vec::new();
        let mut at = 0;
        for c in cuts {
            let end = (at + c).min(bytes.len());
            got.extend(p.feed(&bytes[at..end]).unwrap());
            at = end;
        }
        got.extend(p.feed(&bytes[at..]).unwrap());
        prop_assert_eq!(got, whole);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let _ = SseParser::new().feed(&bytes);
    }
}
