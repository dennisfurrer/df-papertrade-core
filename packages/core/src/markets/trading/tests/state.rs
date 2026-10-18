//! The trading state as the relayer answered it on 2026-10-13, with BTC and ETH listed.

use super::*;
use crate::test_support::TRADING;

#[test]
fn the_live_trading_state_reads() {
    let state = TradingState::parse(TRADING).unwrap();
    assert!(!state.trading_paused);
    assert_eq!(state.minimum_margin, 10_000_000_000_000_000_000, "$10");
    assert_eq!(state.minimum_open_notional, 1_000_000_000_000_000_000_000, "$1,000");
    assert_eq!(state.instruments.len(), 2);
    let btc = state.by_symbol("btc").unwrap();
    assert_eq!((btc.id, btc.asset_index, btc.price_scale, btc.max_leverage), (0, 0, 10, 1000));
    assert!(btc.openable() && !btc.terminal && btc.frozen_mid().is_none());
    assert_eq!(btc.base_rate, 100_000_000_000_000_000, "10%");
    assert_eq!(btc.rate_multiplier, 15_000_000_000_000_000_000_000);
    assert_eq!(btc.position_multiplier, 814_598_000_000_000_000_000);
    assert_eq!(btc.reference_notional, 100_000_000_000_000_000_000_000, "$100,000");
    assert_eq!(btc.max_position_notional, 10_000_000_000_000_000_000_000_000, "$10M");
    assert_eq!(state.instrument(1).unwrap().position_multiplier, 483_979_000_000_000_000_000);
    let intake = state.intake.as_ref().unwrap();
    assert!(intake.paused.is_empty() && state.accepts("openPosition"));
    assert_eq!(intake.maximum_net_open_interest, 40_000_000_000_000_000_000_000_000, "$40M");
}

#[test]
fn a_paused_action_is_not_taken() {
    let text = TRADING.replace(",[]]}", ",[\"openPosition\"]]}");
    let state = TradingState::parse(&text).unwrap();
    assert!(!state.accepts("openPosition") && state.accepts("closePositions"));
}
