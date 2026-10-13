use super::*;

const STREAM: &str = include_str!("../../fixtures/live_stream.txt");

#[test]
fn the_live_stream_splits_into_its_events() {
    let events = Events::new().push(STREAM);
    assert_eq!(events[0].name, LIVENESS);
    assert_eq!(heartbeat_ms(&events[0]), Some(10_000));
    let wallet = events.iter().filter(|event| event.name == WALLET).count();
    assert_eq!(wallet, 13);
    assert!(events.iter().any(|event| event.name == HEARTBEAT && event.data == "null"));
}

/// A chunk can end anywhere, mid-line or mid-event.
#[test]
fn any_chunking_gives_the_same_events() {
    let whole = Events::new().push(STREAM);
    for size in [1, 7, 64, 1000] {
        let mut events = Events::new();
        let mut got = Vec::new();
        let bytes = STREAM.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            let mut end = (start + size).min(bytes.len());
            while !STREAM.is_char_boundary(end) {
                end += 1;
            }
            got.extend(events.push(&STREAM[start..end]));
            start = end;
        }
        assert_eq!(got, whole, "chunks of {size}");
    }
}

#[test]
fn comments_are_skipped_and_lines_joined() {
    let events = Events::new().push(": connected\r\n\r\nevent: x\r\ndata: a\r\ndata: b\r\nid: 7\r\n\r\ndata:c\n\n");
    assert_eq!(
        events,
        vec![
            Event { name: "x".to_owned(), data: "a\nb".to_owned(), id: Some("7".to_owned()) },
            Event { name: "message".to_owned(), data: "c".to_owned(), id: None },
        ]
    );
}
