//! PaperStaking's pending reward from the wallet's staking position.

use super::*;

/// Expected: `pending_base + staked * acc / 10^27 - reward_debt`, never below the base.
#[test]
fn the_pending_reward_is_paperstakings() {
    let e = |power: u32| 10u128.pow(power);
    // (staked, reward debt, accumulator, pending)
    let cases = [
        (3 * e(24), e(22), e(25), 7 + 3 * e(22) - e(22)),
        (3 * e(24), e(22), 0, 7),
        (3 * e(24), 4 * e(22), e(25), 7),
        (0, 0, e(25), 7),
        (u128::MAX, 0, u128::MAX, 7),
    ];
    for (staked, reward_debt, acc, expected) in cases {
        let paper = Paper { balance: 0, staked, pending_base: 7, reward_debt };
        assert_eq!(paper.pending_reward(acc), expected, "{staked} staked at {acc}");
    }
}
