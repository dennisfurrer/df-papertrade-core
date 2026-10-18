//! Instrument rows edited from the recorded trading state: a retired market, and rows whose
//! lifecycle, scale, length or order does not add up.

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

/// BTC retired: paused, terminal, retired and close-only, with its BBO and time frozen.
fn retire(row: &mut [Json]) {
    for index in [5, 6, 7, 8] {
        row[index] = Json::Bool(true);
    }
    row[9] = Json::String("829100".to_owned());
    row[10] = Json::String("829110".to_owned());
    row[11] = Json::Number("1791860000000".to_owned());
}

#[test]
fn a_retired_market_reads_with_its_frozen_price() {
    let btc = TradingState::parse(&with_btc(|row| retire(row))).unwrap().instruments.remove(0);
    assert!(btc.terminal && btc.retired && btc.close_only && !btc.openable());
    assert_eq!(
        (btc.frozen_bid, btc.frozen_ask, btc.paused_at),
        (Some(829_100), Some(829_110), Some(1_791_860_000_000))
    );
    assert_eq!(btc.frozen_mid(), Some(829_105));
}

#[test]
fn a_row_that_does_not_add_up_is_refused() {
    type Edit = fn(&mut Vec<Json>);
    let rows: [(Edit, &str); 12] = [
        (|row| row[6] = Json::Bool(true), "trading_lifecycle"),
        (|row| row[5] = Json::Bool(true), "trading_lifecycle"),
        (|row| row[8] = Json::Bool(true), "trading_lifecycle"),
        (|row| row[4] = Json::Bool(false), "trading_lifecycle"),
        (|row| row[9] = Json::String("829100".to_owned()), "trading_lifecycle"),
        (
            |row| {
                retire(row);
                row[11] = Json::Null;
            },
            "trading_lifecycle",
        ),
        (|row| row[3] = Json::String("20".to_owned()), "trading_lifecycle"),
        (|row| row[3] = Json::String("0".to_owned()), "trading_lifecycle"),
        (|row| row[0] = Json::Number("1".to_owned()), "trading_instrument_order"),
        (|row| row[2] = Json::Number("1".to_owned()), "trading_instrument_order"),
        (|row| row[1] = Json::String("ETH".to_owned()), "trading_instrument_order"),
        (
            |row| {
                row.pop();
            },
            "trading_instrument_tuple",
        ),
    ];
    for (edit, what) in rows {
        let text = with_btc(edit);
        assert_eq!(TradingState::parse(&text).map_err(|bad| bad.what), Err(what.to_owned()), "{text}");
    }
}
