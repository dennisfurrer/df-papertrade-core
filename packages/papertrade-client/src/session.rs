//! The session key's life, as the venue's app lives it:
//!
//! 1. made: 32 bytes from the caller's randomness, kept in the [`Store`] for
//!    its 30 days ([`SessionRecord`]);
//! 2. registered: the wallet signs `RegisterSessionKey` on HyperEVM, and it
//!    is posted to the relayer once. From then on the key signs opens, closes
//!    and cancels with no wallet prompt; it can never withdraw, stake or
//!    claim;
//! 3. renewed: a key with less than an hour left (an intent it signs lives
//!    an hour) counts as none, and enabling makes and registers another.
//!
//! Whether the key signs now ([`key_state`]) is the account stream's to say
//! once it has spoken. Before its first snapshot the record answers
//! (registered once the relayer took it). A key the relayer took that the
//! stream still does not list once its indexer is [`INDEXER_GRACE_MS`] past
//! the acceptance did not land, and wants registering again.

use hl_core::{Address, Http, LocalKey};
use hl_papertrade::session::{self, RENEW_BEFORE_MS, SESSION_TTL_MS, SessionKey, Status};
use hl_papertrade::{Account, Intent, Lane, Signed, nonce};
use zeroize::Zeroizing;

use crate::store::{Slot, Store};
use crate::wallet::{Purpose, SignRequest, Wallet};
use crate::{Client, Error};

/// How long the indexer may lag behind an accepted registration before the
/// key counts as not registered, ms.
pub const INDEXER_GRACE_MS: u64 = 120_000;

/// The session key as kept: its secret, until when it is held, and when the
/// relayer took its registration. As bytes: the 32 secret bytes, then the two
/// times in milliseconds big-endian (0 for not registered), 48 in all.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub secret: Zeroizing<[u8; 32]>,
    pub held_until_ms: u64,
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
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(self.secret.to_vec());
        bytes.extend_from_slice(&self.held_until_ms.to_be_bytes());
        bytes.extend_from_slice(&self.registered_at_ms.unwrap_or(0).to_be_bytes());
        bytes
    }

    /// From what [`SessionRecord::to_bytes`] wrote; `None` for anything else.
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

    /// The key, signing for `wallet`.
    pub fn signer(&self, wallet: Address) -> Result<SessionKey, Error> {
        SessionKey::from_secret(&self.secret, wallet, self.held_until_ms)
            .map_err(|e| Error::Signature(format!("the kept session key: {e}")))
    }
}

/// Where the kept key stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// None, or too near its end to sign: make one.
    Missing,
    /// Kept, not registered (or its registration did not land): the wallet
    /// registers it.
    Unregistered,
    /// Signs now.
    Ready,
}

/// The key's state from the record and, once the stream has spoken, the
/// account, at `now_ms`.
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
    match session::status(
        address,
        record.held_until_ms,
        &account.session_keys,
        account.indexed_through_timestamp,
        now_ms,
    ) {
        Status::Ready { .. } => KeyState::Ready,
        // the relayer took it and the indexer has not caught up yet: as good as registered
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
    /// The session key kept for `owner`, if a readable one.
    pub async fn session(&self, store: &impl Store, owner: Address) -> Result<Option<SessionRecord>, Error> {
        let bytes = store.get(owner, Slot::Session).await?.map(Zeroizing::new);
        Ok(bytes.as_deref().and_then(|bytes| SessionRecord::from_bytes(bytes)))
    }

    async fn keep(&self, store: &impl Store, owner: Address, record: &SessionRecord) -> Result<(), Error> {
        let ttl = record.held_until_ms.saturating_sub(self.now_ms());
        Ok(store.put(owner, Slot::Session, record.to_bytes().to_vec(), Some(ttl)).await?)
    }

    /// Where `owner`'s kept key stands, given the account as last streamed.
    pub async fn key_state(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
    ) -> Result<KeyState, Error> {
        Ok(key_state(self.session(store, owner).await?.as_ref(), account, self.now_ms()))
    }

    /// A new session key for `owner`, kept, unless the kept one still has its
    /// time.
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
        // a secret that is not a curve scalar (about one in 2^128) is drawn again by the next call
        record.address()?;
        self.keep(store, owner, &record).await?;
        Ok(record)
    }

    /// The wallet registers the kept session key (one made first if none is
    /// usable), and the relayer is asked once to take it.
    pub async fn register(&self, store: &impl Store, wallet: &impl Wallet) -> Result<SessionRecord, Error> {
        let owner = wallet.address();
        let mut record = self.new_session(store, owner).await?;
        let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
        let registration = session::registration(owner, record.address()?, record.held_until_ms, nonce, self.now_ms())?;
        let signed = self
            .wallet_sign(wallet, owner, Intent::RegisterSessionKey(registration), Purpose::RegisterSessionKey)
            .await?;
        self.submit(&signed).await?;
        record.registered_at_ms = Some(self.now_ms());
        self.keep(store, owner, &record).await?;
        Ok(record)
    }

    /// Enables trading for the wallet: makes the session key if none is
    /// usable and registers it unless it signs already. One wallet prompt at
    /// most.
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

    /// `owner`'s session key, ready to sign; what to do first when it is not.
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

    /// Hands out `lane`'s next nonce for `owner` and keeps the one after it. A
    /// lane never used here starts where the venue's app starts one: at a
    /// random point.
    pub async fn take_nonce(&self, store: &impl Store, owner: Address, lane: Lane) -> Result<u128, Error> {
        let held = store.get(owner, Slot::Nonce(lane)).await?;
        let next = match held.as_deref().and_then(|bytes| <[u8; 16]>::try_from(bytes).ok()) {
            Some(bytes) => u128::from_be_bytes(bytes),
            None => nonce::first(self.random::<3>()?),
        };
        store.put(owner, Slot::Nonce(lane), next.saturating_add(1).to_be_bytes().to_vec(), None).await?;
        Ok(next)
    }

    /// Has `wallet`, the account's owner, sign `intent`, and checks the
    /// signature recovers to it.
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

    /// Posts `signed` once; an answer that does not keep it is refused.
    pub(crate) async fn submit(&self, signed: &Signed) -> Result<hl_papertrade::Accepted, Error> {
        let accepted = self.relayer().submit(signed).await?;
        if accepted.kept() { Ok(accepted) } else { Err(Error::NotKept(accepted)) }
    }
}
