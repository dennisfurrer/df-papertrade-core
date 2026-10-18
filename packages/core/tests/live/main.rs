//! Against mainnet, read only, run by hand with `just live`: every relayer read, the account
//! stream, and the HyperEVM reads the client makes. Nothing is ever submitted, signed, deposited
//! or registered.
//!
//! Each read must parse into its type, and its answer must have the shape of the recording the
//! unit and mocked suites read ([`tap::same_shape`]), so a field the relayer has added shows up
//! here before it reaches a user.

// A test crate: clippy's allowance for tests covers `#[test]` functions only, not their helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod shape;
mod tap;

mod chain;
mod relayer;
mod stream;
