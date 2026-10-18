//! PaperStaking's pending reward from the wallet's staking position.

use super::*;

/// Expected: `pending_base + staked * acc / 10^27 - reward_debt`.
#[test]
fn the_pending_reward_is_paperstakings() {
    let paper = Paper { balance: 0, staked: 3 * 10u128.pow(24), pending_base: 7, reward_debt: 10u128.pow(22) };
    assert_eq!(paper.pending_reward(10u128.pow(25)), 7 + 3 * 10u128.pow(22) - 10u128.pow(22));
    assert_eq!(paper.pending_reward(0), 7, "never below the base");
}
