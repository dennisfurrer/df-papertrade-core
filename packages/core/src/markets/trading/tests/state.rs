//! The trading state as the relayer answered it on 2026-10-13, with BTC and ETH listed, its intake
//! controls as answered on 2026-10-18 (`trading_mainnet.json`), and the same answers edited where
//! the schema, the switches or the intake do not read.

use serde_json::{Value, json};

use super::*;
use crate::test_support::TRADING;

const MAINNET: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/trading_mainnet.json"));

/// The recorded trading state with `edit` made to it.
fn edited(edit: impl Fn(&mut Value)) -> String {
    edited_from(TRADING, edit)
}

fn edited_from(text: &str, edit: impl Fn(&mut Value)) -> String {
    let mut json: Value = serde_json::from_str(text).unwrap();
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
}

/// `intakeControls`: the revision, the least PAPER a stake moves, and the actions paused.
#[test]
fn the_intake_controls_say_which_actions_are_taken() {
    let state = TradingState::parse(MAINNET).unwrap();
    let intake = state.intake.as_ref().unwrap();
    assert_eq!(intake.revision, 1_792_181_740_343_271);
    assert_eq!(intake.paper_stake_minimum, 1_000_000_000_000_000_000_000, "1,000 PAPER");
    assert!(intake.paused.is_empty() && state.accepts("openPosition"));
    let paused = edited_from(MAINNET, |json| json["intakeControls"][2] = json!(["openPosition"]));
    let state = TradingState::parse(&paused).unwrap();
    assert!(!state.accepts("openPosition") && state.accepts("closePositions"));
}

/// The venue-wide leverage bounds and the win fee: 0 is no bound, as mainnet sends while none is
/// set, and an answer from before these fields reads as none and unknown.
#[test]
fn the_leverage_bounds_and_the_win_fee_read_and_zero_is_no_bound() {
    let state = TradingState::parse(MAINNET).unwrap();
    assert_eq!((state.minimum_leverage, state.maximum_leverage), (None, None));
    assert_eq!(state.win_fee_rate, Some(20_000_000_000_000_000), "2%");
    let btc = state.by_symbol("BTC").unwrap();
    assert_eq!(state.leverage_range(btc), (1, 1000), "the market's own cap alone");
    let bounded = edited_from(MAINNET, |json| {
        json["minimumLeverage"] = json!(5);
        json["maximumLeverage"] = json!(500);
    });
    let state = TradingState::parse(&bounded).unwrap();
    assert_eq!((state.minimum_leverage, state.maximum_leverage), (Some(5), Some(500)));
    assert_eq!(state.leverage_range(state.by_symbol("BTC").unwrap()), (5, 500));
    let older = TradingState::parse(TRADING).unwrap();
    assert_eq!((older.minimum_leverage, older.maximum_leverage, older.win_fee_rate), (None, None, None));
}

/// Without intake controls every action is taken, as before the relayer sent any.
#[test]
fn a_state_with_no_intake_takes_every_action() {
    for text in [
        TRADING.to_owned(),
        edited_from(MAINNET, |json| json["intakeControls"] = Value::Null),
        edited_from(MAINNET, |json| {
            json.as_object_mut().unwrap().remove("intakeControls");
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
        (json!({"intakeControls": [1, "1000"]}), "trading_intake"),
        (json!({"intakeControls": [1, "1000", "openPosition"]}), "trading_intake"),
        (json!({"intakeControls": [1, "1000", [7]]}), "trading_intake_action"),
        (json!({"intakeControls": ["r1", "1000", []]}), "trading_intake_revision"),
        (json!({"intakeControls": [1, "-1", []]}), "trading_paper_stake_minimum"),
        (json!({"maximumLeverage": "lots"}), "trading_maximum_leverage"),
        (json!({"minimumLeverage": -1}), "trading_minimum_leverage"),
        (json!({"winFeeRateRaw": "2%"}), "trading_win_fee_rate"),
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
