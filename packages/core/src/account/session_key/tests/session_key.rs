//! A key's 30-day life and the registration that covers it.

use super::*;
use crate::test_support::{NOW_MS, owner, session_address};
use df_hyperliquid_core::utils::Address;

#[test]
fn a_key_lasts_thirty_days() {
    assert_eq!(expiry(NOW_MS), (NOW_MS + 30 * 24 * 3_600_000) / 1000);
}

#[test]
fn a_registration_runs_until_the_key_does() {
    let held = NOW_MS + SESSION_TTL_MS;
    let registration = registration(owner(), session_address(), held, 3_170_816, NOW_MS, Address::ZERO).unwrap();
    assert_eq!(registration.expiry, held / 1000);
    assert_eq!(registration.deadline, NOW_MS / 1000 + 3_600);
    assert_eq!(registration.fee_address, Address::ZERO);
}
