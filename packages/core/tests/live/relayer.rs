//! The relayer's reads, from mainnet.

use crate::tap::{reader, same_shape};

#[tokio::test]
#[ignore = "mainnet"]
async fn the_trading_state_lists_its_markets() {
    let client = reader();
    let state = client.trading_state().await.unwrap();
    assert!(!state.instruments.is_empty());
    same_shape(&client, "trading.json");
}
