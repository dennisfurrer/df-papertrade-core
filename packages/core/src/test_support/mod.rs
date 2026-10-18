//! What the unit tests share: running a future without a runtime, the recorded fixtures and
//! keys, a scripted wire, and a client with the store, wallet and account it drives.

mod fixtures;
mod setup;
mod wire;

pub(crate) use fixtures::*;
pub(crate) use setup::*;
pub(crate) use wire::*;

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

/// Runs a future to its end on this thread. The fakes here never wait, so each future is ready
/// on its first poll.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}
