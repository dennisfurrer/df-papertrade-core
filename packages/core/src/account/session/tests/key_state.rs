//! The key's state from its record alone, from the stream's key list, and while the indexer lags.

use super::*;
use crate::test_support::{NOW_MS, TTL, account, record, session_address};

#[test]
fn the_key_state_reads_the_record_and_the_stream() {
    assert_eq!(key_state(None, None, NOW_MS), KeyState::Missing);
    assert_eq!(key_state(Some(&record(false)), None, NOW_MS), KeyState::Unregistered);
    assert_eq!(key_state(Some(&record(true)), None, NOW_MS), KeyState::Ready);
    assert_eq!(key_state(Some(&record(true)), None, NOW_MS + TTL - 60_000), KeyState::Missing, "too near its end");

    let listed = format!(r#"["{}","{}"]"#, session_address().to_hex(), (NOW_MS + TTL) / 1000);
    assert_eq!(
        key_state(Some(&record(false)), Some(&account(&listed)), NOW_MS),
        KeyState::Ready,
        "the stream lists it"
    );
    let unlisted = account("");
    assert_eq!(key_state(Some(&record(false)), Some(&unlisted), NOW_MS), KeyState::Unregistered);
    assert_eq!(key_state(Some(&record(true)), Some(&unlisted), NOW_MS), KeyState::Ready, "the indexer may lag");
    let long_ago = SessionRecord { registered_at_ms: Some(NOW_MS - 10 * 60_000), ..record(true) };
    assert_eq!(
        key_state(Some(&long_ago), Some(&unlisted), NOW_MS),
        KeyState::Unregistered,
        "taken ten minutes before the indexer's time and still not listed: it did not land"
    );
}
