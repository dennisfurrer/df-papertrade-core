//! A wallet's close queued 37th, as the relayer answered it.

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
}
