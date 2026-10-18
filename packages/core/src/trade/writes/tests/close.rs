//! Closes over a scripted wire: twelve positions to an intent, one nonce each.

use df_hyperliquid_core::client::Method;

use super::*;
use crate::test_support::{NOW_MS, Wire, accepted, block_on, client, owner, record, session_address, store};
use crate::trade::intent::intent_id;

#[test]
fn closes_go_twelve_to_an_intent() {
    let store = store(Some(&record(true)), 500, 1);
    let ids: Vec<u128> = (1..=25).collect();
    let mut wire = Wire::default();
    for (n, chunk) in ids.chunks(12).enumerate() {
        let close = Close::new(owner(), chunk, 500 + n as u128, deadline(NOW_MS)).unwrap();
        let id = intent_id(&Intent::Close(close).digest(), session_address());
        let piece: &'static str = Box::leak(format!("\"nonce\":\"{}\"", 500 + n).into_boxed_str());
        wire = wire.on(Method::Post, "/api/papertrade/intents", piece, 200, &accepted(id, "closePositions"));
    }
    let answers = block_on(client(&wire).close(&store, owner(), None, &ids)).unwrap();
    assert_eq!(answers.len(), 3);
    assert_eq!(wire.posts_to("/api/papertrade/intents").len(), 3);
    assert_eq!(
        block_on(client(&wire).close(&store, owner(), None, &[])).unwrap_err(),
        Error::Invalid("no positions to close".into())
    );
}
