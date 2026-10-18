//! Cancels: a queued intent cancelled by the session key at its own path, with no nonce, and the
//! relayer's answer handed back as it came.

use df_papertrade_core::client::Accepted;
use df_papertrade_core::trade::{CancelIntent, Intent, deadline};
use df_papertrade_core::utils::Lane;

use super::{id, next, ready, session, signed_by, vector};
use crate::support::venue::Venue;
use crate::support::{NOW, client, owner};

#[tokio::test]
async fn a_queued_open_is_cancelled_by_the_session_key_as_viem_signs_it() {
    let case = vector("cancel the BTC open");
    let target = case["message"]["intentId"].as_str().unwrap();
    let path = format!("/intents/{target}/cancel");
    let answer = format!(r#"{{"intentId":"{target}","action":"openPosition","state":"cancelled"}}"#);
    let venue = Venue::new().post(&path, &answer);
    let store = ready(3_170_830, 900).await;

    let got = client(&venue).cancel(&store, owner(), None, id(target)).await.unwrap();

    assert_eq!(venue.relayed(), [format!("POST {path}")]);
    let posted = venue.posted(&path);
    let mut expected = case["signed"].clone();
    expected.as_object_mut().unwrap().remove("intentId");
    assert_eq!(posted, [expected], "viem's payload, less the id its path names");
    let cancel = Intent::CancelIntent(CancelIntent { user: owner(), intent_id: id(target), deadline: deadline(NOW) });
    signed_by(&posted[0], cancel, session());
    let cancelled = Accepted {
        intent_id: id(target),
        action: "openPosition".to_owned(),
        state: "cancelled".to_owned(),
        queue_revision: None,
    };
    assert_eq!(got, cancelled, "handed back though the relayer did not keep it");
    assert_eq!((next(&store, Lane::Exchange).await, next(&store, Lane::Staking).await), (3_170_830, 900), "no nonce");
}

#[tokio::test]
async fn a_cancels_answer_is_not_held_to_the_intent_it_names() {
    let target = [0x11; 32];
    let path = format!("/intents/{}/cancel", df_hyperliquid_core::utils::hex::encode_prefixed(&target));
    let other = format!(r#"{{"intentId":"0x{}","action":"closePositions","state":"queued"}}"#, "22".repeat(32));
    let venue = Venue::new().post(&path, &other);

    let got = client(&venue).cancel(&ready(1, 1).await, owner(), None, target).await.unwrap();

    assert_eq!(got.intent_id, [0x22; 32]);
    assert_eq!(venue.relayed(), [format!("POST {path}")]);
}
