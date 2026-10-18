//! Whether the stored key signs: a key that is absent, unreadable or near its end, the record
//! alone, the stream's key list, and the indexer's lag behind a registration.

use zeroize::Zeroizing;

use super::*;
use crate::test_support::{NOW_MS, TTL, account, record, session_address};

/// The stream's key list naming the session key until `expiry`, Unix seconds.
fn listed(expiry: u64) -> String {
    format!(r#"["{}","{expiry}"]"#, session_address().to_hex())
}

#[test]
fn a_key_that_is_absent_unreadable_or_near_its_end_is_missing() {
    let end = NOW_MS + TTL - RENEW_BEFORE_MS;
    let unreadable = SessionRecord { secret: Zeroizing::new([0; 32]), ..record(true) };
    let cases: [(&str, Option<SessionRecord>, Option<Account>, u64); 4] = [
        ("no record", None, None, NOW_MS),
        ("an hour and a minute from its end", Some(record(true)), None, end),
        ("near its end though the stream lists it", Some(record(true)), Some(account(&listed(u64::MAX))), end),
        ("a secret that is no key", Some(unreadable), Some(account("")), NOW_MS),
    ];
    for (name, record, account, now) in cases {
        assert_eq!(key_state(record.as_ref(), account.as_ref(), now), KeyState::Missing, "{name}");
    }
    assert_eq!(key_state(Some(&record(true)), None, end - 1), KeyState::Ready, "a millisecond before");
}

#[test]
fn with_no_account_streamed_the_record_decides() {
    for (registered, expected) in [(true, KeyState::Ready), (false, KeyState::Unregistered)] {
        assert_eq!(key_state(Some(&record(registered)), None, NOW_MS), expected, "registered: {registered}");
    }
}

#[test]
fn the_streams_key_list_decides_once_the_indexer_has_caught_up() {
    let long_ago = SessionRecord { registered_at_ms: Some(NOW_MS - INDEXER_GRACE_MS), ..record(true) };
    let cases: [(&str, &SessionRecord, String, KeyState); 4] = [
        ("listed, never registered here", &record(false), listed((NOW_MS + TTL) / 1000), KeyState::Ready),
        ("listed with an hour left", &long_ago, listed(NOW_MS / 1000 + 3_600), KeyState::Unregistered),
        ("not listed, never registered", &record(false), String::new(), KeyState::Unregistered),
        ("not listed, registered past the grace", &long_ago, String::new(), KeyState::Unregistered),
    ];
    for (name, record, keys, expected) in cases {
        assert_eq!(key_state(Some(record), Some(&account(&keys)), NOW_MS), expected, "{name}");
    }
}

#[test]
fn a_registration_the_indexer_has_not_reached_counts_as_ready() {
    // The account was indexed through `NOW_MS`.
    let cases = [
        ("registered now", NOW_MS, KeyState::Ready),
        ("a second inside the grace", NOW_MS - INDEXER_GRACE_MS + 1_000, KeyState::Ready),
        ("the whole grace before the indexer's time", NOW_MS - INDEXER_GRACE_MS, KeyState::Unregistered),
        ("ten minutes before", NOW_MS - 600_000, KeyState::Unregistered),
    ];
    for (name, at, expected) in cases {
        let record = SessionRecord { registered_at_ms: Some(at), ..record(true) };
        assert_eq!(key_state(Some(&record), Some(&account("")), NOW_MS), expected, "{name}");
    }
}
