//! An open against the recorded trading state's limits, and what the queue can fund.

use super::*;
use crate::test_support::{TRADING, WAD};

#[test]
fn an_open_is_held_to_the_venues_limits() {
    let state = TradingState::parse(TRADING).unwrap();
    let btc = &state.instruments[0];
    let mark = 829_105;
    let dollars = |n: u128| n * WAD;
    assert_eq!(check_open(&state, btc, true, dollars(10), 1000, Some(mark), dollars(100), None), Ok(()));
    assert_eq!(
        check_open(&state, btc, true, dollars(9), 1000, Some(mark), dollars(100), None),
        Err(OpenRefusal::BelowMinimumMargin)
    );
    assert_eq!(
        check_open(&state, btc, true, dollars(10), 99, Some(mark), dollars(100), None),
        Err(OpenRefusal::BelowMinimumNotional)
    );
    assert_eq!(
        check_open(&state, btc, true, dollars(20_000), 1000, Some(mark), dollars(100_000), None),
        Err(OpenRefusal::AboveMaximumPosition)
    );
    assert_eq!(
        check_open(&state, btc, true, dollars(200), 10, Some(mark), dollars(100), None),
        Err(OpenRefusal::InsufficientFunds)
    );
    assert_eq!(
        check_open(&state, btc, true, dollars(50), 100, Some(mark), dollars(100), Some(dollars(20))),
        Err(OpenRefusal::QueueLiquidity)
    );
    assert_eq!(
        check_open(&state, btc, true, dollars(10), 1001, Some(mark), dollars(100), None),
        Err(OpenRefusal::LeverageTooHigh)
    );

    let limits = open_limits(&state, btc, true, 1000, Some(mark), dollars(1_000_000), false);
    let headroom = limits.headroom.unwrap();
    assert_eq!(limits.maximum, dollars(10_000).min(headroom / 1000), "$10M of notional at 1000×, or the cap's room");
    let expected = if headroom / 1000 < dollars(10_000) { Limit::OpenInterest } else { Limit::MarketMaxSize };
    assert_eq!(limits.limit, expected);
    let without_net = TradingState { intake: None, ..state.clone() };
    let uncapped = Instrument { long_cap: NO_CAP, ..btc.clone() };
    let limits = open_limits(&without_net, &uncapped, true, 1000, Some(mark), dollars(1_000_000), false);
    assert_eq!((limits.limit, limits.maximum), (Limit::MarketMaxSize, dollars(10_000)), "no cap on the side");
    let limits = open_limits(&state, btc, true, 10, Some(mark), dollars(50), false);
    assert_eq!((limits.limit, limits.maximum), (Limit::Collateral, dollars(50)));
    assert!(limits.headroom.is_some(), "the net open interest limit bounds the side");
}

#[test]
fn the_queue_funds_what_it_can() {
    assert_eq!(debt_room(100, 60, 50), 90);
    assert_eq!(debt_room(5, 60, 0), 0);
}
