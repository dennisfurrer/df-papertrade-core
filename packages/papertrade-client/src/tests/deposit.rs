use hl_core::{Method, Network};

use super::*;
use crate::{Error, Mode, Send, Source};

const PROXY: &str = "0xc125c9a929bd75555a027dd563c89a116bd41c50";
const RPC: &str = "https://rpc.hyperliquid.xyz/evm";
const EXCHANGE: &str = "https://api.hyperliquid.xyz/exchange";

fn word(hex: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"result":"0x{hex:0>64}"}}"#)
}

/// The relayer names the proxy, the factory derives the same, and the proxy has no HyperCore
/// account yet unless `exists`.
fn planned(exists: bool) -> Wire {
    Wire::default()
        .on(Method::Post, "/api/papertrade/funding/deposits/check", "", 200, &format!(r#"{{"proxy":"{PROXY}"}}"#))
        // computeProxyAddress(owner)
        .on(Method::Post, RPC, "0xd600539a", 200, &word(&PROXY[2..]))
        // coreUserExists(proxy)
        .on(Method::Post, RPC, "0000000000000000000000000000000000000810", 200, &word(if exists { "1" } else { "0" }))
}

fn wire_of(send: &Send) -> String {
    send.action().wire(Network::Mainnet)
}

#[test]
fn a_deposit_from_perps_opens_the_address_first() {
    let wire = planned(false);
    let client = client(&wire);
    let deposit = block_on(client.plan_deposit(owner(), 25_000_000, Source::Perp, Mode::Standard)).unwrap();
    assert_eq!(deposit.to.to_hex(), PROXY);
    assert!(deposit.opens_account);
    assert!(matches!(&deposit.sends[..], [Send::ToSpot(_), Send::Spot(_)]));
    assert!(wire_of(&deposit.sends[0]).contains(r#""amount":"26""#), "the amount and the 1 USDC that opens it");
    assert!(wire_of(&deposit.sends[1]).contains(r#""amount":"25""#));
    assert!(wire_of(&deposit.sends[1]).contains(PROXY));

    let pooled = block_on(client.plan_deposit(owner(), 25_000_000, Source::Spot, Mode::Pooled)).unwrap();
    assert!(matches!(&pooled.sends[..], [Send::Pooled(_)]));
    let small = block_on(client.plan_deposit(owner(), 9_999_999, Source::Spot, Mode::Standard)).unwrap_err();
    assert!(matches!(small, Error::Invalid(_)));
}

#[test]
fn an_opened_address_pays_no_fee() {
    let wire = planned(true);
    let deposit = block_on(client(&wire).plan_deposit(owner(), 25_000_000, Source::Perp, Mode::Standard)).unwrap();
    assert!(!deposit.opens_account);
    assert!(wire_of(&deposit.sends[0]).contains(r#""amount":"25""#));
}

#[test]
fn each_send_is_signed_by_the_wallet_and_posted_once() {
    let wire = planned(true).on(Method::Post, EXCHANGE, "", 200, r#"{"status":"ok","response":{"type":"default"}}"#);
    let client = client(&wire);
    let deposit = block_on(client.plan_deposit(owner(), 25_000_000, Source::Perp, Mode::Standard)).unwrap();
    block_on(client.send_deposit(&wallet(), &deposit)).unwrap();
    let posted = wire.posts_to(EXCHANGE);
    assert_eq!(posted.len(), 2);
    assert_eq!(body(&posted[0])["action"]["type"], "usdClassTransfer");
    assert_eq!(body(&posted[1])["action"]["type"], "spotSend");
}

#[test]
fn a_refused_send_stops_the_rest() {
    let wire =
        planned(true).on(Method::Post, EXCHANGE, "", 200, r#"{"status":"err","response":"Insufficient balance"}"#);
    let client = client(&wire);
    let deposit = block_on(client.plan_deposit(owner(), 25_000_000, Source::Perp, Mode::Standard)).unwrap();
    let e = block_on(client.send_deposit(&wallet(), &deposit)).unwrap_err();
    assert_eq!(e, Error::Exchange("Insufficient balance".into()));
    assert_eq!(wire.posts_to(EXCHANGE).len(), 1);
}
