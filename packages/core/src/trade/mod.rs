//! Trading on the venue.
//!
//! - [`intent`]: every intent, its EIP-712 data, signature, intent id and relayer body.
//! - [`math`]: the venue's integer math: bust prices, PnL after impact and fees, open limits and
//!   the PAPER mint.
//! - [`writes`]: the [`Client`](crate::Client) calls that sign intents and post them.

pub mod intent;
pub mod math;
pub mod writes;

pub use intent::{
    BadSignature, CancelIntent, Claim, Close, Intent, Key, OpenPosition, Refusal, RegisterSessionKey, Signed, Staking,
    WithdrawToCore, deadline, intent_id,
};
pub use math::{Curve, OpenLimits, OpenRefusal, Rates, Valuation, bust_price, value};
pub use writes::Open;
