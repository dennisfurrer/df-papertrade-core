//! Withdrawals: USDC from the available balance to the wallet's own HyperCore spot account, signed
//! by the wallet itself at the exchange lane's next nonce.

use df_papertrade_core::account::Purpose;
use df_papertrade_core::trade::{Intent, WithdrawToCore, deadline, intent_id};
use df_papertrade_core::utils::Lane;
use serde_json::json;

use super::{account, id, lanes, next, queued, signed_by, vector};
use crate::account::Prompted;
use crate::support::venue::Venue;
use crate::support::{NOW, client, owner};

#[tokio::test]
async fn a_withdrawal_within_the_balance_goes_to_the_wallets_own_hypercore_account() {
    let case = vector("withdraw 10 USDC to core");
    let venue = Venue::new().post("/intents", &queued(id(case["intentId"].as_str().unwrap()), "withdrawToCore"));
    let store = lanes(3_170_823, 900).await;
    let wallet = Prompted::new();

    let answer = client(&venue).withdraw(&store, &wallet, Some(&account(100, 0)), 10_000_000).await.unwrap();

    let posted = venue.posted("/intents");
    let expected = json!({"action": "withdrawToCore", "signed": case["signed"], "withdrawal": {"route": "hypercore"}});
    assert_eq!(posted, [expected], "as viem signs it, to HyperCore");
    let intent = Intent::WithdrawToCore(WithdrawToCore::new(owner(), 10_000_000, 3_170_823, deadline(NOW)).unwrap());
    assert_eq!(signed_by(&posted[0], intent, owner()).intent_id(), answer.intent_id);
    assert_eq!(wallet.asked()[0].purpose, Purpose::Withdraw);
    assert_eq!((next(&store, Lane::Exchange).await, next(&store, Lane::Staking).await), (3_170_824, 900));
}

#[tokio::test]
async fn a_withdrawal_with_no_account_streamed_is_left_to_the_venue() {
    let intent = Intent::WithdrawToCore(WithdrawToCore::new(owner(), 1_000_000_000, 7, deadline(NOW)).unwrap());
    let venue = Venue::new().post("/intents", &queued(intent_id(&intent.digest(), owner()), "withdrawToCore"));

    client(&venue).withdraw(&lanes(7, 1).await, &Prompted::new(), None, 1_000_000_000).await.unwrap();

    signed_by(&venue.posted("/intents")[0], intent, owner());
}
