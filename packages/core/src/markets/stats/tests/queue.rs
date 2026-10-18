//! The payout queue as the relayer answered it, empty, and a queue with winners waiting.

use super::*;

#[test]
fn a_queue_reads_its_total_count_and_entries() {
    let empty =
        QueueSummary::parse(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/queue_summary.json"))).unwrap();
    assert_eq!(empty, QueueSummary { block: 48_399_239, total: 0, count: 0, largest: None, front: None });

    let entry = r#"{"wallet":"0x9587c978341b4de61fa14e4705f65fbaaec72741","owed":"12000000000000000000"}"#;
    let waiting = format!(
        r#"{{"ok":true,"source":{{"block":"5","at":1}},"total":"12000000000000000000","count":1,"largest":{entry},"front":{entry}}}"#
    );
    let waiting = QueueSummary::parse(&waiting).unwrap();
    assert_eq!((waiting.block, waiting.total, waiting.count), (5, 12_000_000_000_000_000_000, 1));
    assert_eq!(waiting.largest.map(|json| json.compact()), Some(entry.to_owned()));
    assert_eq!(waiting.front.map(|json| json.compact()), Some(entry.to_owned()));
}

#[test]
fn a_queue_that_does_not_read_names_what_failed() {
    for (text, what) in [
        (r#"{"total":"0","count":0}"#, "source"),
        (r#"{"source":{},"total":"0","count":0}"#, "block"),
        (r#"{"source":{"block":"5"},"total":"-1","count":0}"#, "total"),
        (r#"{"source":{"block":"5"},"total":"0","count":"many"}"#, "count"),
        ("<html>", "queue_summary"),
    ] {
        assert_eq!(QueueSummary::parse(text).map_err(|bad| bad.what), Err(what.to_owned()), "{text}");
    }
}
