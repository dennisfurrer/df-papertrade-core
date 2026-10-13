use super::*;
use crate::{
    account::Account,
    sse::{Events, WALLET},
    trading::Instrument,
    trading::TradingState,
};

const TRADING: &str = include_str!("../../fixtures/trading.json");
const STREAMS: [&str; 2] =
    [include_str!("../../fixtures/live_stream.txt"), include_str!("../../fixtures/live_queued_close.txt")];

fn btc() -> Instrument {
    TradingState::parse(TRADING).unwrap().instruments.remove(0)
}

fn position(is_long: bool, entry: u128, leverage: u32) -> Position {
    Position {
        position_id: 1,
        instrument_id: 0,
        is_long,
        use_debt: false,
        entry_price: entry,
        margin: 10_000_000_000_000_000_000,
        leverage,
        bust_price: bust_price(entry, leverage, is_long, btc().bust_buffer).unwrap(),
        opened_at_ms: 0,
        intent_id: None,
    }
}

/// Every position the indexer reported in the captures has the bust price
/// computed here from its entry, leverage and the market's buffer.
#[test]
fn every_captured_bust_price_is_ours() {
    let state = TradingState::parse(TRADING).unwrap();
    let mut checked = 0;
    for stream in STREAMS {
        for event in Events::new().push(stream).into_iter().filter(|event| event.name == WALLET) {
            let json = hl_core::Json::parse(&event.data).unwrap();
            let rows = json.get("openPositions").or_else(|| json.get("addedPositions")).unwrap();
            if rows.as_array().unwrap().is_empty() {
                continue;
            }
            let Ok(account) = Account::parse(&event.data) else {
                // A patch: read its added rows through a one-off snapshot of them.
                for row in rows.as_array().unwrap() {
                    let items = row.as_array().unwrap();
                    let field = |index: usize| {
                        items[index].as_str().map(str::to_owned).unwrap_or_else(|| items[index].compact())
                    };
                    let instrument = state.instrument(field(1).parse().unwrap()).unwrap();
                    let expected: u128 = field(7).parse().unwrap();
                    let is_long = items[2] == hl_core::Json::Bool(true);
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

/// The docs: on BTC, counting the deadband, a 1% move keeps about 88% of the
/// raw gain and a 0.1% move about 74% after the win fee; never more than 90%
/// (the base rate). The exact figures are the app's integer curve, checked
/// against a separate big-integer port of it.
#[test]
fn the_impact_curve_keeps_what_the_docs_say() {
    let instrument = btc();
    let entry = 1_000_000;
    let long = position(true, entry, 100);
    let keep = |exit: u128| {
        let valued = value(&long, &instrument, exit, WIN_FEE_RATE).unwrap();
        (valued.adjusted_pnl * 1000 / valued.raw_pnl, valued)
    };
    let (one_percent, valued) = keep(1_010_000);
    assert_eq!(one_percent, 883, "88.3% of the raw gain");
    assert_eq!(
        valued.raw_pnl,
        10_000_000_000_000_000_000 * 100 / 100 - 10_000_000_000_000_000_000 * 100 * 20 / 1_000_000
    );
    assert_eq!(valued.payout_pnl, valued.adjusted_pnl - valued.adjusted_pnl * 2 / 100, "less the 2% win fee");
    let (tenth, _) = keep(1_001_000);
    assert_eq!(tenth, 754, "75.4%, 73.9% after the fee");
    let (huge, _) = keep(2_000_000);
    assert!(huge < 900, "under 1 − base rate");
}

#[test]
fn a_win_inside_the_deadband_is_no_win() {
    let instrument = btc();
    let long = position(true, 1_000_000, 1000);
    let valued = value(&long, &instrument, 1_000_020, WIN_FEE_RATE).unwrap();
    assert_eq!((valued.raw_pnl, valued.adjusted_pnl, valued.payout_pnl), (0, 0, 0));
    let short = position(false, 1_000_000, 1000);
    assert_eq!(value(&short, &instrument, 999_980, WIN_FEE_RATE).unwrap().raw_pnl, 0);
}

#[test]
fn a_loss_is_the_move_and_never_past_the_margin() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let valued = value(&long, &instrument, 995_000, WIN_FEE_RATE).unwrap();
    assert_eq!(valued.raw_pnl, -5_000_000_000_000_000_000, "half a percent of $1,000");
    assert_eq!(valued.adjusted_pnl, valued.raw_pnl);
    assert_eq!(valued.payout_pnl, valued.raw_pnl, "no fee on a loss");
    let wiped = value(&long, &instrument, 900_000, WIN_FEE_RATE).unwrap();
    assert_eq!(wiped.adjusted_pnl, -10_000_000_000_000_000_000);
    assert!(wiped.liquidated);
    assert_eq!(wiped.risk_bp, Some(10_000));
}

/// The docs' worked example: $100 long at 100×, a 1% drop takes the margin;
/// the bust is a buffer earlier, near $99.05.
#[test]
fn the_bust_sits_a_buffer_inside_the_margin() {
    let buffer = btc().bust_buffer;
    assert_eq!(bust_price(1_000, 100, true, buffer), Some(990));
    assert_eq!(bust_price(100_000_000, 100, true, buffer), Some(99_047_128));
    assert_eq!(bust_price(100_000_000, 100, false, buffer), Some(100_951_920));
    assert_eq!(bust_price(1, 0, true, buffer), None);
}

#[test]
fn risk_runs_from_entry_to_bust() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let halfway = long.entry_price - (long.entry_price - long.bust_price) / 2;
    let valued = value(&long, &instrument, halfway, WIN_FEE_RATE).unwrap();
    let expected = (long.entry_price - halfway) * 10_000 / (long.entry_price - long.bust_price);
    assert_eq!(valued.risk_bp, Some(u32::try_from(expected).unwrap()));
    assert!((4_999..=5_000).contains(&expected));
    assert!(!valued.liquidated);
    assert_eq!(value(&long, &instrument, 1_100_000, WIN_FEE_RATE).unwrap().risk_bp, Some(0));
}

#[test]
fn a_retired_market_values_at_its_frozen_price() {
    let mut instrument = btc();
    instrument.terminal = true;
    instrument.frozen_bid = Some(1_009_990);
    instrument.frozen_ask = Some(1_010_010);
    let long = position(true, 1_000_000, 100);
    let valued = value(&long, &instrument, 1, WIN_FEE_RATE).unwrap();
    assert_eq!(valued.mark, 1_010_000);
    assert!(!valued.liquidated && valued.risk_bp.is_none());
    instrument.frozen_ask = None;
    assert_eq!(value(&long, &instrument, 1, WIN_FEE_RATE), Err(ValuationError::NoPrice));
}

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
    let uncapped = Instrument { long_cap: crate::trading::NO_CAP, ..btc.clone() };
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

/// The live curve (summary 2026-10-13): tracked LP past the $2M cliff, tail
/// progress $114.27M, so `100 · (120 / 234.27)²`, about 26.2 PAPER a dollar.
#[test]
fn the_mint_rate_follows_the_curve() {
    let rate = 100 * WAD;
    let cliff = 2_000_000 * WAD;
    let decay = 120_000_000 * WAD;
    assert_eq!(mint_rate(-5, cliff, rate, decay, 0), rate, "under water is flat");
    assert_eq!(mint_rate(1_999_999 * 10i128.pow(18), cliff, rate, decay, 0), rate);
    let live = mint_rate(4_840_851_053_562_728_341_709_751, cliff, rate, decay, 114_265_779_761_243_460_574_521_289);
    assert_eq!(live / 10u128.pow(16), 2_623, "26.23 PAPER a dollar");
    assert_eq!(paper_for_loss(2 * WAD, live), 2 * live);
}
