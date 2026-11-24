//! The rates a close signed now settles at: the builder fee of the stored session key, read on
//! HyperEVM (the key's registration at the SessionKeyManager, then the builder's fee at the
//! BuilderRegistry), and the win fee and curve of the trading state as recorded on 2026-11-24
//! (contract set 2, the off-hours curve on). Then the account stream's recent nonces moving the
//! stored lanes on.

use df_hyperliquid_core::utils::Address;
use df_papertrade_core::account::{MemoryStore, RecentNonces};
use df_papertrade_core::client::deployment::{BUILDER_REGISTRY, EXCHANGE, SESSION_KEY_MANAGER};
use df_papertrade_core::markets::TradingState;
use df_papertrade_core::trade::math::{BUILDER_FEE_RATE, WIN_FEE_RATE};
use df_papertrade_core::trade::{Curve, Rates};
use df_papertrade_core::utils::Lane;
use serde_json::{Value, json};

use super::{lanes, next, ready, session};
use crate::support::venue::Venue;
use crate::support::{client, fixture, owner};

/// trademarkets, an approved builder on 2026-11-24.
const BUILDER: &str = "6169d2c383451347b98faadec6f635aa59d8e4c4";

fn eth_call(to: Address, data: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "eth_call", "params": [{"to": to.to_hex(), "data": data}, "latest"]})
}

/// The SessionKeyManager's answer for a key registered until 2026-12-02, naming `builder`.
fn registered(builder: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"result":"0x{:064x}{builder:0>64}"}}"#, 1_796_290_091u64)
}

fn session_keys_call() -> String {
    format!("0x96ade1f9{:0>64}{:0>64}", &owner().to_hex()[2..], &session().to_hex()[2..])
}

#[tokio::test]
async fn a_close_through_an_approved_builder_pays_its_fee_on_the_curve_in_force() {
    let venue = Venue::new()
        .rpc_call(&SESSION_KEY_MANAGER.to_hex(), &registered(BUILDER))
        .rpc_call(&BUILDER_REGISTRY.to_hex(), &fixture("hyperevm/builder_fee_rate_of.json"));
    let client = client(&venue);
    let state = TradingState::parse(&fixture("trading_mainnet.json")).unwrap();

    let fee = client.close_builder_fee(&ready(77, 1).await, owner()).await.unwrap();
    let rates = Rates::current(&state, fee).unwrap();

    assert_eq!(rates, Rates { win_fee: WIN_FEE_RATE, builder_fee: BUILDER_FEE_RATE, curve: Curve::OffHours });
    assert_eq!(
        venue.calls(),
        [
            eth_call(SESSION_KEY_MANAGER, &session_keys_call()),
            eth_call(BUILDER_REGISTRY, &format!("0xd779e270{BUILDER:0>64}")),
        ]
    );
    assert!(venue.relayed().is_empty());
}

/// A key that names no builder costs one read; no stored key costs none. A builder the registry
/// no longer approves has a fee rate of 0.
#[tokio::test]
async fn a_close_with_no_approved_builder_pays_no_builder_fee() {
    let named_none = Venue::new().rpc_call(&SESSION_KEY_MANAGER.to_hex(), &fixture("hyperevm/session_key.json"));
    let fee = client(&named_none).close_builder_fee(&ready(77, 1).await, owner()).await;
    assert_eq!((fee, named_none.calls().len()), (Ok(0), 1));

    let removed = Venue::new()
        .rpc_call(&SESSION_KEY_MANAGER.to_hex(), &registered(BUILDER))
        .rpc_call(&BUILDER_REGISTRY.to_hex(), &format!(r#"{{"jsonrpc":"2.0","id":1,"result":"0x{:064x}"}}"#, 0));
    assert_eq!(client(&removed).close_builder_fee(&ready(77, 1).await, owner()).await, Ok(0));

    let keyless = Venue::new();
    assert_eq!(client(&keyless).close_builder_fee(&lanes(77, 1).await, owner()).await, Ok(0));
    assert!(keyless.sent().is_empty(), "nothing to read");
}

#[tokio::test]
async fn the_off_hours_flag_is_read_from_the_exchange() {
    let venue = Venue::new().rpc_call(&EXCHANGE.to_hex(), &fixture("hyperevm/off_hours.json"));
    assert_eq!(client(&venue).off_hours().await, Ok(false));
    assert_eq!(venue.calls(), [eth_call(EXCHANGE, "0xf009ac03")]);
}

/// The recorded wallet's nonces, seen just ahead of the stored lanes, move them on; a lane far
/// behind, or with nothing stored, stays.
#[tokio::test]
async fn the_lanes_move_past_the_nonces_the_relayer_has_seen() {
    let client = client(&Venue::new());
    let store = lanes(182_791_800, 609_122_819).await;
    let recent = RecentNonces { exchange: Some(182_791_869), staking: Some(609_122_819) };

    let moved = client.catch_up_nonces(&store, owner(), &recent).await.unwrap();

    assert_eq!(moved, [Lane::Exchange, Lane::Staking]);
    assert_eq!((next(&store, Lane::Exchange).await, next(&store, Lane::Staking).await), (182_791_870, 609_122_820));
    let behind = RecentNonces { exchange: Some(5), staking: Some(609_122_819 + 70_000) };
    assert_eq!(client.catch_up_nonces(&store, owner(), &behind).await, Ok(Vec::new()));
    let empty = MemoryStore::default();
    assert_eq!(client.catch_up_nonces(&empty, owner(), &recent).await, Ok(Vec::new()));
}
