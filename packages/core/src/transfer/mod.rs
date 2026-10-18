//! Moving money into the venue: [`deposit`] plans and sends USDC from the wallet's Hyperliquid
//! balance to its deposit address. Withdrawals are intents, in [`crate::trade`].

pub mod deposit;

pub use deposit::{ACTIVATION_FEE_USDC, Deposit, MIN_DEPOSIT_USDC, Mode, Send, Source};
