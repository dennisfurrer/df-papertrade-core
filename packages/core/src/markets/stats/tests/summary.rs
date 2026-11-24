//! The summary as the relayer answered it on 2026-11-24, with contract set 2's mint constant, and
//! the same answer without it, as the relayer sent before.

use super::*;

const SUMMARY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/summary.json"));
const DOLLAR: u128 = 1_000_000_000_000_000_000;

#[test]
fn the_summary_reads() {
    let summary = Summary::parse(SUMMARY).unwrap();
    assert!(summary.tvl > summary.lp && summary.open_positions > 0);
    assert_eq!(summary.queue, 0);
    assert_eq!(summary.paper.cliff, 2_000_000 * DOLLAR);
    assert_eq!(summary.paper.cap, 5_000_000 * DOLLAR);
    assert_eq!(summary.paper.rate, 100 * DOLLAR);
    assert_eq!(summary.paper.rebate_c36, Some(1_101_001_766_258_568_981_998_368_146), "c about 1.101e-9 a dollar");
}

/// `supply x (e^c - 1)` for one dollar: about 10.47 PAPER, and a $1M loss mints about 0.110% of
/// the supply, as the docs give it. Both exact to the unit, rounded down.
#[test]
fn the_mint_follows_contract_set_2_when_its_constant_is_sent() {
    let paper = Summary::parse(SUMMARY).unwrap().paper;
    assert_eq!(paper.supply, 9_513_753_274_249_414_221_168_489_780);
    assert_eq!(paper.mint_rate(), 10_474_659_164_463_158_000);
    let million = paper.mint_for_loss(1_000_000 * DOLLAR);
    assert_eq!(million, 10_480_427_584_635_654_704_288_724);
    assert_eq!(million * 1_000_000 / paper.supply, 1_101, "0.1101% of the supply");
    assert!(million > 1_000_000 * paper.mint_rate(), "the curve grows with the loss");
}

#[test]
fn without_the_constant_the_mint_follows_contract_set_1() {
    let mut json: serde_json::Value = serde_json::from_str(SUMMARY).unwrap();
    json["paper"].as_object_mut().unwrap().remove("rebateC36");
    let paper = Summary::parse(&json.to_string()).unwrap().paper;
    assert_eq!(paper.rebate_c36, None);
    let curve =
        crate::trade::math::mint_rate(paper.tracked_lp, paper.cliff, paper.rate, paper.decay, paper.tail_progress);
    assert_eq!(paper.mint_rate(), curve);
    assert!(curve < paper.rate, "past the cliff the rate has decayed");
    assert_eq!(paper.mint_for_loss(2 * DOLLAR), 2 * curve);
    json["paper"]["rebateC36"] = serde_json::json!("c");
    assert_eq!(Summary::parse(&json.to_string()).unwrap_err().what, "rebateC36");
}
