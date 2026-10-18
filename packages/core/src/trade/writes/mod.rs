//! Intents posted to the relayer once each: open, close and cancel signed by the session key,
//! withdraw, stake, unstake and claim signed by the wallet. If the relayer does not answer, the
//! intent's result is unknown, not failed, and the account stream reports it by id.

mod cancel;
mod close;
pub mod open;
mod staking;
mod submit;
mod withdraw;

pub use open::Open;
