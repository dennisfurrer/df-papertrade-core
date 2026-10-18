//! Submissions and refusals over a transport that answers one canned response.

use std::cell::RefCell;
use std::future::Future;

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::client::{Request, Response, TransportError};

use super::*;
use crate::test_support::{SESSION_SECRET, block_on, owner};
use crate::trade::intent::Close;

struct Canned {
    status: u16,
    body: String,
    sent: RefCell<Vec<Request>>,
}

impl Http for Canned {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        self.sent.borrow_mut().push(request);
        let response = Response { status: self.status, headers: Vec::new(), body: self.body.clone().into_bytes() };
        async move { Ok(response) }
    }
}

fn canned(status: u16, body: &str) -> Canned {
    Canned { status, body: body.to_owned(), sent: RefCell::new(Vec::new()) }
}

fn signed_close() -> Signed {
    let key = LocalKey::from_hex(SESSION_SECRET).unwrap();
    let intent = Intent::Close(Close::new(owner(), &[408_665], 3_170_820, 1_791_864_000).unwrap());
    let signature = key.sign_digest(&intent.digest()).unwrap();
    intent.signed(signature, key.address())
}

#[test]
fn a_submission_is_checked_against_the_answer() {
    let signed = signed_close();
    let id = hex::encode_prefixed(&signed.intent_id());
    let answer = format!(r#"{{"intentId":"{id}","action":"closePositions","state":"queued","queueRevision":"5646"}}"#);
    let relayer = Relayer::new(canned(200, &answer));
    let accepted = block_on(relayer.submit(&signed)).unwrap();
    assert!(accepted.kept());
    assert_eq!(accepted.queue_revision, Some(5_646));
    let sent = relayer.http().sent.borrow();
    assert_eq!(sent.len(), 1, "posted once");
    assert_eq!(sent[0].url, "https://exchange.papertrade.xyz/intents");
    assert_eq!(String::from_utf8(sent[0].body.clone()).unwrap(), signed.body());

    let other = answer.replace(&id[2..10], "00000000");
    assert!(matches!(
        block_on(Relayer::new(canned(200, &other)).submit(&signed)),
        Err(RelayerError::OtherIntent { .. })
    ));
}

#[test]
fn a_refusal_carries_its_code_and_words() {
    let body = r#"{"ok":false,"error":"deadline_expired","message":"This request expired before it was accepted."}"#;
    let refused = block_on(Relayer::new(canned(422, body)).submit(&signed_close())).unwrap_err();
    assert_eq!(
        refused,
        RelayerError::Refused {
            status: 422,
            code: Some("deadline_expired".to_owned()),
            message: Some("This request expired before it was accepted.".to_owned()),
        }
    );
    let quiet =
        block_on(Relayer::new(canned(200, r#"{"ok":false,"code":"intake_action_paused"}"#)).send(&Call::health()));
    assert!(matches!(quiet, Err(RelayerError::Refused { code: Some(code), .. }) if code == "intake_action_paused"));
}
