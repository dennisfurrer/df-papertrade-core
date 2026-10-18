//! The parser over a busy wallet's five-minute capture, in chunks of any size, and over the
//! format's corners: comments, CRLF, data split across lines, ids and fields it does not know.

use super::*;
use crate::test_support::STREAM;

fn event(name: &str, data: &str, id: Option<&str>) -> Event {
    Event { name: name.to_owned(), data: data.to_owned(), id: id.map(str::to_owned) }
}

#[test]
fn the_capture_splits_into_its_events() {
    let events = Events::new().push(STREAM);
    assert_eq!(events[0], event(LIVENESS, r#"{"heartbeatIntervalMs":10000}"#, None));
    for (name, count) in [(LIVENESS, 1), (WALLET, 13), (HEARTBEAT, 12)] {
        assert_eq!(events.iter().filter(|event| event.name == name).count(), count, "{name}");
    }
    assert!(events.iter().filter(|event| event.name == HEARTBEAT).all(|event| event.data == "null"));
}

#[test]
fn any_chunking_gives_the_same_events() {
    let whole = Events::new().push(STREAM);
    for size in [1, 7, 64, 1000] {
        let mut events = Events::new();
        let mut got = Vec::new();
        let mut start = 0;
        while start < STREAM.len() {
            let mut end = (start + size).min(STREAM.len());
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
fn each_line_reads_as_the_format_says() {
    let cases = [
        (": connected\n\n", vec![]),
        ("data: a\n\n", vec![event("message", "a", None)]),
        ("data:a\n\n", vec![event("message", "a", None)]),
        ("event: x\r\ndata: a\r\ndata: b\r\n\r\n", vec![event("x", "a\nb", None)]),
        ("id: 7\ndata: a\n\ndata: b\n\n", vec![event("message", "a", Some("7")), event("message", "b", None)]),
        ("event: x\n\n", vec![event("x", "", None)]),
        ("data\n\n", vec![event("message", "", None)]),
        ("retry: 5\nfoo\n\n", vec![]),
        ("data: a\n", vec![]),
    ];
    for (text, expected) in cases {
        assert_eq!(Events::new().push(text), expected, "{text:?}");
    }
}

#[test]
fn only_a_liveness_event_names_the_heartbeat_interval() {
    let cases = [(r#"{"heartbeatIntervalMs":10000}"#, Some(10_000)), ("null", None), ("{}", None), ("not json", None)];
    for (data, expected) in cases {
        assert_eq!(heartbeat_ms(&event(LIVENESS, data, None)), expected, "{data}");
    }
}
