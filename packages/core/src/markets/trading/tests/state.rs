//! The trading state as the relayer answered it on 2026-10-13, with BTC and ETH listed, as it
//! answered on 2026-11-24 with its intake controls, leverage bounds, account cap, Exchange
//! implementation and the off-hours curve on (`trading_mainnet.json`), and the same answers edited
//! where the schema, the switches or the intake do not read.

use serde_json::{Value, json};

use super::*;
use crate::test_support::TRADING;
use crate::trade::math::Curve;

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
    assert_eq!(intake.revision, 1_795_255_200_000_000);
    assert_eq!(intake.paper_stake_minimum, 1_000_000_000_000_000_000_000, "1,000 PAPER");
    assert!(intake.paused.is_empty() && state.accepts("openPosition"));
    let paused = edited_from(MAINNET, |json| json["intakeControls"][2] = json!(["openPosition"]));
    let state = TradingState::parse(&paused).unwrap();
    assert!(!state.accepts("openPosition") && state.accepts("closePositions"));
}

/// The venue-wide leverage bounds and the win fee: 0 is no bound, as mainnet sends for the
/// minimum while none is set, and an answer from before these fields reads as none and unknown.
#[test]
fn the_leverage_bounds_and_the_win_fee_read_and_zero_is_no_bound() {
    let state = TradingState::parse(MAINNET).unwrap();
    assert_eq!((state.minimum_leverage, state.maximum_leverage), (None, Some(1000)));
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

/// `accountOiCapRaw`, `exchange` and `offHours` are top-level fields, not per market: $10M of
/// notional per account and market, contract set 2, and the off-hours curve on at 14:26 UTC.
#[test]
fn the_account_cap_the_exchange_and_the_curve_read() {
    let state = TradingState::parse(MAINNET).unwrap();
    assert_eq!(state.account_oi_cap, Some(10_000_000 * 10u128.pow(18)), "$10M");
    assert_eq!(state.exchange, ExchangeVersion::Next);
    assert!(state.off_hours);
    assert_eq!(state.curve(), Some(Curve::OffHours));
    assert_eq!(state.instruments.len(), 2);

    let standard = edited_from(MAINNET, |json| {
        json.as_object_mut().unwrap().remove("offHours");
    });
    let standard = TradingState::parse(&standard).unwrap();
    assert_eq!((standard.off_hours, standard.curve()), (false, Some(Curve::Standard)), "sent only while on");
    let off = TradingState::parse(&edited_from(MAINNET, |json| json["offHours"] = json!(false))).unwrap();
    assert_eq!(off.curve(), Some(Curve::Standard));

    let uncapped = TradingState::parse(&edited_from(MAINNET, |json| json["accountOiCapRaw"] = json!("0"))).unwrap();
    assert_eq!(uncapped.account_oi_cap, None, "0 is no cap");
    let other = TradingState::parse(&edited_from(MAINNET, |json| json["exchange"] = json!("v3"))).unwrap();
    assert_eq!((other.exchange.clone(), other.curve()), (ExchangeVersion::Other("v3".to_owned()), None));

    let older = TradingState::parse(TRADING).unwrap();
    assert_eq!(
        (older.account_oi_cap, older.exchange.clone(), older.off_hours, older.curve()),
        (None, ExchangeVersion::Original, false, None),
        "an answer from contract set 1"
    );
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
        (json!({"accountOiCapRaw": "10M"}), "trading_account_oi_cap"),
        (json!({"exchange": 2}), "trading_exchange"),
        (json!({"offHours": "yes"}), "trading_off_hours"),
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
