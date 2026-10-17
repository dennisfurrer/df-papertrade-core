//! The session key's lifecycle: made from the caller's randomness, kept in the [`Store`] for
//! 30 days, registered once by the wallet, and renewed near its end. It signs opens, closes
//! and cancels without a wallet prompt, and can never withdraw, stake or claim.

use df_hyperliquid_core::utils::Address;

use df_hyperliquid_core::client::Http;

use crate::account::Account;
use crate::account::session_key::{self, RENEW_BEFORE_MS, SESSION_TTL_MS, SessionKey, Status};
use crate::trade::{Intent, Signed};
use crate::utils::{Lane, nonce};
use df_hyperliquid_core::account::LocalKey;
use zeroize::Zeroizing;

use crate::account::store::{Slot, Store};
use crate::account::wallet::{Purpose, SignRequest, Wallet};
use crate::client::{Client, Error};

/// How long the indexer may lag behind an accepted registration before the key counts as not
/// registered, in ms.
pub const INDEXER_GRACE_MS: u64 = 120_000;

/// A stored session key. Stored as 48 bytes: the 32-byte secret, then `held_until_ms` and
/// `registered_at_ms` as big-endian u64 (0 when not registered).
#[derive(Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub secret: Zeroizing<[u8; 32]>,
    /// When the key stops being used, in ms since the epoch.
    pub held_until_ms: u64,
    /// When the relayer accepted its registration, in ms since the epoch.
    pub registered_at_ms: Option<u64>,
}

impl std::fmt::Debug for SessionRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRecord")
            .field("held_until_ms", &self.held_until_ms)
            .field("registered_at_ms", &self.registered_at_ms)
            .finish_non_exhaustive()
    }
}

impl SessionRecord {
    /// The 48-byte stored form.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(self.secret.to_vec());
        bytes.extend_from_slice(&self.held_until_ms.to_be_bytes());
        bytes.extend_from_slice(&self.registered_at_ms.unwrap_or(0).to_be_bytes());
        bytes
    }

    /// Parses what [`SessionRecord::to_bytes`] wrote, or `None` for anything else.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<SessionRecord> {
        if bytes.len() != 48 {
            return None;
        }
        let mut secret = Zeroizing::new([0u8; 32]);
        secret.copy_from_slice(&bytes[..32]);
        let word = |at: usize| {
            let mut eight = [0u8; 8];
            eight.copy_from_slice(&bytes[at..at + 8]);
            u64::from_be_bytes(eight)
        };
        let registered = word(40);
        Some(SessionRecord {
            secret,
            held_until_ms: word(32),
            registered_at_ms: (registered > 0).then_some(registered),
        })
    }

    /// The key's address.
    pub fn address(&self) -> Result<Address, Error> {
        LocalKey::from_bytes(&self.secret)
            .map(|key| key.address())
            .map_err(|e| Error::Signature(format!("the kept session key: {e}")))
    }

    /// The key as a signer for `wallet`.
    pub fn signer(&self, wallet: Address) -> Result<SessionKey, Error> {
        SessionKey::from_secret(&self.secret, wallet, self.held_until_ms)
            .map_err(|e| Error::Signature(format!("the kept session key: {e}")))
    }
}

/// Whether the stored key can sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// No key, or too near its end to sign. Make one.
    Missing,
    /// Stored but not registered, or its registration did not land. The wallet registers it.
    Unregistered,
    /// Signs now.
    Ready,
}

/// The key's state at `now_ms`. Under [`RENEW_BEFORE_MS`] left (about an hour, the life of a
/// signed intent) is [`KeyState::Missing`]. Otherwise the account's key list decides, or the
/// record when there is no account or the indexer is under [`INDEXER_GRACE_MS`] past registration.
#[must_use]
pub fn key_state(record: Option<&SessionRecord>, account: Option<&Account>, now_ms: u64) -> KeyState {
    let Some(record) = record else {
        return KeyState::Missing;
    };
    if record.held_until_ms <= now_ms.saturating_add(RENEW_BEFORE_MS) {
        return KeyState::Missing;
    }
    let Some(account) = account else {
        return if record.registered_at_ms.is_some() { KeyState::Ready } else { KeyState::Unregistered };
    };
    let Ok(address) = record.address() else {
        return KeyState::Missing;
    };
    match session_key::status(
        address,
        record.held_until_ms,
        &account.session_keys,
        account.indexed_through_timestamp,
        now_ms,
    ) {
        Status::Ready { .. } => KeyState::Ready,
        // accepted by the relayer, not yet indexed
        Status::Unregistered
            if record.registered_at_ms.is_some_and(|at| {
                account.indexed_through_timestamp.saturating_mul(1000) < at.saturating_add(INDEXER_GRACE_MS)
            }) =>
        {
            KeyState::Ready
        }
        Status::Unregistered | Status::Expiring { .. } => KeyState::Unregistered,
    }
}

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
    pub(crate) async fn submit(&self, signed: &Signed) -> Result<crate::client::Accepted, Error> {
        let accepted = self.relayer().submit(signed).await?;
        if accepted.kept() { Ok(accepted) } else { Err(Error::NotKept(accepted)) }
    }
}
