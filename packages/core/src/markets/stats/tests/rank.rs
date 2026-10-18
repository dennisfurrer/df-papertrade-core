//! A wallet's close queued 37th as the relayer answered it, a wallet with nothing queued, and
//! answers that do not read.

use df_hyperliquid_core::utils::hex;

use super::*;

#[test]
fn a_queue_rank_reads() {
    let rank =
        QueueRank::parse(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/queue_rank.json"))).unwrap();
    assert_eq!(rank.revision, Some(5_646));
    assert_eq!(rank.entries.len(), 1);
    assert_eq!(
        hex::encode_prefixed(&rank.entries[0].0),
        "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d"
    );
    assert_eq!(rank.entries[0].1, 37);
    assert_eq!(QueueRank::parse(r#"{"ok":true,"entries":[]}"#), Ok(QueueRank { revision: None, entries: Vec::new() }));
}

#[test]
fn a_rank_that_does_not_read_names_what_failed() {
    let id = "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d";
    for (text, what) in [
        (r#"{"queueRevision":"1"}"#.to_owned(), "entries"),
        (r#"{"queueRevision":"one","entries":[]}"#.to_owned(), "queueRevision"),
        (r#"{"entries":{}}"#.to_owned(), "entries"),
        (format!(r#"{{"entries":[{{"intentId":"{id}","position":"first"}}]}}"#), "position"),
        (r#"{"entries":[{"intentId":"0x66","position":"1"}]}"#.to_owned(), "intentId"),
        (r#"{"entries":[{"position":"1"}]}"#.to_owned(), "intentId"),
    ] {
        assert_eq!(QueueRank::parse(&text).map_err(|bad| bad.what), Err(what.to_owned()), "{text}");
    }
}
