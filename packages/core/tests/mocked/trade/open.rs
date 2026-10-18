//! Opens: checked against a fresh trading state and the account as streamed, then signed by the
//! session key at the exchange lane's next nonce, from the balance or the queued balance.

use df_papertrade_core::trade::{Intent, Open, OpenPosition};
use df_papertrade_core::utils::Lane;
use df_papertrade_core::utils::units::WAD;
use serde_json::json;

use super::{BTC_MARK, account, id, next, queued, ready, session, signed_by, vector};
use crate::support::client;
use crate::support::venue::Venue;

/// ETH's mid, raw: $2,502.25.
const ETH_MARK: u128 = 250_225;

/// The intent a vector's message describes, as the client builds it.
fn open_of(case: &serde_json::Value) -> Intent {
    let m = &case["message"];
    let number = |key: &str| m[key].as_str().unwrap().parse::<u128>().unwrap();
    Intent::OpenPosition(
        OpenPosition::new(
            m["user"].as_str().unwrap().parse().unwrap(),
            u32::try_from(number("instrumentId")).unwrap(),
            m["isLong"].as_bool().unwrap(),
            number("size"),
            u32::try_from(number("leverage")).unwrap(),
            m["useDebt"].as_bool().unwrap(),
            number("nonce"),
            u64::try_from(number("deadline")).unwrap(),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn an_open_from_the_balance_is_checked_then_signed_by_the_session_key() {
    let case = vector("open BTC long 1000x $10 from balance");
    let intent_id = id(case["intentId"].as_str().unwrap());
    let venue = Venue::new().get("/state/trading", "trading.json").post("/intents", &queued(intent_id, "openPosition"));
    let store = ready(3_170_817, 900).await;

    let ticket = Open { instrument_id: 0, is_long: true, margin: 10 * WAD, leverage: 1000, use_debt: false };
    let answer =
        client(&venue).open(&store, crate::support::owner(), &account(100, 0), ticket, Some(BTC_MARK)).await.unwrap();

    assert_eq!(venue.relayed(), ["GET /state/trading", "POST /intents"]);
    let posted = venue.posted("/intents");
    assert_eq!(posted, [json!({"action": "openPosition", "signed": case["signed"]})], "as viem signs it");
    assert_eq!(signed_by(&posted[0], open_of(&case), session()).intent_id(), intent_id);
    assert_eq!(
        (answer.intent_id, answer.action.as_str(), answer.state.as_str()),
        (intent_id, "openPosition", "queued")
    );
    assert_eq!(answer.queue_revision, Some(882));
    assert_eq!((next(&store, Lane::Exchange).await, next(&store, Lane::Staking).await), (3_170_818, 900));
}

#[tokio::test]
async fn an_open_from_the_queued_balance_draws_on_what_the_queue_can_fund() {
    let case = vector("open ETH short 2x from queued balance");
    let size: u128 = case["message"]["size"].as_str().unwrap().parse().unwrap();
    let intent_id = id(case["intentId"].as_str().unwrap());
    let venue = Venue::new().get("/state/trading", "trading.json").post("/intents", &queued(intent_id, "openPosition"));
    let store = ready(3_170_818, 1).await;

    let ticket = Open { instrument_id: 1, is_long: false, margin: size, leverage: 2, use_debt: true };
    let from_queue = account(0, 5_000);
    let answer =
        client(&venue).open(&store, crate::support::owner(), &from_queue, ticket, Some(ETH_MARK)).await.unwrap();

    let posted = venue.posted("/intents");
    assert_eq!(posted, [json!({"action": "openPosition", "signed": case["signed"]})], "as viem signs it");
    assert_eq!(signed_by(&posted[0], open_of(&case), session()).intent_id(), answer.intent_id);
    assert_eq!(next(&store, Lane::Exchange).await, 3_170_819);
}
