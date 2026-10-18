//! A held key against the stream's registered keys, the clock and the indexer's time.

use super::*;
use crate::account::session_key::SESSION_TTL_MS;
use crate::test_support::{NOW_MS, session_address};

#[test]
fn a_key_the_stream_does_not_list_is_unregistered() {
    let other = (crate::test_support::owner(), u64::MAX);
    for registered in [&[][..], &[other][..]] {
        assert_eq!(status(session_address(), NOW_MS + SESSION_TTL_MS, registered, 0, NOW_MS), Status::Unregistered);
    }
}

#[test]
fn a_listed_key_signs_until_an_intents_life_before_the_earlier_of_its_two_ends() {
    let key = session_address();
    let held = NOW_MS + SESSION_TTL_MS;
    let late = held - RENEW_BEFORE_MS;
    let short = NOW_MS / 1000 + 7_200;
    // (name, registered expiry, indexed seconds, now, status)
    let cases = [
        ("now", held / 1000, 0, NOW_MS, Status::Ready { expires_at: held / 1000 }),
        ("a millisecond before the renewal", held / 1000, 0, late - 1, Status::Ready { expires_at: held / 1000 }),
        ("at the renewal", held / 1000, 0, late, Status::Expiring { expires_at: held / 1000 }),
        (
            "the indexer ahead of the clock",
            held / 1000,
            late / 1000,
            NOW_MS,
            Status::Expiring { expires_at: held / 1000 },
        ),
        ("registered for two hours", short, 0, NOW_MS, Status::Ready { expires_at: short }),
        ("registered past the hold", u64::MAX, 0, NOW_MS, Status::Ready { expires_at: held / 1000 }),
    ];
    for (name, expiry, indexed, now, expected) in cases {
        assert_eq!(status(key, held, &[(key, expiry)], indexed, now), expected, "{name}");
    }
}
