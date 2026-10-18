//! The mint curve: flat below the cliff, decayed on the tail as the live figures show.

use super::*;

/// Live figures from the 2026-10-13 summary: tracked LP past the $2M cliff and tail progress
/// $114.27M, so `100 * (120 / 234.27)^2`, about 26.2 PAPER a dollar.
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
