use hl_core::Address;

use super::*;
use crate::intent::{CancelIntent, Claim, Intent};

#[test]
fn a_lane_starts_where_the_app_starts_it() {
    assert_eq!(first([0, 0, 0]), 256, "never zero");
    assert_eq!(first([0, 0, 1]), 256);
    assert_eq!(first([0x30, 0x62, 0x01]), 0x30_6201 * 256);
    assert_eq!(first([0xff, 0xff, 0xff]), 0xff_ffff * 256);
}

#[test]
fn each_lane_counts_up_on_its_own() {
    let mut nonces = Nonces::resume(3_170_816, 77);
    assert_eq!(nonces.take(Lane::Exchange), 3_170_816);
    assert_eq!(nonces.take(Lane::Exchange), 3_170_817);
    assert_eq!(nonces.peek(Lane::Exchange), 3_170_818);
    assert_eq!(nonces.take(Lane::Staking), 77);
    assert_eq!(nonces.peek(Lane::Staking), 78);
}

#[test]
fn staking_spends_its_own_lane_and_a_cancel_none() {
    let claim = Intent::Claim(Claim { user: Address::ZERO, nonce: 1, deadline: 2 });
    assert_eq!(Lane::of(&claim), Some(Lane::Staking));
    let cancel = Intent::CancelIntent(CancelIntent { user: Address::ZERO, intent_id: [0; 32], deadline: 2 });
    assert_eq!(Lane::of(&cancel), None);
}
