//! Positions valued on BTC: the impact curve, the deadband, losses, risk and retired markets.

use super::*;
use crate::test_support::btc;
use crate::trade::math::bust_price;

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

/// Papertrade's docs: on BTC a 1% move keeps about 88% of the raw gain, a 0.1% move about
/// 74% after the win fee, never more than 90%. Exact figures cross-checked with a separate
/// big-integer port of the curve.
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
