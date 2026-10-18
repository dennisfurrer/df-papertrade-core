//! How the relayer's refusals reach a caller: an error status, a 2xx that says `{"ok":false}` or
//! carries an `error`, and an answer to a submission that names another intent or does not read.
//! Each refusal carries the relayer's own code and words, and nothing is asked twice.

use df_hyperliquid_core::account::{LocalKey, Signature, recover};
use df_hyperliquid_core::utils::{Address, hex};
use df_papertrade_core::Error;
use df_papertrade_core::client::deployment::ORIGIN;
use df_papertrade_core::client::{Accepted, Relayer, RelayerError};
use df_papertrade_core::trade::intent::{CancelIntent, Close, Intent, Signed, deadline};
use df_papertrade_core::utils::BadWire;
use serde_json::Value;

use crate::support::venue::Venue;
use crate::support::{NOW, client, fixture, owner};

/// The viem vectors' session key, which signs closes and cancels. A published test key.
const SESSION_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const EXPIRED: &str =
    r#"{"ok":false,"error":"deadline_expired","message":"This request expired before it was accepted."}"#;
/// What the relayer's host answers a path it does not serve.
const NOT_FOUND: &str = "<html>\r\n<head><title>404 Not Found</title></head>\r\n<body>\r\n<center><h1>404 Not Found</h1></center>\r\n</body>\r\n</html>";

fn refused(status: u16, code: Option<&str>, message: Option<&str>) -> RelayerError {
    RelayerError::Refused { status, code: code.map(str::to_owned), message: message.map(str::to_owned) }
}

fn session() -> LocalKey {
    LocalKey::from_hex(SESSION_KEY).unwrap()
}

/// `intent` signed by the session key.
fn signed(intent: Intent) -> Signed {
    let signature = session().sign_digest(&intent.digest()).unwrap();
    Signed::recover(intent, signature, session().address()).unwrap()
}

/// A close of one of the owner's positions, as the session key signs it.
fn close() -> Signed {
    signed(Intent::Close(Close::new(owner(), &[408_665], 3_170_820, deadline(NOW)).unwrap()))
}

/// What the relayer answers a submission it took under `id`.
fn taken(id: &str) -> String {
    format!(r#"{{"intentId":"{id}","action":"closePositions","state":"queued","queueRevision":"5646"}}"#)
}

/// Who signed `digest`, by the `v`, `r` and `s` of a posted payload.
fn signer(payload: &Value, digest: &[u8; 32]) -> Address {
    let word = |key: &str| hex::decode_array(payload[key].as_str().unwrap()).unwrap();
    let v = payload["v"].as_str().unwrap().parse().unwrap();
    recover(digest, &Signature::from_parts(word("r"), word("s"), v)).unwrap()
}

#[tokio::test]
async fn an_error_status_is_a_refusal_in_the_relayer_s_words() {
    let venue = Venue::new().get("/state/trading", "trading.json").status("/state/trading", 422, EXPIRED).status(
        "/state/trading",
        404,
        NOT_FOUND,
    );
    let client = client(&venue);

    let expired = client.trading_state().await.unwrap_err();
    let missing = client.trading_state().await.unwrap_err();
    client.trading_state().await.unwrap();

    let words = "This request expired before it was accepted.";
    assert_eq!(expired, Error::Relayer(refused(422, Some("deadline_expired"), Some(words))));
    assert_eq!(expired.to_string(), format!("the relayer refused (422, deadline_expired): {words}"));
    assert_eq!(missing, Error::Relayer(refused(404, None, None)));
    assert_eq!(missing.to_string(), "the relayer refused (404): no reason given");
    assert_eq!(venue.relayed(), ["GET /state/trading"; 3], "each asked once, and the next read goes through");
}

#[tokio::test]
async fn a_2xx_that_says_it_refused_is_a_refusal() {
    for (answer, code, message) in [
        (r#"{"ok":false,"code":"intake_action_paused"}"#, Some("intake_action_paused"), None),
        (r#"{"ok":false}"#, None, None),
        (r#"{"error":"rate_limited","message":"Slow down."}"#, Some("rate_limited"), Some("Slow down.")),
        (r#"{"ok":true,"error":"stale_projection"}"#, Some("stale_projection"), None),
    ] {
        let venue = Venue::new().get_text("/query/protocol/summary", answer);
        let summary = client(&venue).summary().await.map(|_| ());
        assert_eq!(summary, Err(Error::Relayer(refused(200, code, message))), "{answer}");
        assert_eq!(venue.relayed(), ["GET /query/protocol/summary"]);
    }
}

/// `"error": null` says nothing went wrong: the answer is read, not refused.
#[tokio::test]
async fn a_2xx_whose_error_is_null_is_an_answer() {
    let mut answer: Value = serde_json::from_str(&fixture("summary.json")).unwrap();
    answer["error"] = Value::Null;
    let venue = Venue::new().get_text("/query/protocol/summary", &answer.to_string());
    let summary = client(&venue).summary().await.unwrap();
    assert_eq!(summary.block, 48_399_240);
}

/// Through a [`Relayer`] of its own at the venue's origin, with no client.
#[tokio::test]
async fn a_submission_is_taken_only_under_the_intent_id_sent() {
    let close = close();
    let sent = hex::encode_prefixed(&close.intent_id());
    let other = format!("0x00000000{}", &sent[10..]);
    let venue = Venue::new().post("/intents", &taken(&sent)).post("/intents", &taken(&other));
    let relayer = Relayer::new(venue);

    let accepted = relayer.submit(&close).await.unwrap();
    let elsewhere = relayer.submit(&close).await.unwrap_err();

    assert_eq!(relayer.base(), ORIGIN);
    let queued = Accepted {
        intent_id: close.intent_id(),
        action: "closePositions".to_owned(),
        state: "queued".to_owned(),
        queue_revision: Some(5_646),
    };
    assert_eq!(accepted, queued);
    assert!(accepted.kept());
    assert_eq!(elsewhere, RelayerError::OtherIntent { sent, found: other });
    let posted = relayer.http().posted("/intents");
    assert_eq!(posted.len(), 2, "each posted once, never again");
    for body in &posted {
        assert_eq!(body, &serde_json::from_str::<Value>(&close.body()).unwrap());
        assert_eq!(
            (&body["action"], &body["signed"]["user"]),
            (&Value::from("closePositions"), &Value::from(owner().to_hex()))
        );
        assert_eq!(signer(&body["signed"], &close.intent().digest()), session().address());
    }
}

#[tokio::test]
async fn a_cancel_is_taken_as_the_relayer_answers_it() {
    let queued = close().intent_id();
    let cancel =
        signed(Intent::CancelIntent(CancelIntent { user: owner(), intent_id: queued, deadline: deadline(NOW) }));
    let path = format!("/intents/{}/cancel", hex::encode_prefixed(&queued));
    let answer =
        format!(r#"{{"intentId":"{}","action":"cancelIntent","state":"cancelled"}}"#, hex::encode_prefixed(&queued));
    let venue = Venue::new().post(&path, &answer);

    let cancelled = client(&venue).relayer().submit(&cancel).await.unwrap();

    assert_ne!(cancel.intent_id(), queued, "the answer names the queued intent, not the cancel");
    assert_eq!((cancelled.intent_id, cancelled.state.as_str(), cancelled.queue_revision), (queued, "cancelled", None));
    assert!(!cancelled.kept());
    assert_eq!(venue.relayed(), [format!("POST {path}")]);
    let posted = &venue.posted(&path)[0];
    assert!(posted.get("intentId").is_none() && posted.get("action").is_none(), "the payload alone: {posted}");
    assert_eq!(posted["deadline"], Value::from(deadline(NOW).to_string()));
    assert_eq!(signer(posted, &cancel.intent().digest()), session().address());
}

#[tokio::test]
async fn a_submission_refused_or_answered_unreadably_is_not_taken() {
    let venue = Venue::new()
        .post("/intents", r#"{"action":"closePositions","state":"queued"}"#)
        .post("/intents", r#"{"ok":false,"error":"nonce_used"}"#)
        .status("/intents", 503, "upstream unavailable");
    let relayer = client(&venue).relayer();

    let unavailable = relayer.submit(&close()).await;
    let unreadable = relayer.submit(&close()).await;
    let used = relayer.submit(&close()).await;

    assert_eq!(unavailable, Err(refused(503, None, None)));
    let missing = BadWire { what: "intentId".to_owned(), found: "missing".to_owned() };
    assert_eq!(unreadable, Err(RelayerError::Unreadable(missing)));
    assert_eq!(used, Err(refused(200, Some("nonce_used"), None)));
    assert_eq!(venue.relayed(), ["POST /intents"; 3], "each posted once");
}
