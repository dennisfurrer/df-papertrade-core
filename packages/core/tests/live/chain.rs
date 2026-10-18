//! The HyperEVM reads the client makes, from mainnet: each listed book's BBO and mid, the
//! Exchange's win fee, a staker's unpaid rewards, deposit addresses, and whether one has a
//! HyperCore account. Each answer is held against its recording under `fixtures/hyperevm/`.
//! Only `eth_call`s: nothing is signed or sent.

use df_hyperliquid_core::client::evm::Rpc;
use df_hyperliquid_core::client::evm::precompiles::core_user_exists;
use df_hyperliquid_core::utils::{Address, Dec};
use df_papertrade_core::client::deployment::LISTINGS;
use df_papertrade_core::trade::math::WIN_FEE_RATE;
use df_papertrade_core::utils::units;

use crate::support::owner;
use crate::tap::{reader, same_shape};

/// The wallet whose account stream is recorded: it trades, stakes and has deposited.
const BUSY: &str = "0xa9f28db0543b215eb2705740f10258c3d000cefd";
/// Its deposit address, as the factory derived it on 2026-10-18.
const BUSY_PROXY: &str = "0xa9cf30b9bba30d52b82d9c677107a421c1e0dc38";
/// The vectors' wallet's deposit address, recorded in `hyperevm/deposit_address.json`.
const OWNER_PROXY: &str = "0xeb9426c82652bd8dd95742027ad301a545eaf800";

fn address(text: &str) -> Address {
    text.parse().unwrap()
}

#[tokio::test]
#[ignore = "mainnet"]
async fn each_listed_book_quotes_a_mid_inside_its_touch() {
    let client = reader();
    for listing in LISTINGS {
        let quote = client.quote(listing.asset_index).await.unwrap();
        same_shape(&client, "hyperevm/bbo_btc.json");
        assert!(quote.bid <= quote.mid && quote.mid <= quote.ask, "{}: {quote:?}", listing.symbol);
        assert!((quote.ask - quote.bid) * 100 < quote.mid, "{}: a spread under 1%: {quote:?}", listing.symbol);
        let price = units::price(quote.mid, listing.price_scale).unwrap();
        assert!(price > Dec::from_int(100), "{}: {price}", listing.symbol);
    }
}

#[tokio::test]
#[ignore = "mainnet"]
async fn the_win_fee_is_the_one_the_math_assumes() {
    let client = reader();
    assert_eq!(client.win_fee_rate().await, Ok(WIN_FEE_RATE));
    same_shape(&client, "hyperevm/win_fee_rate.json");
}

#[tokio::test]
#[ignore = "mainnet"]
async fn a_staker_s_unpaid_rewards_read() {
    let client = reader();
    let pending = client.pending_reward(address(BUSY)).await.unwrap();
    same_shape(&client, "hyperevm/pending_reward.json");
    assert!(units::usd(pending).is_some(), "{pending}");
}

/// A CREATE2 address: the same for a wallet whatever the block.
#[tokio::test]
#[ignore = "mainnet"]
async fn deposit_addresses_are_the_ones_the_factory_derived_before() {
    let client = reader();
    assert_eq!(client.deposit_address(owner()).await, Ok(address(OWNER_PROXY)));
    same_shape(&client, "hyperevm/deposit_address.json");
    assert_eq!(client.deposit_address(address(BUSY)).await, Ok(address(BUSY_PROXY)));
    same_shape(&client, "hyperevm/deposit_address.json");
}

/// What a deposit plan asks HyperCore before it adds the fee that opens an account.
#[tokio::test]
#[ignore = "mainnet"]
async fn a_used_deposit_address_has_a_hypercore_account() {
    let client = reader();
    let rpc = Rpc::new(client.http().clone(), client.config().rpc.clone());
    assert_eq!(rpc.read(&core_user_exists(address(BUSY_PROXY))).await, Ok(true));
    same_shape(&client, "hyperevm/core_user_opened.json");
    rpc.read(&core_user_exists(address(OWNER_PROXY))).await.unwrap();
    same_shape(&client, "hyperevm/core_user_new.json");
}
