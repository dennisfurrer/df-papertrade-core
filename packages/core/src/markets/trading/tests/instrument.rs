//! Instrument rows edited from the recorded trading state: a retired market, and rows whose
//! lifecycle or order does not add up.

use super::*;
use crate::markets::trading::TradingState;
use crate::test_support::TRADING;

/// The recorded trading state with BTC's row edited.
fn with_btc(edit: impl Fn(&mut Vec<Json>)) -> String {
    let mut json = Json::parse(TRADING).unwrap();
    let Json::Object(fields) = &mut json else { panic!() };
    let (_, Json::Array(rows)) = fields.iter_mut().find(|(key, _)| key == "instruments").unwrap() else { panic!() };
    let Json::Array(btc) = &mut rows[0] else { panic!() };
    edit(btc);
    json.compact()
}

/// A paused market must also be terminal, retired and close-only, with a frozen BBO and time.
#[test]
fn a_retired_market_reads_with_its_frozen_price() {
    let text = with_btc(|row| {
        for index in [5, 6, 7, 8] {
            row[index] = Json::Bool(true);
        }
        row[9] = Json::String("829100".to_owned());
        row[10] = Json::String("829110".to_owned());
        row[11] = Json::Number("1791860000000".to_owned());
    });
    let btc = TradingState::parse(&text).unwrap().instruments.remove(0);
    assert!(btc.terminal && !btc.openable());
    assert_eq!(btc.frozen_mid(), Some(829_105));
}

#[test]
fn an_inconsistent_lifecycle_is_refused() {
    let text = with_btc(|row| row[6] = Json::Bool(true));
    assert_eq!(TradingState::parse(&text).unwrap_err().what, "trading_lifecycle");
    let text = with_btc(|row| row[0] = Json::Number("1".to_owned()));
    assert_eq!(TradingState::parse(&text).unwrap_err().what, "trading_instrument_order");
}
