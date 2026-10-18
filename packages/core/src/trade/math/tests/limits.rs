//! An open against the recorded trading state: refused by the first check it fails, the most a
//! new position may commit and what limits it, and what the queue can fund.

use super::*;
use crate::markets::trading::Intake;
use crate::test_support::{TRADING, WAD};

/// BTC at $82,910.50, raw.
const MARK: u128 = 829_105;
/// What the recorded BTC long side has left under its cap at [`MARK`], in collateral units.
const LONG_HEADROOM: u128 = 4_388_507_917_257_093_181_991_861;

const fn dollars(n: u128) -> u128 {
    n * WAD
}

#[test]
fn an_open_is_refused_by_the_first_check_it_fails() {
    let state = TradingState::parse(TRADING).unwrap();
    let btc = &state.instruments[0];
    let paused = TradingState { trading_paused: true, ..state.clone() };
    let paused_opens = Intake { revision: 1, paper_stake_minimum: 0, paused: vec!["openPosition".to_owned()] };
    let no_opens = TradingState { intake: Some(paused_opens), ..state.clone() };
    let closed = Instrument { paused: true, ..btc.clone() };
    // (name, state, instrument, margin in dollars, leverage, mark, available, queue room, result)
    let cases = [
        ("within every limit", &state, btc, 10, 1000, Some(MARK), 100, None, Ok(())),
        ("trading paused", &paused, btc, 10, 1000, Some(MARK), 100, None, Err(OpenRefusal::TradingPaused)),
        ("a paused market", &state, &closed, 10, 1000, Some(MARK), 100, None, Err(OpenRefusal::MarketClosed)),
        ("opens paused at intake", &no_opens, btc, 10, 1000, Some(MARK), 100, None, Err(OpenRefusal::IntakePaused)),
        ("no leverage", &state, btc, 10, 0, Some(MARK), 100, None, Err(OpenRefusal::LeverageTooHigh)),
        ("past the market's leverage", &state, btc, 10, 1001, Some(MARK), 100, None, Err(OpenRefusal::LeverageTooHigh)),
        ("under the minimum margin", &state, btc, 9, 1000, Some(MARK), 100, None, Err(OpenRefusal::BelowMinimumMargin)),
        (
            "past the largest position",
            &state,
            btc,
            20_000,
            1000,
            Some(MARK),
            100_000,
            None,
            Err(OpenRefusal::AboveMaximumPosition),
        ),
        (
            "under the smallest position",
            &state,
            btc,
            10,
            99,
            Some(MARK),
            100,
            None,
            Err(OpenRefusal::BelowMinimumNotional),
        ),
        ("more than is available", &state, btc, 200, 10, Some(MARK), 100, None, Err(OpenRefusal::InsufficientFunds)),
        (
            "more than the queue funds",
            &state,
            btc,
            50,
            100,
            Some(MARK),
            100,
            Some(20),
            Err(OpenRefusal::QueueLiquidity),
        ),
        ("past the side's room", &state, btc, 5_000, 1000, Some(MARK), 100_000, None, Err(OpenRefusal::OpenInterest)),
        ("just inside the side's room", &state, btc, 4_388, 1000, Some(MARK), 100_000, None, Ok(())),
        ("with no mark, no room to check", &state, btc, 5_000, 1000, None, 100_000, None, Ok(())),
    ];
    for (name, state, instrument, margin, leverage, mark, available, room, expected) in cases {
        let got =
            check_open(state, instrument, true, dollars(margin), leverage, mark, dollars(available), room.map(dollars));
        assert_eq!(got, expected, "{name}");
    }
}

#[test]
fn a_new_position_may_commit_up_to_its_tightest_limit() {
    let state = TradingState::parse(TRADING).unwrap();
    let btc = &state.instruments[0];
    let uncapped = Instrument { long_cap: NO_CAP, ..btc.clone() };
    let plenty = dollars(1_000_000);
    let (open_interest, market, collateral, queue) =
        (Limit::OpenInterest, Limit::MarketMaxSize, Limit::Collateral, Limit::QueueLiquidity);
    let capped = Some(LONG_HEADROOM);
    // (name, instrument, leverage, mark, available, queue limited, limit, maximum, headroom)
    let cases = [
        ("the side's cap", btc, 1000, Some(MARK), plenty, false, open_interest, LONG_HEADROOM / 1000, capped),
        ("no cap on the side", &uncapped, 1000, Some(MARK), plenty, false, market, dollars(10_000), None),
        ("no mark", btc, 1000, None, plenty, false, market, dollars(10_000), None),
        ("a zero mark", btc, 1000, Some(0), plenty, false, market, dollars(10_000), None),
        ("the balance", btc, 10, Some(MARK), dollars(50), false, collateral, dollars(50), capped),
        ("the queue", btc, 10, Some(MARK), dollars(50), true, queue, dollars(50), capped),
        ("past the market's leverage", btc, 1001, Some(MARK), plenty, false, Limit::Leverage, 0, None),
    ];
    for (name, instrument, leverage, mark, available, queued, limit, maximum, headroom) in cases {
        let got = open_limits(instrument, true, leverage, mark, available, queued);
        assert_eq!(got, OpenLimits { maximum, limit, headroom }, "{name}");
    }
}

#[test]
fn the_queue_funds_its_consumable_debt_less_what_others_reserve() {
    // (consumable debt, queued, available queued, room)
    for (debt, queued, available, room) in [(100, 60, 50, 90), (100, 60, 60, 100), (5, 60, 0, 0), (0, 0, 0, 0)] {
        assert_eq!(debt_room(debt, queued, available), room, "{debt} {queued} {available}");
    }
}
