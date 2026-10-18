//! The trading state as the relayer answered it on 2026-10-13, with BTC and ETH listed, and the
//! same answer edited where the schema, the switches or the intake do not read.

use serde_json::{Value, json};

use super::*;
use crate::test_support::TRADING;

/// The recorded trading state with `edit` made to it.
fn edited(edit: impl Fn(&mut Value)) -> String {
    let mut json: Value = serde_json::from_str(TRADING).unwrap();
    edit(&mut json);
    json.to_string()
}

#[test]
fn the_live_trading_state_reads() {
    let state = TradingState::parse(TRADING).unwrap();
    assert!(!state.trading_paused);
    assert_eq!(state.indexed_through_block, 48_399_240);
    assert_eq!(state.minimum_margin, 10_000_000_000_000_000_000, "$10");
    assert_eq!(state.minimum_open_notional, 1_000_000_000_000_000_000_000, "$1,000");
    assert_eq!(state.acc_reward_per_share, 10_508_152_974_861_028_961_272_120);
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
    assert_eq!((state.instrument(2), state.by_symbol("SOL")), (None, None));
    let intake = state.intake.as_ref().unwrap();
    assert!(intake.paused.is_empty() && state.accepts("openPosition"));
    assert_eq!(intake.revision, 1_791_841_985_919_872);
    assert_eq!(intake.paper_stake_minimum, 1_000_000_000_000_000_000_000, "1,000 PAPER");
    assert_eq!(intake.maximum_net_open_interest, 40_000_000_000_000_000_000_000_000, "$40M");
}

#[test]
fn a_paused_action_is_not_taken() {
    let text = TRADING.replace(",[]]}", ",[\"openPosition\"]]}");
    let state = TradingState::parse(&text).unwrap();
    assert!(!state.accepts("openPosition") && state.accepts("closePositions"));
}

/// Without an intake, from an older relayer, every action is taken.
#[test]
fn a_state_with_no_intake_takes_every_action() {
    for text in [
        edited(|json| json["intake"] = Value::Null),
        edited(|json| {
            json.as_object_mut().unwrap().remove("intake");
        }),
    ] {
        let state = TradingState::parse(&text).unwrap();
        assert!(state.intake.is_none() && state.accepts("openPosition") && state.accepts("paperStake"), "{text}");
    }
}

#[test]
fn a_state_that_does_not_read_names_what_failed() {
    for (edit, what) in [
        (json!({"schemaVersion": 2}), "trading_schema"),
        (json!({"schemaVersion": "one"}), "trading_schema"),
        (json!({"instruments": {}}), "trading_schema"),
        (json!({"tradingPaused": "no"}), "trading_schema"),
        (json!({"indexedThroughBlock": -1}), "trading_block"),
        (json!({"minimumTradeSizeRaw": "10.5"}), "minimum_trade_size"),
        (json!({"minimumOpenNotionalRaw": null}), "minimum_open_notional"),
        (json!({"accRewardPerShareRaw": ""}), "reward_accumulator"),
        (json!({"intake": [1, "1000", "40"]}), "trading_intake"),
        (json!({"intake": [1, "1000", "40", "openPosition"]}), "trading_intake"),
        (json!({"intake": [1, "1000", "40", [7]]}), "trading_intake_action"),
        (json!({"intake": ["r1", "1000", "40", []]}), "trading_intake_revision"),
        (json!({"intake": [1, "-1", "40", []]}), "trading_paper_stake_minimum"),
        (json!({"intake": [1, "1000", "x", []]}), "trading_net_open_interest"),
    ] {
        let text = edited(|json| {
            for (key, value) in edit.as_object().unwrap() {
                json[key] = value.clone();
            }
        });
        assert_eq!(TradingState::parse(&text).map_err(|bad| bad.what), Err(what.to_owned()), "{edit}");
    }
    let missing = edited(|json| {
        json.as_object_mut().unwrap().remove("tradingPaused");
    });
    assert_eq!(TradingState::parse(&missing).unwrap_err().what, "tradingPaused");
    assert_eq!(TradingState::parse("{").unwrap_err().what, "trading_frame");
}
