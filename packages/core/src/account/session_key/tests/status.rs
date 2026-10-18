//! A held key against the stream's registered keys, the clock and the indexer's time.

use super::*;
use crate::account::session_key::SESSION_TTL_MS;
use crate::test_support::{NOW_MS, session_address};

#[test]
fn the_status_reads_the_stream_and_the_clock() {
    let key = session_address();
    let held = NOW_MS + SESSION_TTL_MS;
    let registered = [(key, held / 1000)];
    assert_eq!(status(key, held, &[], 0, NOW_MS), Status::Unregistered);
    assert_eq!(status(key, held, &registered, 0, NOW_MS), Status::Ready { expires_at: held / 1000 });
    let late = held - RENEW_BEFORE_MS;
    assert_eq!(status(key, held, &registered, 0, late), Status::Expiring { expires_at: held / 1000 });
    assert_eq!(
        status(key, held, &registered, late / 1000, NOW_MS),
        Status::Expiring { expires_at: held / 1000 },
        "the indexer's time counts when it is ahead of the clock"
    );
    let shorter = [(key, NOW_MS / 1000 + 7_200)];
    assert_eq!(status(key, held, &shorter, 0, NOW_MS), Status::Ready { expires_at: NOW_MS / 1000 + 7_200 });
}
