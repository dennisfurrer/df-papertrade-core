//! The builders refuse what the venue would refuse, and take its edges.

use df_hyperliquid_core::utils::Address;
use df_hyperliquid_core::utils::sign::eip712::uint256;

use super::*;
use crate::client::deployment::EXCHANGE;
use crate::test_support::owner;

#[test]
fn the_builders_refuse_what_the_venue_refuses() {
    let user = owner();
    let thirteen: Vec<u128> = (0..13).collect();
    let twelve: Vec<u128> = (0..12).collect();
    let cases = [
        ("leverage 0", OpenPosition::new(user, 0, true, 1, 0, false, 1, 2).map(drop), Err(Refusal::Leverage(0))),
        (
            "leverage 1001",
            OpenPosition::new(user, 0, true, 1, 1001, false, 1, 2).map(drop),
            Err(Refusal::Leverage(1001)),
        ),
        ("no margin", OpenPosition::new(user, 0, true, 0, 10, false, 1, 2).map(drop), Err(Refusal::NoMargin)),
        ("the highest leverage", OpenPosition::new(user, 0, true, 1, MAX_LEVERAGE, false, 1, 2).map(drop), Ok(())),
        ("a close of nothing", Close::new(user, &[], 1, 2).map(drop), Err(Refusal::CloseCount(0))),
        ("a close of thirteen", Close::new(user, &thirteen, 1, 2).map(drop), Err(Refusal::CloseCount(13))),
        ("a close of twelve", Close::new(user, &twelve, 1, 2).map(drop), Ok(())),
        ("a close of one twice", Close::new(user, &[7, 8, 7], 1, 2).map(drop), Err(Refusal::DuplicatePosition)),
        (
            "under 10 USDC",
            WithdrawToCore::new(user, MIN_WITHDRAWAL_USDC - 1, 1, 2).map(drop),
            Err(Refusal::WithdrawalTooSmall),
        ),
        ("10 USDC", WithdrawToCore::new(user, MIN_WITHDRAWAL_USDC, 1, 2).map(drop), Ok(())),
        ("no PAPER", Staking::new(user, 0, 1, 2).map(drop), Err(Refusal::NoAmount)),
        ("one PAPER unit", Staking::new(user, 1, 1, 2).map(drop), Ok(())),
        (
            "the wallet as its own key",
            RegisterSessionKey::new(user, user, 10, 1, 2).map(drop),
            Err(Refusal::BadSessionKey),
        ),
        ("the zero key", RegisterSessionKey::new(user, Address::ZERO, 10, 1, 2).map(drop), Err(Refusal::BadSessionKey)),
        (
            "a key that ends at the deadline",
            RegisterSessionKey::new(user, EXCHANGE, 2, 1, 2).map(drop),
            Err(Refusal::ExpiresFirst),
        ),
        ("a key that outlives it", RegisterSessionKey::new(user, EXCHANGE, 3, 1, 2).map(drop), Ok(())),
    ];
    for (name, got, expected) in cases {
        assert_eq!(got, expected, "{name}");
    }
}

#[test]
fn a_close_names_its_positions_as_uint256_words() {
    let close = Close::new(owner(), &[408_665, u128::MAX], 1, 2).unwrap();
    assert_eq!(close.position_ids(), &[uint256(408_665), uint256(u128::MAX)]);
    let past = [[0xff; 32], uint256(1)];
    assert_eq!(Close::of_words(owner(), past.to_vec(), 1, 2).unwrap().position_ids(), &past);
}
