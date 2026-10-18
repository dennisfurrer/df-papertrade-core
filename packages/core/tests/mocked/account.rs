//! The session key through the [`Client`](df_papertrade_core::Client): made from the caller's
//! randomness and kept in a store, registered once by the wallet through the relayer, renewed near
//! its end, and refused before or after it is posted. The wallets here keep every request they
//! are asked to sign, decline, or sign with a key that is not theirs.

mod refused;
mod session;

use std::cell::RefCell;
use std::future::{Future, ready};

use df_hyperliquid_core::account::{LocalKey, Signature};
use df_hyperliquid_core::utils::Address;
use df_papertrade_core::account::{LocalWallet, SignRequest, Wallet, WalletError};

use crate::support::wallet;

/// The owner's own wallet, keeping each request it signs.
pub struct Prompted {
    wallet: LocalWallet,
    asked: RefCell<Vec<SignRequest>>,
}

impl Prompted {
    pub fn new() -> Prompted {
        Prompted { wallet: wallet(), asked: RefCell::default() }
    }

    /// What it was asked to sign, in order.
    pub fn asked(&self) -> Vec<SignRequest> {
        self.asked.borrow().clone()
    }
}

impl Wallet for Prompted {
    fn address(&self) -> Address {
        self.wallet.address()
    }

    fn sign(&self, request: &SignRequest) -> impl Future<Output = Result<Signature, WalletError>> {
        self.asked.borrow_mut().push(request.clone());
        self.wallet.sign(request)
    }
}

/// The owner's wallet, declining every request.
pub struct Declines;

impl Wallet for Declines {
    fn address(&self) -> Address {
        wallet().address()
    }

    fn sign(&self, _: &SignRequest) -> impl Future<Output = Result<Signature, WalletError>> {
        ready(Err(WalletError::Declined))
    }
}

/// Claims the owner's address but signs with another key.
pub struct Forges;

impl Wallet for Forges {
    fn address(&self) -> Address {
        wallet().address()
    }

    fn sign(&self, request: &SignRequest) -> impl Future<Output = Result<Signature, WalletError>> {
        let other = LocalKey::from_bytes(&[9; 32]).unwrap();
        ready(other.sign_digest(&request.digest).map_err(|error| WalletError::Failed(error.to_string())))
    }
}
