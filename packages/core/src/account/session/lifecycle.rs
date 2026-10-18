//! The client's side of the session key: reading and keeping the stored record, making a key,
//! having the wallet register it, and handing it out as a signer.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::Address;
use zeroize::Zeroizing;

use super::{KeyState, SessionRecord, key_state};
use crate::account::Account;
use crate::account::session_key::{self, RENEW_BEFORE_MS, SESSION_TTL_MS, SessionKey};
use crate::account::store::{Slot, Store};
use crate::account::wallet::{Purpose, Wallet};
use crate::client::{Client, Error};
use crate::trade::Intent;
use crate::utils::Lane;

impl<H: Http + Clone> Client<H> {
    /// The session key stored for `owner`, if readable.
    pub async fn session(&self, store: &impl Store, owner: Address) -> Result<Option<SessionRecord>, Error> {
        let bytes = store.get(owner, Slot::Session).await?.map(Zeroizing::new);
        Ok(bytes.as_deref().and_then(|bytes| SessionRecord::from_bytes(bytes)))
    }

    async fn keep(&self, store: &impl Store, owner: Address, record: &SessionRecord) -> Result<(), Error> {
        let ttl = record.held_until_ms.saturating_sub(self.now_ms());
        Ok(store.put(owner, Slot::Session, record.to_bytes().to_vec(), Some(ttl)).await?)
    }

    /// Where `owner`'s stored key stands, given the account as last streamed.
    pub async fn key_state(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
    ) -> Result<KeyState, Error> {
        Ok(key_state(self.session(store, owner).await?.as_ref(), account, self.now_ms()))
    }

    /// Makes and stores a session key for `owner`, unless the stored one is still usable.
    pub async fn new_session(&self, store: &impl Store, owner: Address) -> Result<SessionRecord, Error> {
        let now = self.now_ms();
        if let Some(record) = self.session(store, owner).await?
            && record.held_until_ms > now.saturating_add(RENEW_BEFORE_MS)
        {
            return Ok(record);
        }
        let record = SessionRecord {
            secret: Zeroizing::new(self.random::<32>()?),
            held_until_ms: now.saturating_add(SESSION_TTL_MS),
            registered_at_ms: None,
        };
        // a secret that is not a valid scalar (about 1 in 2^128) fails here and is redrawn next call
        record.address()?;
        self.keep(store, owner, &record).await?;
        Ok(record)
    }

    /// Has the wallet register the stored session key (made first if none is usable) and posts
    /// the registration to the relayer once.
    pub async fn register(&self, store: &impl Store, wallet: &impl Wallet) -> Result<SessionRecord, Error> {
        let owner = wallet.address();
        let mut record = self.new_session(store, owner).await?;
        let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
        let registration =
            session_key::registration(owner, record.address()?, record.held_until_ms, nonce, self.now_ms())?;
        let signed = self
            .wallet_sign(wallet, owner, Intent::RegisterSessionKey(registration), Purpose::RegisterSessionKey)
            .await?;
        self.submit(&signed).await?;
        record.registered_at_ms = Some(self.now_ms());
        self.keep(store, owner, &record).await?;
        Ok(record)
    }

    /// Enables trading: makes a session key if needed and registers it unless it is ready. At
    /// most one wallet prompt.
    pub async fn enable(
        &self,
        store: &impl Store,
        wallet: &impl Wallet,
        account: Option<&Account>,
    ) -> Result<KeyState, Error> {
        if self.key_state(store, wallet.address(), account).await? == KeyState::Ready {
            return Ok(KeyState::Ready);
        }
        self.register(store, wallet).await?;
        Ok(KeyState::Ready)
    }

    /// `owner`'s session key as a signer, or [`Error::NeedsKey`] with the step to take first.
    pub async fn session_key(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
    ) -> Result<SessionKey, Error> {
        let record = self.session(store, owner).await?;
        match key_state(record.as_ref(), account, self.now_ms()) {
            KeyState::Ready => record.ok_or(Error::NeedsKey(KeyState::Missing))?.signer(owner),
            other => Err(Error::NeedsKey(other)),
        }
    }
}
