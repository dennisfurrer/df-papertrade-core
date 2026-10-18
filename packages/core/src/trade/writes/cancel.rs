//! Cancelling a queued intent before the relayer sends it, signed by the session key.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::Address;

use crate::account::Account;
use crate::account::store::Store;
use crate::client::{Accepted, Client, Error};
use crate::trade::{CancelIntent, Intent, deadline};

impl<H: Http + Clone> Client<H> {
    /// Cancels a queued intent (`intent_id`) before the relayer sends it. The relayer's answer is
    /// returned as is, without checking that it kept the cancel.
    pub async fn cancel(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
        intent_id: [u8; 32],
    ) -> Result<Accepted, Error> {
        let key = self.session_key(store, owner, account).await?;
        let cancel = CancelIntent { user: owner, intent_id, deadline: deadline(self.now_ms()) };
        let signed =
            key.sign(Intent::CancelIntent(cancel), self.now_ms()).map_err(|e| Error::Signature(e.to_string()))?;
        Ok(self.relayer().submit(&signed).await?)
    }
}
