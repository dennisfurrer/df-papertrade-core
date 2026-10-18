//! A mainnet [`Client`] whose wire keeps every answer, so a test can hold what the relayer or the
//! node said against the recording the unit and mocked suites read.

use std::cell::RefCell;
use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;

use df_hyperliquid_core::client::native::{Native, clock};
use df_hyperliquid_core::client::{Http, Request, Response, TransportError};
use df_papertrade_core::{Client, Config};
use serde_json::Value;

use crate::shape::added;
use crate::support::{fixture, sevens};

/// The real wire, keeping each request's URL and its answer's text. Clones share what was kept.
#[derive(Clone)]
pub struct Tap {
    wire: Native,
    kept: Rc<RefCell<Vec<(String, String)>>>,
}

impl Http for Tap {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        let url = request.url.clone();
        async move {
            let answer = self.wire.send(request).await?;
            self.kept.borrow_mut().push((url, answer.text()));
            Ok(answer)
        }
    }
}

/// A client on mainnet through a [`Tap`]. It only reads: nothing in the live suite signs.
pub fn reader() -> Client<Tap> {
    let tap = Tap { wire: Native::new(), kept: Rc::default() };
    Client::new(tap, Config::mainnet(clock, sevens))
}

/// The last answer the client got, as JSON.
pub fn last(client: &Client<Tap>) -> Value {
    let kept = client.http().kept.borrow();
    let (url, answer) = kept.last().expect("nothing was sent");
    serde_json::from_str(answer).unwrap_or_else(|error| panic!("{error}: the answer from {url} is not JSON: {answer}"))
}

/// Holds the last answer against the recording at `fixtures/<recording>`. Fails naming every key
/// path the venue has added since it was recorded, and keeps the live answer as
/// `target/tmp/live/<recording>` to re-record from.
pub fn same_shape(client: &Client<Tap>, recording: &str) {
    let live = last(client);
    let recorded: Value = serde_json::from_str(&fixture(recording)).unwrap();
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("live").join(recording);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&live).unwrap()).unwrap();
    let added = added(&recorded, &live);
    assert!(added.is_empty(), "the venue now answers {recording} with fields it did not have: {added:#?}");
}
