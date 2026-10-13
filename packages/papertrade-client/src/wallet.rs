//! The account's own wallet, for what a session key may not sign: the session
//! key's registration, withdrawals, staking, and a deposit's sends on
//! Hyperliquid. Handed each as a [`SignRequest`]: what it is for, the
//! typed-data JSON a browser wallet's `eth_signTypedData_v4` takes, and the
//! digest that JSON hashes to, for a signer that signs digests and shows the
//! JSON. Every signature is recovered against the account's owner before it
//! is posted.

use std::future::Future;

use hl_core::{Address, LocalKey, Signature};

/// What the wallet is asked to sign, in words a confirm can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Lets a session key trade for the wallet for 30 days; it can never
    /// withdraw, stake or claim.
    RegisterSessionKey,
    /// USDC from the venue to the wallet's own HyperCore spot account.
    Withdraw,
    Stake,
    Unstake,
    /// Distributed USDC rewards into the venue balance.
    Claim,
    /// A deposit's first send: USDC from the perp balance to spot.
    DepositToSpot,
    /// A deposit's send to the wallet's deposit address.
    Deposit,
}

impl Purpose {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Purpose::RegisterSessionKey => "register a session key that trades for 30 days and cannot withdraw",
            Purpose::Withdraw => "withdraw USDC to the wallet's own HyperCore spot account",
            Purpose::Stake => "stake PAPER",
            Purpose::Unstake => "unstake PAPER",
            Purpose::Claim => "claim staking rewards into the venue balance",
            Purpose::DepositToSpot => "move USDC from perps to spot, for a deposit",
            Purpose::Deposit => "send USDC to the deposit address",
        }
    }
}

/// One signature asked of the wallet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignRequest {
    pub purpose: Purpose,
    /// The typed data, as `eth_signTypedData_v4` takes it.
    pub json: String,
    /// What `json` hashes to under EIP-712: the bytes signed.
    pub digest: [u8; 32],
}

/// Why the wallet did not sign.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WalletError {
    #[error("declined in the wallet")]
    Declined,
    #[error("the wallet is on chain {got}, and this is signed on {want}")]
    WrongChain { got: u64, want: u64 },
    #[error("the wallet could not sign: {0}")]
    Failed(String),
}

/// The account's own wallet. No `Send` bound: the browser is single-threaded.
pub trait Wallet {
    fn address(&self) -> Address;

    fn sign(&self, request: &SignRequest) -> impl Future<Output = Result<Signature, WalletError>>;
}

/// A key held in memory as the wallet: tests, and native tools that hold one.
pub struct LocalWallet(pub LocalKey);

impl std::fmt::Debug for LocalWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LocalWallet").field(&self.0.address()).finish()
    }
}

impl Wallet for LocalWallet {
    fn address(&self) -> Address {
        self.0.address()
    }

    fn sign(&self, request: &SignRequest) -> impl Future<Output = Result<Signature, WalletError>> {
        std::future::ready(self.0.sign_digest(&request.digest).map_err(|e| WalletError::Failed(e.to_string())))
    }
}
