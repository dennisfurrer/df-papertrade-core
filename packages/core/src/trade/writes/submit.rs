//! What every write goes through: the lane's next nonce from the store, the wallet's signature
//! checked against the owner, and one post to the relayer that must keep the intent.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::Address;

use crate::account::RecentNonces;
use crate::account::store::{Slot, Store};
use crate::account::wallet::{Purpose, SignRequest, Wallet};
use crate::client::{Accepted, Client, Error};
use crate::trade::{Intent, Signed};
use crate::utils::{Lane, nonce};

impl<H: Http + Clone> Client<H> {
    /// Returns `lane`'s next nonce for `owner` and stores the one after it. An unused lane
    /// starts at a random point.
    pub async fn take_nonce(&self, store: &impl Store, owner: Address, lane: Lane) -> Result<u128, Error> {
        let held = store.get(owner, Slot::Nonce(lane)).await?;
        let next = match held.as_deref().and_then(|bytes| <[u8; 16]>::try_from(bytes).ok()) {
            Some(bytes) => u128::from_be_bytes(bytes),
            None => nonce::first(self.random::<3>()?),
        };
        store.put(owner, Slot::Nonce(lane), next.saturating_add(1).to_be_bytes().to_vec(), None).await?;
        Ok(next)
    }

    /// Moves `owner`'s stored lanes past the latest nonces the relayer has seen the wallet use
    /// (the account's [`recent_nonces`](crate::account::Account::recent_nonces)), so a nonce
    /// another device or tab spent is not handed out again. Only a lane with a stored next nonce
    /// moves, and only within [`nonce::CATCH_UP_WINDOW`] of it ([`nonce::caught_up`]). Call it with
    /// each snapshot. Returns the lanes it moved.
    pub async fn catch_up_nonces(
        &self,
        store: &impl Store,
        owner: Address,
        recent: &RecentNonces,
    ) -> Result<Vec<Lane>, Error> {
        let mut moved = Vec::new();
        for lane in [Lane::Exchange, Lane::Staking] {
            let Some(seen) = recent.get(lane) else { continue };
            let held = store.get(owner, Slot::Nonce(lane)).await?;
            let Some(next) =
                held.as_deref().and_then(|bytes| <[u8; 16]>::try_from(bytes).ok()).map(u128::from_be_bytes)
            else {
                continue;
            };
            if let Some(after) = nonce::caught_up(next, seen) {
                store.put(owner, Slot::Nonce(lane), after.to_be_bytes().to_vec(), None).await?;
                moved.push(lane);
            }
        }
        Ok(moved)
    }

    /// Has `wallet` sign `intent` and checks the signature recovers to `owner`.
    pub(crate) async fn wallet_sign(
        &self,
        wallet: &impl Wallet,
        owner: Address,
        intent: Intent,
        purpose: Purpose,
    ) -> Result<Signed, Error> {
        if wallet.address() != owner {
            return Err(Error::WrongWallet { wallet: wallet.address().to_hex(), owner: owner.to_hex() });
        }
        if intent.user() != owner {
            return Err(Error::WrongWallet { wallet: intent.user().to_hex(), owner: owner.to_hex() });
        }
        let request = SignRequest { purpose, json: intent.wallet_json(), digest: intent.digest() };
        let signature = wallet.sign(&request).await?;
        Signed::recover(intent, signature, owner).map_err(|e| Error::Signature(e.to_string()))
    }

    /// Posts `signed` once. An answer that does not keep it is an error.
    pub(crate) async fn submit(&self, signed: &Signed) -> Result<Accepted, Error> {
        let accepted = self.relayer().submit(signed).await?;
        if accepted.kept() { Ok(accepted) } else { Err(Error::NotKept(accepted)) }
    }
}

#[cfg(test)]
#[path = "tests/submit.rs"]
mod tests;
