//! The builders refuse what the venue would refuse.

use df_hyperliquid_core::utils::Address;
use df_hyperliquid_core::utils::sign::eip712::uint256;

use super::*;
use crate::client::deployment::EXCHANGE;
use crate::test_support::owner;

#[test]
fn the_builders_refuse_what_the_venue_refuses() {
    let user = owner();
    assert_eq!(OpenPosition::new(user, 0, true, 1, 0, false, 1, 2), Err(Refusal::Leverage(0)));
    assert_eq!(OpenPosition::new(user, 0, true, 1, 1001, false, 1, 2), Err(Refusal::Leverage(1001)));
    assert_eq!(OpenPosition::new(user, 0, true, 0, 10, false, 1, 2), Err(Refusal::NoMargin));
    assert!(OpenPosition::new(user, 0, true, 1, MAX_LEVERAGE, false, 1, 2).is_ok());

    assert_eq!(Close::new(user, &[], 1, 2), Err(Refusal::CloseCount(0)));
    let thirteen: Vec<u128> = (0..13).collect();
    assert_eq!(Close::new(user, &thirteen, 1, 2), Err(Refusal::CloseCount(13)));
    assert_eq!(Close::new(user, &[7, 8, 7], 1, 2), Err(Refusal::DuplicatePosition));
    let close = Close::new(user, &[408_665], 1, 2).unwrap();
    assert_eq!(close.position_ids(), &[uint256(408_665)]);

    assert_eq!(WithdrawToCore::new(user, MIN_WITHDRAWAL_USDC - 1, 1, 2), Err(Refusal::WithdrawalTooSmall));
    assert!(WithdrawToCore::new(user, MIN_WITHDRAWAL_USDC, 1, 2).is_ok());
    assert_eq!(Staking::new(user, 0, 1, 2), Err(Refusal::NoAmount));

    assert_eq!(RegisterSessionKey::new(user, user, 10, 1, 2), Err(Refusal::BadSessionKey));
    assert_eq!(RegisterSessionKey::new(user, Address::ZERO, 10, 1, 2), Err(Refusal::BadSessionKey));
    assert_eq!(RegisterSessionKey::new(user, EXCHANGE, 2, 1, 2), Err(Refusal::ExpiresFirst));
}
