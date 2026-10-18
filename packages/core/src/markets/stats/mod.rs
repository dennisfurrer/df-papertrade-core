//! Public protocol figures from the relayer's query service. Amounts are collateral units (18
//! decimals) unless named otherwise.
//!
//! - [`summary`]: TVL, LP, fees, and PAPER's supply and mint curve.
//! - [`volumes`]: each market's notional over the last day.
//! - [`queue`]: the payout queue, what queued winners are owed.
//! - [`trades`]: the latest trades across the venue.
//! - [`rank`]: where a wallet's intents stand in the relayer's queue.

pub mod queue;
pub mod rank;
pub mod summary;
pub mod trades;
pub mod volumes;

pub use queue::QueueSummary;
pub use rank::QueueRank;
pub use summary::{PaperFigures, Summary};
pub use trades::{Trade, TradeKind, recent_trades};
pub use volumes::market_volumes;
