use super::*;

const TRADING: &str = include_str!("../../fixtures/trading.json");

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

fn with_btc(edit: impl Fn(&mut Vec<hl_core::Json>)) -> String {
    let mut json = hl_core::Json::parse(TRADING).unwrap();
    let hl_core::Json::Object(fields) = &mut json else { panic!() };
    let (_, hl_core::Json::Array(rows)) = fields.iter_mut().find(|(key, _)| key == "instruments").unwrap() else {
        panic!()
    };
    let hl_core::Json::Array(btc) = &mut rows[0] else { panic!() };
    edit(btc);
    json.compact()
}

/// A paused market is terminal and close-only with its frozen BBO and time.
#[test]
fn a_retired_market_reads_with_its_frozen_price() {
    let text = with_btc(|row| {
        for index in [5, 6, 7, 8] {
            row[index] = hl_core::Json::Bool(true);
        }
        row[9] = hl_core::Json::String("829100".to_owned());
        row[10] = hl_core::Json::String("829110".to_owned());
        row[11] = hl_core::Json::Number("1791860000000".to_owned());
    });
    let btc = TradingState::parse(&text).unwrap().instruments.remove(0);
    assert!(btc.terminal && !btc.openable());
    assert_eq!(btc.frozen_mid(), Some(829_105));
}

#[test]
fn an_inconsistent_lifecycle_is_refused() {
    let text = with_btc(|row| row[6] = hl_core::Json::Bool(true));
    assert_eq!(TradingState::parse(&text).unwrap_err().what, "trading_lifecycle");
    let text = with_btc(|row| row[0] = hl_core::Json::Number("1".to_owned()));
    assert_eq!(TradingState::parse(&text).unwrap_err().what, "trading_instrument_order");
}

#[test]
fn a_paused_action_is_not_taken() {
    let text = TRADING.replace(",[]]}", ",[\"openPosition\"]]}");
    let state = TradingState::parse(&text).unwrap();
    assert!(!state.accepts("openPosition") && state.accepts("closePositions"));
}
