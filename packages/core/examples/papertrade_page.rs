//! Reads the trading state and protocol figures from the relayer, the BBO and a wallet's
//! deposit address from HyperEVM, then builds the open a $10 BTC long at 100x would sign.
//! Nothing is signed or sent.
//!
//! ```sh
//! cargo run -p df-papertrade-core --example papertrade_page
//! ```

use std::error::Error;

use df_hyperliquid_core::client::native::{Native, clock};
use df_hyperliquid_core::utils::{Address, Round, hex};
use df_papertrade_core::trade::math::{self, WIN_FEE_RATE};
use df_papertrade_core::trade::{Intent, OpenPosition, deadline};
use df_papertrade_core::utils::units;
use df_papertrade_core::{Client, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Reads only: nothing here makes a session key or a nonce, so no randomness is needed.
    let client = Client::new(Native::new(), Config::mainnet(clock, |_| Err("this example signs nothing".to_owned())));

    let state = client.trading_state().await?;
    let btc = state.by_symbol("BTC").ok_or("BTC is not listed")?;
    let quote = client.quote(btc.asset_index).await?;
    let show = |raw| units::price(raw, btc.price_scale).map_or_else(|| "?".to_owned(), |price| price.to_string());
    println!(
        "BTC  instrument {}  up to {}x  mid {} (bid {} ask {})",
        btc.id,
        btc.max_leverage,
        show(quote.mid),
        show(quote.bid),
        show(quote.ask)
    );

    let summary = client.summary().await?;
    let usd = |raw| units::usd(raw).map_or_else(|| "?".to_owned(), |dollars| dollars.round(0, Round::Down).to_string());
    println!(
        "     tvl ${}  lp ${}  queue ${}  {} open  PAPER mints {} a dollar",
        usd(summary.tvl),
        usd(summary.lp),
        usd(summary.queue),
        summary.open_positions,
        units::to_dec(summary.paper.mint_rate(), 18)
            .map_or_else(|| "?".to_owned(), |rate| rate.round(2, Round::Down).to_string())
    );

    let wallet: Address = "0x9587c978341b4de61fa14e4705f65fbaaec72741".parse()?;
    println!("     deposit address of {wallet}: {}", client.deposit_address(wallet).await?);

    let margin = units::collateral("10".parse()?).ok_or("bad margin")?;
    math::check_open(&state, btc, true, margin, 100, Some(quote.mid), margin, None)?;
    let entry_bust = math::bust_price(quote.mid, 100, true, btc.bust_buffer).ok_or("no bust price")?;
    let open = OpenPosition::new(wallet, btc.id, true, margin, 100, false, 256, deadline(clock()))?;
    let intent = Intent::OpenPosition(open);
    println!("\nwould sign ({:?} key): {}", intent.key(), intent.wallet_json());
    println!(
        "digest {}  bust near {}  win fee {}%",
        hex::encode_prefixed(&intent.digest()),
        show(entry_bust),
        WIN_FEE_RATE / 10u128.pow(16)
    );
    Ok(())
}
