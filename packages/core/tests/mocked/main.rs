//! Whole journeys through the public [`Client`](df_papertrade_core::Client) against a fake relayer,
//! node and `/exchange` that answer from recordings ([`support::venue::Venue`]). Each test is one
//! thing a user does, end to end; the units it passes through have their own tests next to their
//! code.

// A test crate: clippy's allowance for tests covers `#[test]` functions only, not their helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod account;
mod markets;
mod stream;
mod trade;
mod transfer;
