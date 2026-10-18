//! What is listed and how the protocol stands, read through the client.

use crate::support::client;
use crate::support::venue::Venue;

#[tokio::test]
async fn the_trading_state_and_the_protocol_summary_are_read_from_the_relayer() {
    let venue = Venue::new().get("/state/trading", "trading.json").get("/query/protocol/summary", "summary.json");
    let client = client(&venue);

    let state = client.trading_state().await.unwrap();
    client.summary().await.unwrap();

    assert!(!state.instruments.is_empty());
    assert_eq!(venue.relayed(), ["GET /state/trading", "GET /query/protocol/summary"]);
}
