//! The mint curve: flat below the cliff, decayed on the tail as the live figures show, and the
//! PAPER a loss mints at a rate.

use super::*;

/// Live figures from the 2026-10-13 summary: tracked LP past the $2M cliff and tail progress
/// $114.27M, so `100 * (120 / 234.27)^2`, about 26.2 PAPER a dollar.
#[test]
fn the_mint_rate_is_flat_below_the_cliff_and_decays_past_it() {
    let rate = 100 * WAD;
    let cliff = 2_000_000 * WAD;
    let decay = 120_000_000 * WAD;
    let tracked = 4_840_851_053_562_728_341_709_751;
    let tail = 114_265_779_761_243_460_574_521_289;
    // (name, tracked LP, decay scale, tail progress, rate)
    let cases = [
        ("under water", -5, decay, 0, rate),
        ("a unit under the cliff", 1_999_999 * 10i128.pow(18), decay, 0, rate),
        ("at the cliff, no tail yet", 2_000_000 * 10i128.pow(18), decay, 0, rate),
        ("the live figures", tracked, decay, tail, 26_238_849_242_755_169_239),
        ("a tail past any reach", tracked, decay, u128::MAX, 0),
        ("no decay scale and no tail", tracked, 0, 0, 0),
    ];
    for (name, tracked, decay, tail, expected) in cases {
        assert_eq!(mint_rate(tracked, cliff, rate, decay, tail), expected, "{name}");
    }
}

#[test]
fn a_loss_mints_its_dollars_times_the_rate() {
    let rate = 26_238_849_242_755_169_239;
    for (loss, expected) in [(2 * WAD, 2 * rate), (WAD / 2, rate / 2), (0, 0), (u128::MAX, 0)] {
        assert_eq!(paper_for_loss(loss, rate), expected, "{loss}");
    }
}

/// Contract set 2: `supply x (e^(c x g) - 1)`, checked against 80-digit decimal arithmetic with
/// the 2026-11-24 supply and `rebateC36`, and at the edges.
#[test]
fn contract_set_2_mints_a_share_of_the_supply_per_dollar() {
    let supply = 9_513_753_274_249_414_221_168_489_780;
    let c36 = 1_101_001_766_258_568_981_998_368_146;
    // (name, loss basis, PAPER minted)
    let cases = [
        ("a dollar", WAD, 10_474_659_164_463_158_000),
        ("a million dollars", 1_000_000 * WAD, 10_480_427_584_635_654_704_288_724),
        ("nothing", 0, 0),
        ("too large to compute", u128::MAX, 0),
    ];
    for (name, basis, minted) in cases {
        assert_eq!(rebate_mint(basis, supply, c36), minted, "{name}");
    }
    assert_eq!(rebate_rate(supply, c36), 10_474_659_164_463_158_000);
    assert_eq!(rebate_mint(WAD, supply, 0), 0, "no constant, no mint");
    // c x g = 1 exactly: the supply times e - 1 (1.718281828...)
    assert_eq!(rebate_mint(WAD, WAD, WAD * WAD) / 1_000_000_000, 1_718_281_828);
}
