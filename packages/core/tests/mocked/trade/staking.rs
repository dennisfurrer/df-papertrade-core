//! Staking: a stake, an unstake and a claim, each signed by the wallet itself at the staking
//! lane's next nonce, never the exchange lane's.

use df_papertrade_core::account::Purpose;
use df_papertrade_core::trade::{Claim, Intent, Staking, deadline};
use df_papertrade_core::utils::Lane;
use serde_json::json;

use super::{id, lanes, next, queued, signed_by, vector};
use crate::account::Prompted;
use crate::support::venue::Venue;
use crate::support::{NOW, client, owner};

#[tokio::test]
async fn a_stake_an_unstake_and_a_claim_are_the_wallets_from_the_staking_lane() {
    let cases = [vector("stake 1000 PAPER"), vector("unstake 250.5 PAPER"), vector("claim rewards")];
    let mut venue = Venue::new();
    for (case, action) in cases.iter().zip(["paperStake", "paperUnstake", "paperClaim"]) {
        venue = venue.post("/intents", &queued(id(case["intentId"].as_str().unwrap()), action));
    }
    let store = lanes(40, 3_170_824).await;
    let (client, wallet) = (client(&venue), Prompted::new());
    let thousand = 1_000_000_000_000_000_000_000;
    let two_fifty_and_a_half = 250_500_000_000_000_000_000;

    let staked = client.stake(&store, &wallet, thousand).await.unwrap();
    let unstaked = client.unstake(&store, &wallet, two_fifty_and_a_half).await.unwrap();
    let claimed = client.claim(&store, &wallet).await.unwrap();

    let posted = venue.posted("/intents");
    let actions = ["paperStake", "paperUnstake", "paperClaim"];
    let expected: Vec<_> =
        cases.iter().zip(actions).map(|(case, action)| json!({"action": action, "signed": case["signed"]})).collect();
    assert_eq!(posted, expected, "as viem signs them");
    let intents = [
        Intent::Stake(Staking::new(owner(), thousand, 3_170_824, deadline(NOW)).unwrap()),
        Intent::Unstake(Staking::new(owner(), two_fifty_and_a_half, 3_170_825, deadline(NOW)).unwrap()),
        Intent::Claim(Claim { user: owner(), nonce: 3_170_826, deadline: deadline(NOW) }),
    ];
    let answers = [staked, unstaked, claimed];
    for ((body, intent), answer) in posted.iter().zip(intents).zip(&answers) {
        assert_eq!(signed_by(body, intent, owner()).intent_id(), answer.intent_id);
    }
    let purposes: Vec<Purpose> = wallet.asked().iter().map(|asked| asked.purpose).collect();
    assert_eq!(purposes, [Purpose::Stake, Purpose::Unstake, Purpose::Claim]);
    assert_eq!((next(&store, Lane::Staking).await, next(&store, Lane::Exchange).await), (3_170_827, 40));
}
