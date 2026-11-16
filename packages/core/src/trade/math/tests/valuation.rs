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
        let valued = value(&long, &instrument, exit, Rates::default()).unwrap();
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
    assert!(huge < 900, "under one less the base rate");
}

#[test]
fn a_win_inside_the_deadband_is_no_win() {
    let instrument = btc();
    let long = position(true, 1_000_000, 1000);
    let valued = value(&long, &instrument, 1_000_020, Rates::default()).unwrap();
    assert_eq!((valued.raw_pnl, valued.adjusted_pnl, valued.payout_pnl), (0, 0, 0));
    let short = position(false, 1_000_000, 1000);
    assert_eq!(value(&short, &instrument, 999_980, Rates::default()).unwrap().raw_pnl, 0);
}

#[test]
fn a_loss_is_the_move_and_never_past_the_margin() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let valued = value(&long, &instrument, 995_000, Rates::default()).unwrap();
    assert_eq!(valued.raw_pnl, -5_000_000_000_000_000_000, "half a percent of $1,000");
    assert_eq!(valued.adjusted_pnl, valued.raw_pnl);
    assert_eq!(valued.payout_pnl, valued.raw_pnl, "no fee on a loss");
    let wiped = value(&long, &instrument, 900_000, Rates::default()).unwrap();
    assert_eq!(wiped.adjusted_pnl, -10_000_000_000_000_000_000);
    assert!(wiped.liquidated);
    assert_eq!(wiped.risk_bp, Some(10_000));
}

#[test]
fn risk_runs_from_entry_to_bust() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let halfway = long.entry_price - (long.entry_price - long.bust_price) / 2;
    let valued = value(&long, &instrument, halfway, Rates::default()).unwrap();
    let expected = (long.entry_price - halfway) * 10_000 / (long.entry_price - long.bust_price);
    assert_eq!(valued.risk_bp, Some(u32::try_from(expected).unwrap()));
    assert!((4_999..=5_000).contains(&expected));
    assert!(!valued.liquidated);
    assert_eq!(value(&long, &instrument, 1_100_000, Rates::default()).unwrap().risk_bp, Some(0));
}

#[test]
fn a_retired_market_values_at_its_frozen_price() {
    let mut instrument = btc();
    instrument.terminal = true;
    instrument.frozen_bid = Some(1_009_990);
    instrument.frozen_ask = Some(1_010_010);
    let long = position(true, 1_000_000, 100);
    let valued = value(&long, &instrument, 1, Rates::default()).unwrap();
    assert_eq!(valued.mark, 1_010_000);
    assert!(!valued.liquidated && valued.risk_bp.is_none());
    instrument.frozen_ask = None;
    assert_eq!(value(&long, &instrument, 1, Rates::default()), Err(ValuationError::NoPrice));
}

#[test]
fn a_position_that_cannot_be_valued_is_refused() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let cases = [
        (
            "on another market",
            Position { instrument_id: 1, ..long.clone() },
            1_000_000,
            ValuationError::WrongInstrument,
        ),
        ("a mark past 64 bits", long.clone(), u128::from(u64::MAX) + 1, ValuationError::OutOfRange),
        ("an entry of zero", Position { entry_price: 0, ..long.clone() }, 1_000_000, ValuationError::OutOfRange),
        ("a notional past 128 bits", Position { margin: u128::MAX, ..long }, 1_010_000, ValuationError::OutOfRange),
    ];
    for (name, position, mark, expected) in cases {
        assert_eq!(value(&position, &instrument, mark, Rates::default()), Err(expected), "{name}");
    }
}

/// The docs: the standard curve holds back at most about 1.2 bps, at a 3.4 bps move; the
/// off-hours one about 2 bps at 4.2 bps; nothing outside their bands.
#[test]
fn the_payout_curves_hold_back_what_the_docs_say() {
    let bp = WAD / 10_000;
    let peak = |curve: Curve| (1..1_200).map(|i| (i, curve.holdback(i * bp / 100))).max_by_key(|(_, h)| *h).unwrap();
    let (at, most) = peak(Curve::Standard);
    assert!((337..=338).contains(&at), "the standard peak at {at} hundredths of a bp");
    assert_eq!(most * 100 / bp, 118, "1.18 bps");
    let (at, most) = peak(Curve::OffHours);
    assert!((417..=419).contains(&at), "the off-hours peak at {at} hundredths of a bp");
    assert_eq!(most * 100 / bp, 203, "2.03 bps, rounded down");
    for curve in [Curve::Standard, Curve::OffHours] {
        assert_eq!(curve.holdback(bp / 10), 0);
        assert_eq!(curve.holdback(20 * bp), 0);
    }
    assert_eq!(Curve::Standard.holdback(bp / 2), 0, "below the standard band");
    assert!(Curve::OffHours.holdback(bp / 2) > 0, "inside the off-hours band");
}

/// A bigger move always pays more, through the holdback band too.
#[test]
fn a_bigger_move_pays_more_on_either_curve() {
    let instrument = btc();
    let long = position(true, 100_000_000, 1000);
    for curve in [Curve::Standard, Curve::OffHours] {
        let rates = Rates { curve, ..Rates::default() };
        let pays = |ticks: u128| value(&long, &instrument, 100_000_000 + ticks, rates).unwrap().payout_pnl;
        let mut last = 0;
        for ticks in (0..=150).step_by(3) {
            let now = pays(ticks);
            assert!(now >= last, "{curve:?} at {ticks}");
            last = now;
        }
    }
    let standard = value(&long, &instrument, 100_034_000, Rates::default()).unwrap();
    let off = value(&long, &instrument, 100_034_000, Rates { curve: Curve::OffHours, ..Rates::default() }).unwrap();
    assert!(off.payout_pnl < standard.payout_pnl, "off hours holds back more at 3.4 bps");
}

/// A win through a builder pays it 0.5% beside the 2% win fee; a loss pays it nothing.
#[test]
fn a_builder_takes_its_fee_from_a_win() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let rates = Rates { builder_fee: BUILDER_FEE_RATE, ..Rates::default() };
    let win = value(&long, &instrument, 1_010_000, rates).unwrap();
    let gain = win.adjusted_pnl;
    assert_eq!(win.builder_fee, gain.unsigned_abs() / 200);
    assert_eq!(win.payout_pnl, gain - gain * 2 / 100 - gain / 200, "97.5% of the scaled gain");
    let loss = value(&long, &instrument, 995_000, rates).unwrap();
    assert_eq!((loss.builder_fee, loss.payout_pnl), (0, loss.raw_pnl));
}

/// A close at the bust price, a touch before the margin is gone, settles as a liquidation.
#[test]
fn a_close_at_bust_loses_the_whole_margin() {
    let instrument = btc();
    let long = position(true, 1_000_000, 100);
    let rates = Rates { builder_fee: BUILDER_FEE_RATE, ..Rates::default() };
    let at_bust = value(&long, &instrument, long.bust_price, rates).unwrap();
    assert!(at_bust.liquidated && at_bust.raw_pnl > -10_000_000_000_000_000_000);
    assert_eq!((at_bust.adjusted_pnl, at_bust.payout_pnl), (-10_000_000_000_000_000_000, -10_000_000_000_000_000_000));
    assert_eq!(at_bust.builder_fee, 0);
    let before = value(&long, &instrument, long.bust_price + 1, rates).unwrap();
    assert!(!before.liquidated && before.payout_pnl > -10_000_000_000_000_000_000);
}
