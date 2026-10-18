//! What the client reads on HyperEVM rather than from the relayer: the book's BBO and its mid from
//! HyperCore's precompile, the Exchange's win fee and a wallet's unpaid staking rewards. The node
//! answers as mainnet did (`fixtures/hyperevm/`), and each call goes out as an `eth_call` at
//! `latest` to its contract.

use df_hyperliquid_core::client::evm::{ReadError as PrecompileError, RpcError};
use df_hyperliquid_core::utils::{Address, Dec};
use df_papertrade_core::Error;
use df_papertrade_core::client::Quote;
use df_papertrade_core::client::chain::ReadError;
use df_papertrade_core::client::deployment::{BTC, ETH, EXCHANGE, PAPER_STAKING};
use df_papertrade_core::trade::math::WIN_FEE_RATE;
use df_papertrade_core::utils::units;
use serde_json::{Value, json};

use crate::support::venue::Venue;
use crate::support::{client, fixture};

/// HyperCore's BBO precompile.
const BBO: &str = "0x000000000000000000000000000000000000080e";
/// The wallet whose account stream is recorded, a staker.
const BUSY: &str = "0xa9f28db0543b215eb2705740f10258c3d000cefd";
/// A node's answer to a call that reverted.
const REVERTED: &str = r#"{"jsonrpc":"2.0","id":1,"error":{"code":3,"message":"execution reverted"}}"#;

/// The `eth_call` the client makes: `data` to `to` at the latest block.
fn eth_call(to: &str, data: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "eth_call", "params": [{"to": to, "data": data}, "latest"]})
}

/// A node's answer of two words.
fn words(first: u64, second: u64) -> String {
    answer(&format!("{first:064x}{second:064x}"))
}

/// A node's answer of `hex`.
fn answer(hex: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"result":"0x{hex}"}}"#)
}

#[tokio::test]
async fn a_quote_is_the_mid_of_the_book_the_oracle_reads() {
    let venue = Venue::new().rpc_call(BBO, &fixture("hyperevm/bbo_btc.json"));

    let quote = client(&venue).quote(BTC.asset_index).await.unwrap();

    assert_eq!(quote, Quote { bid: 813_000, ask: 813_010, mid: 813_005 });
    assert_eq!(units::price(quote.mid, BTC.price_scale), Some(Dec::parse("81300.5").unwrap()));
    assert_eq!(venue.calls(), [eth_call(BBO, &format!("0x{:064x}", BTC.asset_index))]);
    assert!(venue.relayed().is_empty(), "the relayer is never asked");
}

#[tokio::test]
async fn an_odd_spread_is_rounded_down_to_the_mid_the_oracle_executes_at() {
    let venue = Venue::new().rpc_call(BBO, &words(249_235, 249_240));

    let quote = client(&venue).quote(ETH.asset_index).await.unwrap();

    assert_eq!(quote, Quote { bid: 249_235, ask: 249_240, mid: 249_237 });
    assert_eq!(units::price(quote.mid, ETH.price_scale), Some(Dec::parse("2492.37").unwrap()));
    assert_eq!(venue.calls(), [eth_call(BBO, &format!("0x{:064x}", 1))]);
}

#[tokio::test]
async fn an_empty_or_crossed_book_gives_no_quote() {
    for (bid, ask) in [(0, 813_010), (813_000, 0), (0, 0), (813_010, 813_000)] {
        let venue = Venue::new().rpc_call(BBO, &words(bid, ask));
        let quote = client(&venue).quote(BTC.asset_index).await;
        assert_eq!(quote, Err(Error::Decode("the book is empty or crossed".to_owned())), "{bid} {ask}");
    }
}

#[tokio::test]
async fn a_precompile_that_refuses_gives_no_quote() {
    let venue = Venue::new().rpc_call(BBO, REVERTED);

    let quote = client(&venue).quote(77).await;

    let Err(Error::Precompile(PrecompileError::Rpc(RpcError::Node { method: "eth_call", error }))) = quote else {
        panic!("{quote:?}")
    };
    assert_eq!((error.code, error.message.as_str()), (3, "execution reverted"));
}

#[tokio::test]
async fn the_win_fee_and_a_wallet_s_unpaid_rewards_are_read_from_their_contracts() {
    let venue = Venue::new()
        .rpc_call(&EXCHANGE.to_hex(), &fixture("hyperevm/win_fee_rate.json"))
        .rpc_call(&PAPER_STAKING.to_hex(), &fixture("hyperevm/pending_reward.json"));
    let client = client(&venue);
    let busy: Address = BUSY.parse().unwrap();

    let fee = client.win_fee_rate().await.unwrap();
    let pending = client.pending_reward(busy).await.unwrap();

    assert_eq!((fee, units::to_dec(fee, 18)), (WIN_FEE_RATE, Some(Dec::parse("0.02").unwrap())), "2% of a win");
    assert_eq!(units::usd(pending), Some(Dec::parse("4.01699399888367832").unwrap()));
    assert_eq!(
        venue.calls(),
        [
            eth_call(&EXCHANGE.to_hex(), "0x48ee5e08"),
            eth_call(&PAPER_STAKING.to_hex(), &format!("0xf40f0f52000000000000000000000000{}", &BUSY[2..])),
        ]
    );
    assert!(venue.relayed().is_empty());
}

#[tokio::test]
async fn a_contract_read_that_fails_says_why() {
    let reverted = Venue::new().rpc_call(&EXCHANGE.to_hex(), REVERTED);
    let short = Venue::new().rpc_call(&EXCHANGE.to_hex(), &answer(""));
    let past_u128 = answer(&format!("{:0>32}{:032x}", 1, 0));
    let huge = Venue::new().rpc_call(&PAPER_STAKING.to_hex(), &past_u128);

    let reverted = client(&reverted).win_fee_rate().await;
    let short = client(&short).win_fee_rate().await;
    let huge = client(&huge).pending_reward(BUSY.parse().unwrap()).await;

    assert!(
        matches!(reverted, Err(Error::Chain(ReadError::Rpc(RpcError::Node { method: "eth_call", .. })))),
        "{reverted:?}"
    );
    assert!(matches!(short, Err(Error::Chain(ReadError::Abi(_)))), "{short:?}");
    assert_eq!(huge, Err(Error::Chain(ReadError::Shape)), "2^128 is past what a reward can be");
}
