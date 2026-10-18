//! Bust prices against every position in the captured streams, and the docs' worked example.

use super::*;
use crate::account::state::Account;
use crate::live::{Events, WALLET};
use crate::markets::trading::TradingState;
use crate::test_support::{QUEUED, STREAM, TRADING, btc};

#[test]
fn every_captured_bust_price_is_ours() {
    let state = TradingState::parse(TRADING).unwrap();
    let mut checked = 0;
    for stream in [STREAM, QUEUED] {
        for event in Events::new().push(stream).into_iter().filter(|event| event.name == WALLET) {
            let json = df_hyperliquid_core::utils::Json::parse(&event.data).unwrap();
            let rows = json.get("openPositions").or_else(|| json.get("addedPositions")).unwrap();
            if rows.as_array().unwrap().is_empty() {
                continue;
            }
            let Ok(account) = Account::parse(&event.data) else {
                // A patch, which `Account::parse` refuses: check its added rows directly.
                for row in rows.as_array().unwrap() {
                    let items = row.as_array().unwrap();
                    let field = |index: usize| {
                        items[index].as_str().map(str::to_owned).unwrap_or_else(|| items[index].compact())
                    };
                    let instrument = state.instrument(field(1).parse().unwrap()).unwrap();
                    let expected: u128 = field(7).parse().unwrap();
                    let is_long = items[2] == df_hyperliquid_core::utils::Json::Bool(true);
                    let ours = bust_price(
                        field(4).parse().unwrap(),
                        field(6).parse().unwrap(),
                        is_long,
                        instrument.bust_buffer,
                    );
                    assert_eq!(ours, Some(expected), "position {}", field(0));
                    checked += 1;
                }
                continue;
            };
            for position in &account.positions {
                let instrument = state.instrument(position.instrument_id).unwrap();
                let ours =
                    bust_price(position.entry_price, position.leverage, position.is_long, instrument.bust_buffer);
                assert_eq!(ours, Some(position.bust_price), "position {}", position.position_id);
                checked += 1;
            }
        }
    }
    assert!(checked >= 10, "checked {checked}");
}

/// Papertrade's docs example: a $100 long at 100x loses its margin on a 1% drop, and busts a
/// buffer earlier, near $99.05. A short busts as far above.
#[test]
fn the_bust_sits_a_buffer_inside_the_margin() {
    let buffer = btc().bust_buffer;
    // (entry, leverage, long, bust)
    let cases = [
        (1_000, 100, true, Some(990)),
        (100_000_000, 100, true, Some(99_047_128)),
        (100_000_000, 100, false, Some(100_951_920)),
        (100_000_000, 1, true, Some(0)),
        (1, 0, true, None),
        (u128::MAX, 2, false, None),
    ];
    for (entry, leverage, is_long, expected) in cases {
        assert_eq!(bust_price(entry, leverage, is_long, buffer), expected, "{entry} at {leverage}x, long: {is_long}");
    }
}
