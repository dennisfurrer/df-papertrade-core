//! Papertrade, a perps venue on HyperEVM: every trade a synthetic swap against
//! an LP that started at zero, priced at Hyperliquid's BBO mid, read on chain
//! through the BBO precompile. Up to 1000× leverage, no orderbook, spread,
//! slippage or funding; the venue's cut is an asymmetric impact on winning
//! closes and a 2% fee. Winners the LP cannot pay wait in a FIFO queue; losses
//! mint PAPER, and staked PAPER earns USDC.
//!
//! Trading is gasless: a wallet registers a session key once, the key signs
//! opens and closes, the wallet itself signs withdrawals and staking, and the
//! relayer batches every signed intent on chain. The contracts take intents
//! from the relayer alone, so a frontend of its own signs the same intents and
//! posts them to the relayer.
//!
//! Owns:
//! - [`deployment`]: the contracts on chain 999, the listed markets, the
//!   relayer's origin, and the three EIP-712 domains, checked on chain;
//! - [`intent`]: every intent's typed data, digest and wallet JSON, the
//!   payload and body the relayer takes, and the intent id it answers with,
//!   checked against viem (`vectors/papertrade_viem.json`);
//! - [`session`] and [`nonce`]: session keys (lifetime, status, and signing
//!   with a key in memory behind the `sign` feature) and client-chosen nonces;
//! - [`relayer`]: each call as a pure request, and a client over
//!   [`hl_core::Http`]; submissions are never retried;
//! - [`trading`], [`account`] and [`sse`]: the trading state, the live account
//!   stream folded snapshot and patch with the app's own checks, and the event
//!   stream split from its text;
//! - [`stats`]: the protocol's summary, markets, queue and recent trades;
//! - [`math`]: bust prices, a position's PnL after the impact curve and the
//!   win fee, the most an open may commit, and the PAPER mint rate, in
//!   integers exactly as the app computes them;
//! - [`chain`]: the reads it makes on HyperEVM (deposit address, BBO, win fee,
//!   pending rewards);
//! - [`units`]: collateral in 18 decimals, USDC in 6, PAPER in 18, prices at
//!   each market's scale.
//!
//! Sans-IO: no runtime, clock or randomness; the caller brings the time, the
//! random bytes and the wire. Mainnet only. Money is integers and
//! [`hl_core::Dec`], never floats.
#![deny(clippy::float_arithmetic)]

pub mod account;
pub mod chain;
pub mod deployment;
pub mod intent;
pub mod math;
pub mod nonce;
pub mod relayer;
pub mod session;
pub mod sse;
pub mod stats;
pub mod trading;
pub mod units;
mod wire;

pub use account::{Account, AccountError, Applied, ClosedPosition, Operation, OperationKind, Outcome, Position};
#[cfg(feature = "sign")]
pub use intent::BadSignature;
pub use intent::{
    CancelIntent, Claim, Close, Intent, Key, OpenPosition, Refusal, RegisterSessionKey, Signed, Staking,
    WithdrawToCore, deadline, intent_id,
};
pub use math::{OpenLimits, OpenRefusal, Valuation, bust_price, value};
pub use nonce::{Lane, Nonces};
pub use relayer::{Accepted, Call, Relayer, RelayerError};
#[cfg(feature = "sign")]
pub use session::SessionKey;
pub use session::{SessionRefusal, Status};
pub use stats::{QueueRank, QueueSummary, Summary, Trade};
pub use trading::{Instrument, Intake, TradingState};
pub use wire::BadWire;
