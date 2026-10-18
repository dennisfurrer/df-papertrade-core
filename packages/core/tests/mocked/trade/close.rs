//! Closes: whole positions by id, signed by the session key, twelve to an intent and one nonce
//! each, with one answer per intent. A refusal midway leaves the rest unsent.

use df_papertrade_core::Error;
use df_papertrade_core::client::RelayerError;
use df_papertrade_core::trade::{Close, Intent, deadline, intent_id};
use df_papertrade_core::utils::Lane;
use serde_json::json;

use super::{id, next, queued, ready, session, signed_by, vector};
use crate::support::venue::Venue;
use crate::support::{NOW, client, owner};

/// The close the client signs for `ids` at `nonce`.
fn close(ids: &[u128], nonce: u128) -> Intent {
    Intent::Close(Close::new(owner(), ids, nonce, deadline(NOW)).unwrap())
}

#[tokio::test]
async fn one_position_and_then_twelve_close_as_viem_signs_them() {
    let one = vector("close one position");
    let twelve = vector("close twelve positions");
    let twelve_ids: Vec<u128> = (0..12).map(|n| 408_600 + 7 * n).collect();
    let venue = Venue::new()
        .post("/intents", &queued(id(one["intentId"].as_str().unwrap()), "closePositions"))
        .post("/intents", &queued(id(twelve["intentId"].as_str().unwrap()), "closePositions"));
    let store = ready(3_170_820, 1).await;
    let client = client(&venue);

    let first = client.close(&store, owner(), None, &[408_665]).await.unwrap();
    let second = client.close(&store, owner(), None, &twelve_ids).await.unwrap();

    let posted = venue.posted("/intents");
    let expected: Vec<_> =
        [&one, &twelve].iter().map(|case| json!({"action": "closePositions", "signed": case["signed"]})).collect();
    assert_eq!(posted, expected, "as viem signs them");
    assert_eq!(signed_by(&posted[0], close(&[408_665], 3_170_820), session()).intent_id(), first[0].intent_id);
    assert_eq!(signed_by(&posted[1], close(&twelve_ids, 3_170_821), session()).intent_id(), second[0].intent_id);
    assert_eq!((first.len(), second.len()), (1, 1));
    assert_eq!(next(&store, Lane::Exchange).await, 3_170_822);
}

#[tokio::test]
async fn twenty_five_positions_close_in_three_intents_one_nonce_each() {
    let ids: Vec<u128> = (1..=25).collect();
    let chunks: Vec<Intent> = ids.chunks(12).zip(500..).map(|(chunk, nonce)| close(chunk, nonce)).collect();
    let mut venue = Venue::new();
    for intent in &chunks {
        venue = venue.post("/intents", &queued(intent_id(&intent.digest(), session()), "closePositions"));
    }
    let store = ready(500, 1).await;

    let answers = client(&venue).close(&store, owner(), None, &ids).await.unwrap();

    assert_eq!(venue.relayed(), ["POST /intents"; 3]);
    let posted = venue.posted("/intents");
    for ((body, intent), answer) in posted.iter().zip(chunks).zip(&answers) {
        assert_eq!(signed_by(body, intent, session()).intent_id(), answer.intent_id);
    }
    let counts: Vec<usize> =
        posted.iter().map(|body| body["signed"]["positionIds"].as_array().unwrap().len()).collect();
    assert_eq!(counts, [12, 12, 1]);
    assert_eq!(next(&store, Lane::Exchange).await, 503);
}

#[tokio::test]
async fn a_close_refused_midway_leaves_the_rest_unsent() {
    let ids: Vec<u128> = (1..=25).collect();
    let first = close(&ids[..12], 500);
    let venue = Venue::new()
        .post("/intents", &queued(intent_id(&first.digest(), session()), "closePositions"))
        .post("/intents", r#"{"error":"busy","message":"the queue is full"}"#);
    let store = ready(500, 1).await;

    let got = client(&venue).close(&store, owner(), None, &ids).await;

    let busy = RelayerError::Refused {
        status: 200,
        code: Some("busy".to_owned()),
        message: Some("the queue is full".to_owned()),
    };
    assert_eq!(got, Err(Error::Relayer(busy)));
    assert_eq!(venue.relayed(), ["POST /intents"; 2], "the third twelve never went");
    assert_eq!(next(&store, Lane::Exchange).await, 502, "nor took a nonce");
}
