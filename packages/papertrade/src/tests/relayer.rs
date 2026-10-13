use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

use hl_core::{Address, LocalKey, Response, hex};

use super::*;
use crate::intent::{Close, Intent};

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

/// Answers every request with one canned response and keeps what was sent.
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
    let key = LocalKey::from_hex("0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318").unwrap();
    let wallet: Address = "0x14791697260e4c9a71f18484c9f997b308e59325".parse().unwrap();
    let intent = Intent::Close(Close::new(wallet, &[408_665], 3_170_820, 1_791_864_000).unwrap());
    let signature = key.sign_digest(&intent.digest()).unwrap();
    intent.signed(signature, key.address())
}

#[test]
fn a_call_is_a_request_at_a_base() {
    let wallet: Address = "0x9587C978341B4DE61FA14E4705F65FBAAEC72741".parse().unwrap();
    let request = Call::account_stream(wallet).request("/api/");
    assert_eq!(request.url, "/api/state/user/live?wallet=0x9587c978341b4de61fa14e4705f65fbaaec72741");
    assert_eq!(request.method, Method::Get);
    assert!(request.body.is_empty());
    let check = Call::deposit_check(wallet).request(ORIGIN);
    assert_eq!(check.url, "https://exchange.papertrade.xyz/funding/deposits/check");
    assert_eq!(check.body, br#"{"user":"0x9587c978341b4de61fa14e4705f65fbaaec72741"}"#);
    assert!(check.headers.contains(&("Content-Type".to_owned(), "application/json".to_owned())));
    assert_eq!(Call::queue_rank(wallet).path, "/queue-rank/accounts/0x9587c978341b4de61fa14e4705f65fbaaec72741");
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

#[test]
fn reads_parse_through_the_client() {
    let state = block_on(Relayer::at(canned(200, include_str!("../../fixtures/trading.json")), "/api").trading_state());
    assert_eq!(state.unwrap().instruments.len(), 2);
    let proxy = block_on(
        Relayer::new(canned(200, r#"{"proxy":"0xc125c9a929bd75555a027dd563c89a116bd41c50"}"#))
            .deposit_address(Address::ZERO),
    );
    assert_eq!(proxy.unwrap().to_hex(), "0xc125c9a929bd75555a027dd563c89a116bd41c50");
}
