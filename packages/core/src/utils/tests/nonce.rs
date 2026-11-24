//! First nonces from three random bytes, lanes that count up, and which lane each intent spends.

use df_hyperliquid_core::utils::Address;

use super::*;
use crate::trade::intent::{
    CancelIntent, Claim, Close, Intent, OpenPosition, RegisterSessionKey, Staking, WithdrawToCore,
};

#[test]
fn a_lane_starts_where_the_app_starts_it() {
    for (random, nonce) in [
        ([0, 0, 0], 256),
        ([0, 0, 1], 256),
        ([0, 0, 2], 512),
        ([0x30, 0x62, 0x01], 0x30_6201 * 256),
        ([0xff, 0xff, 0xff], 0xff_ffff * 256),
    ] {
        assert_eq!(first(random), nonce, "{random:?}");
    }
    let seeded = Nonces::seeded([0, 0, 2], [7, 7, 7]);
    assert_eq!((seeded.peek(Lane::Exchange), seeded.peek(Lane::Staking)), (512, 0x07_0707 * 256));
}

#[test]
fn each_lane_counts_up_on_its_own() {
    let mut nonces = Nonces::resume(3_170_816, 77);
    assert_eq!(nonces.take(Lane::Exchange), 3_170_816);
    assert_eq!(nonces.take(Lane::Exchange), 3_170_817);
    assert_eq!(nonces.peek(Lane::Exchange), 3_170_818);
    assert_eq!(nonces.take(Lane::Staking), 77);
    assert_eq!(nonces.peek(Lane::Staking), 78);
    let mut last = Nonces::resume(u128::MAX, 0);
    assert_eq!((last.take(Lane::Exchange), last.peek(Lane::Exchange)), (u128::MAX, u128::MAX), "stops at the top");
}

#[test]
fn each_intent_spends_its_contract_s_lane_and_a_cancel_none() {
    let user = Address::ZERO;
    let key: Address = "0x2c7536e3605d9c16a7a3d7b1898e529396a65c23".parse().unwrap();
    let staking = Staking { user, amount: 1, nonce: 1, deadline: 2 };
    for (intent, lane) in [
        (Intent::RegisterSessionKey(RegisterSessionKey::new(user, key, 3, 1, 2).unwrap()), Some(Lane::Exchange)),
        (
            Intent::OpenPosition(OpenPosition {
                user,
                instrument_id: 0,
                is_long: true,
                size: 1,
                leverage: 1,
                use_debt: false,
                nonce: 1,
                deadline: 2,
            }),
            Some(Lane::Exchange),
        ),
        (Intent::Close(Close::new(user, &[1], 1, 2).unwrap()), Some(Lane::Exchange)),
        (Intent::WithdrawToCore(WithdrawToCore { user, amount: 1, nonce: 1, deadline: 2 }), Some(Lane::Exchange)),
        (Intent::Stake(staking.clone()), Some(Lane::Staking)),
        (Intent::Unstake(staking), Some(Lane::Staking)),
        (Intent::Claim(Claim { user, nonce: 1, deadline: 2 }), Some(Lane::Staking)),
        (Intent::CancelIntent(CancelIntent { user, intent_id: [0; 32], deadline: 2 }), None),
    ] {
        assert_eq!(Lane::of(&intent), lane, "{}", intent.primary_type());
    }
}

/// A nonce the relayer has seen at or past the lane's next moves it to the one after, within
/// 65,536; one behind the lane, or further ahead, leaves it.
#[test]
fn a_lane_catches_up_with_the_nonces_seen_elsewhere() {
    // (next, recent, next after)
    for (next, recent, after) in [
        (100, 100, Some(101)),
        (100, 150, Some(151)),
        (100, 100 + CATCH_UP_WINDOW, Some(101 + CATCH_UP_WINDOW)),
        (100, 101 + CATCH_UP_WINDOW, None),
        (100, 99, None),
        (u128::MAX, u128::MAX, None),
    ] {
        assert_eq!(caught_up(next, recent), after, "{next} {recent}");
    }
    let mut nonces = Nonces::resume(182_791_800, 609_122_900);
    assert!(nonces.catch_up(Lane::Exchange, 182_791_869));
    assert!(!nonces.catch_up(Lane::Staking, 609_122_819));
    assert_eq!((nonces.take(Lane::Exchange), nonces.peek(Lane::Staking)), (182_791_870, 609_122_900));
}
