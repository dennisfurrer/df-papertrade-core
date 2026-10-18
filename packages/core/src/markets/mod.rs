//! What the venue lists and its figures.
//!
//! - [`trading`]: the trading state: switches, minimums, every instrument and the relayer's
//!   intake.
//! - [`stats`]: the protocol summary, market volumes, the payout queue, recent trades and a
//!   wallet's queue rank.

pub mod stats;
pub mod trading;

pub use stats::{QueueRank, QueueSummary, Summary, Trade};
pub use trading::{Instrument, Intake, TradingState};
