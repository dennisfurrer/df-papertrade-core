//! Closing whole positions by id, signed by the session key, one intent per twelve positions.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::Address;

use crate::account::Account;
use crate::account::store::Store;
use crate::client::{Accepted, Client, Error};
use crate::trade::intent::MAX_CLOSE_IDS;
use crate::trade::{Close, Intent, deadline};
use crate::utils::Lane;

impl<H: Http + Clone> Client<H> {
    /// Closes whole positions by id, at most 12 per intent, with one answer per intent. A failure
    /// after the first leaves the rest unsent.
    pub async fn close(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
        ids: &[u128],
    ) -> Result<Vec<Accepted>, Error> {
        if ids.is_empty() {
            return Err(Error::Invalid("no positions to close".into()));
        }
        let key = self.session_key(store, owner, account).await?;
        let mut answers = Vec::new();
        for chunk in ids.chunks(MAX_CLOSE_IDS) {
            let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
            let close = Close::new(owner, chunk, nonce, deadline(self.now_ms()))?;
            let signed = key.sign(Intent::Close(close), self.now_ms()).map_err(|e| Error::Signature(e.to_string()))?;
            answers.push(self.submit(&signed).await?);
        }
        Ok(answers)
    }
}
