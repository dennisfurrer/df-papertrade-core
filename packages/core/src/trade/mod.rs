//! Intents and their signatures, the venue's integer math, and the writes that post them.

pub mod intent;
pub mod math;
pub mod writes;

pub use intent::BadSignature;
pub use intent::{
    CancelIntent, Claim, Close, Intent, Key, OpenPosition, Refusal, RegisterSessionKey, Signed, Staking,
    WithdrawToCore, deadline, intent_id,
};
pub use math::{OpenLimits, OpenRefusal, Valuation, bust_price, value};
pub use writes::Open;
