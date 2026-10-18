//! Cancelling a queued intent before the relayer sends it.

use df_hyperliquid_core::utils::Address;

/// Cancels a queued intent before the relayer sends it. Signed by the session key under
/// [`CANCEL_DOMAIN`](crate::client::deployment::CANCEL_DOMAIN), with no nonce and a `uint256`
/// deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelIntent {
    pub user: Address,
    pub intent_id: [u8; 32],
    pub deadline: u64,
}
