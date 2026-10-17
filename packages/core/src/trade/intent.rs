//! Intents signed by a wallet or its session key. Each builds its EIP-712 struct, domain,
//! digest and wallet typed data, and once signed, the relayer payload and intent id.
//! The session key signs opens, closes and cancels. The wallet signs everything else.

use df_hyperliquid_core::utils::{Address, Json, hex};

use df_hyperliquid_core::utils::sign::{Domain, Field, TypedStruct, keccak256};

use df_hyperliquid_core::account::Signature;

use df_hyperliquid_core::utils::sign::eip712::{uint256, word_decimal};

use crate::client::deployment::{CANCEL_DOMAIN, EXCHANGE_DOMAIN, STAKING_DOMAIN};

/// How long an intent stays valid after it is made, in seconds.
pub const INTENT_TTL_SECS: u64 = 3600;
/// Highest leverage any market allows. A market's own cap may be lower.
pub const MAX_LEVERAGE: u32 = 1000;
/// Most position ids one close intent may name.
pub const MAX_CLOSE_IDS: usize = 12;
/// Smallest withdrawal: 10 USDC, in 6 decimals.
pub const MIN_WITHDRAWAL_USDC: u128 = 10_000_000;
/// Relayer withdrawal route to the wallet's own HyperCore spot account. The only route that
/// needs no second signature.
pub const HYPERCORE_ROUTE: &str = "hypercore";

/// The keccak256 of the intent id tag, which starts every intent id preimage.
#[must_use]
pub fn intent_id_tag() -> [u8; 32] {
    keccak256(b"papertrade:intent-id:v2")
}

/// Deadline of an intent made at `now_ms`: one hour later, in Unix seconds.
#[must_use]
pub const fn deadline(now_ms: u64) -> u64 {
    now_ms / 1000 + INTENT_TTL_SECS
}

/// Which key signs an intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The wallet itself, through `eth_signTypedData_v4`.
    Wallet,
    /// The wallet's registered session key.
    Session,
}

/// Why an intent was not built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("leverage {0} is outside 1 to {MAX_LEVERAGE}")]
    Leverage(u32),
    #[error("a position's margin must be more than zero")]
    NoMargin,
    #[error("a close names 1 to {MAX_CLOSE_IDS} positions, not {0}")]
    CloseCount(usize),
    #[error("a close names a position twice")]
    DuplicatePosition,
    #[error("a withdrawal moves at least 10 USDC")]
    WithdrawalTooSmall,
    #[error("the amount must be more than zero")]
    NoAmount,
    #[error("a session key is a fresh address, not the wallet or zero")]
    BadSessionKey,
    #[error("the session key expires before the registration does")]
    ExpiresFirst,
}

/// Authorizes a session key for the wallet until `expiry` (Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterSessionKey {
    pub user: Address,
    pub key: Address,
    pub expiry: u64,
    /// Always the zero address.
    pub fee_address: Address,
    pub nonce: u128,
    pub deadline: u64,
}

impl RegisterSessionKey {
    /// Refuses a key that is zero or the wallet, or that expires before `deadline`.
    pub fn new(user: Address, key: Address, expiry: u64, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if key == user || key.is_zero() {
            return Err(Refusal::BadSessionKey);
        }
        if expiry <= deadline {
            return Err(Refusal::ExpiresFirst);
        }
        Ok(RegisterSessionKey { user, key, expiry, fee_address: Address::ZERO, nonce, deadline })
    }
}

/// Opens a position. `size` is the margin in collateral units (18 decimals), drawn from the
/// queued balance when `use_debt` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPosition {
    pub user: Address,
    pub instrument_id: u32,
    pub is_long: bool,
    pub size: u128,
    pub leverage: u32,
    pub use_debt: bool,
    pub nonce: u128,
    pub deadline: u64,
}

impl OpenPosition {
    /// Refuses leverage outside 1 to [`MAX_LEVERAGE`] and a zero margin.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user: Address,
        instrument_id: u32,
        is_long: bool,
        size: u128,
        leverage: u32,
        use_debt: bool,
        nonce: u128,
        deadline: u64,
    ) -> Result<Self, Refusal> {
        if leverage == 0 || leverage > MAX_LEVERAGE {
            return Err(Refusal::Leverage(leverage));
        }
        if size == 0 {
            return Err(Refusal::NoMargin);
        }
        Ok(OpenPosition { user, instrument_id, is_long, size, leverage, use_debt, nonce, deadline })
    }
}

/// Closes whole positions. A position is never closed in part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Close {
    pub user: Address,
    position_ids: Vec<[u8; 32]>,
    pub nonce: u128,
    pub deadline: u64,
}

impl Close {
    /// Positions by id, 1 to [`MAX_CLOSE_IDS`] of them, none twice.
    pub fn new(user: Address, ids: &[u128], nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        Close::of_words(user, ids.iter().map(|id| uint256(*id)).collect(), nonce, deadline)
    }

    /// Positions by their `uint256` words, for an id past 128 bits.
    pub fn of_words(user: Address, words: Vec<[u8; 32]>, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if words.is_empty() || words.len() > MAX_CLOSE_IDS {
            return Err(Refusal::CloseCount(words.len()));
        }
        for (index, word) in words.iter().enumerate() {
            if words[..index].contains(word) {
                return Err(Refusal::DuplicatePosition);
            }
        }
        Ok(Close { user, position_ids: words, nonce, deadline })
    }

    /// The position ids as `uint256` words.
    #[must_use]
    pub fn position_ids(&self) -> &[[u8; 32]] {
        &self.position_ids
    }
}

/// Moves `amount` USDC (6 decimals) of the available balance to the wallet's
/// HyperCore spot account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawToCore {
    pub user: Address,
    pub amount: u128,
    pub nonce: u128,
    pub deadline: u64,
}

impl WithdrawToCore {
    /// Refuses less than [`MIN_WITHDRAWAL_USDC`].
    pub fn new(user: Address, amount: u128, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if amount < MIN_WITHDRAWAL_USDC {
            return Err(Refusal::WithdrawalTooSmall);
        }
        Ok(WithdrawToCore { user, amount, nonce, deadline })
    }
}

/// Stakes or unstakes `amount` PAPER (18 decimals). Both take effect at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staking {
    pub user: Address,
    pub amount: u128,
    pub nonce: u128,
    pub deadline: u64,
}

impl Staking {
    /// Refuses a zero amount.
    pub fn new(user: Address, amount: u128, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if amount == 0 {
            return Err(Refusal::NoAmount);
        }
        Ok(Staking { user, amount, nonce, deadline })
    }
}

/// Claims every distributed USDC reward into the Exchange balance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub user: Address,
    pub nonce: u128,
    pub deadline: u64,
}

/// Cancels a queued intent before the relayer sends it. Signed by the session key under
/// [`CANCEL_DOMAIN`], with no nonce and a `uint256` deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelIntent {
    pub user: Address,
    pub intent_id: [u8; 32],
    pub deadline: u64,
}

/// One intent, of any kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    RegisterSessionKey(RegisterSessionKey),
    OpenPosition(OpenPosition),
    Close(Close),
    WithdrawToCore(WithdrawToCore),
    Stake(Staking),
    Unstake(Staking),
    Claim(Claim),
    CancelIntent(CancelIntent),
}

impl Intent {
    /// The EIP-712 primary type.
    #[must_use]
    pub const fn primary_type(&self) -> &'static str {
        match self {
            Intent::RegisterSessionKey(_) => "RegisterSessionKey",
            Intent::OpenPosition(_) => "OpenPosition",
            Intent::Close(_) => "Close",
            Intent::WithdrawToCore(_) => "WithdrawToCore",
            Intent::Stake(_) => "Stake",
            Intent::Unstake(_) => "Unstake",
            Intent::Claim(_) => "Claim",
            Intent::CancelIntent(_) => "CancelIntent",
        }
    }

    /// The `action` of its submission. `None` for a cancel, which posts to its own path.
    #[must_use]
    pub const fn action(&self) -> Option<&'static str> {
        match self {
            Intent::RegisterSessionKey(_) => Some("registerSessionKey"),
            Intent::OpenPosition(_) => Some("openPosition"),
            Intent::Close(_) => Some("closePositions"),
            Intent::WithdrawToCore(_) => Some("withdrawToCore"),
            Intent::Stake(_) => Some("paperStake"),
            Intent::Unstake(_) => Some("paperUnstake"),
            Intent::Claim(_) => Some("paperClaim"),
            Intent::CancelIntent(_) => None,
        }
    }

    /// Who signs it.
    #[must_use]
    pub const fn key(&self) -> Key {
        match self {
            Intent::OpenPosition(_) | Intent::Close(_) | Intent::CancelIntent(_) => Key::Session,
            Intent::RegisterSessionKey(_)
            | Intent::WithdrawToCore(_)
            | Intent::Stake(_)
            | Intent::Unstake(_)
            | Intent::Claim(_) => Key::Wallet,
        }
    }

    /// The domain it is signed under.
    #[must_use]
    pub const fn domain(&self) -> Domain {
        match self {
            Intent::Stake(_) | Intent::Unstake(_) | Intent::Claim(_) => STAKING_DOMAIN,
            Intent::CancelIntent(_) => CANCEL_DOMAIN,
            _ => EXCHANGE_DOMAIN,
        }
    }

    /// The wallet the intent acts for.
    #[must_use]
    pub const fn user(&self) -> Address {
        match self {
            Intent::RegisterSessionKey(intent) => intent.user,
            Intent::OpenPosition(intent) => intent.user,
            Intent::Close(intent) => intent.user,
            Intent::WithdrawToCore(intent) => intent.user,
            Intent::Stake(intent) | Intent::Unstake(intent) => intent.user,
            Intent::Claim(intent) => intent.user,
            Intent::CancelIntent(intent) => intent.user,
        }
    }

    /// Unix seconds after which the relayer drops it.
    #[must_use]
    pub const fn deadline(&self) -> u64 {
        match self {
            Intent::RegisterSessionKey(intent) => intent.deadline,
            Intent::OpenPosition(intent) => intent.deadline,
            Intent::Close(intent) => intent.deadline,
            Intent::WithdrawToCore(intent) => intent.deadline,
            Intent::Stake(intent) | Intent::Unstake(intent) => intent.deadline,
            Intent::Claim(intent) => intent.deadline,
            Intent::CancelIntent(intent) => intent.deadline,
        }
    }

    /// The nonce it spends, if it spends one.
    #[must_use]
    pub const fn nonce(&self) -> Option<u128> {
        match self {
            Intent::RegisterSessionKey(intent) => Some(intent.nonce),
            Intent::OpenPosition(intent) => Some(intent.nonce),
            Intent::Close(intent) => Some(intent.nonce),
            Intent::WithdrawToCore(intent) => Some(intent.nonce),
            Intent::Stake(intent) | Intent::Unstake(intent) => Some(intent.nonce),
            Intent::Claim(intent) => Some(intent.nonce),
            Intent::CancelIntent(_) => None,
        }
    }

    /// The EIP-712 struct, fields in the order the contract hashes them.
    #[must_use]
    pub fn typed(&self) -> TypedStruct<'_> {
        let typed = TypedStruct::new(self.primary_type());
        match self {
            Intent::RegisterSessionKey(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("key", Field::Address(intent.key))
                .field("expiry", Field::Uint64(intent.expiry))
                .field("feeAddress", Field::Address(intent.fee_address))
                .field("nonce", Field::Uint256(uint256(intent.nonce)))
                .field("deadline", Field::Uint64(intent.deadline)),
            Intent::OpenPosition(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("instrumentId", Field::Uint32(intent.instrument_id))
                .field("isLong", Field::Bool(intent.is_long))
                .field("size", Field::Uint128(intent.size))
                .field("leverage", Field::Uint32(intent.leverage))
                .field("useDebt", Field::Bool(intent.use_debt))
                .field("nonce", Field::Uint256(uint256(intent.nonce)))
                .field("deadline", Field::Uint64(intent.deadline)),
            Intent::Close(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("positionIds", Field::Uint256Array(&intent.position_ids))
                .field("nonce", Field::Uint256(uint256(intent.nonce)))
                .field("deadline", Field::Uint64(intent.deadline)),
            Intent::WithdrawToCore(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("amount", Field::Uint256(uint256(intent.amount)))
                .field("nonce", Field::Uint256(uint256(intent.nonce)))
                .field("deadline", Field::Uint64(intent.deadline)),
            Intent::Stake(intent) | Intent::Unstake(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("amount", Field::Uint256(uint256(intent.amount)))
                .field("nonce", Field::Uint256(uint256(intent.nonce)))
                .field("deadline", Field::Uint64(intent.deadline)),
            Intent::Claim(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("nonce", Field::Uint256(uint256(intent.nonce)))
                .field("deadline", Field::Uint64(intent.deadline)),
            Intent::CancelIntent(intent) => typed
                .field("user", Field::Address(intent.user))
                .field("intentId", Field::Bytes32(intent.intent_id))
                .field("deadline", Field::Uint256(uint256(u128::from(intent.deadline)))),
        }
    }

    /// The EIP-712 digest the key signs.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        self.typed().digest(&self.domain())
    }

    /// The typed data JSON `eth_signTypedData_v4` takes.
    #[must_use]
    pub fn wallet_json(&self) -> String {
        self.typed().wallet_json(&self.domain())
    }

    /// Attaches a signature made elsewhere, trusting that `signer` made it. With the `sign`
    /// feature, `Signed::recover` verifies the signer instead.
    #[must_use]
    pub fn signed(self, signature: Signature, signer: Address) -> Signed {
        Signed { intent: self, signature, signer }
    }
}

/// The intent id of `digest` signed by `signer`: `keccak256(tag || digest || signer)`, with
/// the tag from [`intent_id_tag`].
#[must_use]
pub fn intent_id(digest: &[u8; 32], signer: Address) -> [u8; 32] {
    keccak256(&[&intent_id_tag()[..], &digest[..], &signer.bytes()[..]].concat())
}

/// An intent with its signature and signer, ready for the relayer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    intent: Intent,
    signature: Signature,
    signer: Address,
}

/// Why a signature was not taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadSignature {
    #[error(transparent)]
    Curve(#[from] df_hyperliquid_core::account::SignError),
    #[error("signed by {recovered}, not {expected}")]
    WrongSigner { expected: Address, recovered: Address },
}

impl Signed {
    /// Recovers the signer and refuses any signer other than `expected`.
    pub fn recover(intent: Intent, signature: Signature, expected: Address) -> Result<Signed, BadSignature> {
        let recovered = df_hyperliquid_core::account::recover(&intent.digest(), &signature)?;
        if recovered != expected {
            return Err(BadSignature::WrongSigner { expected, recovered });
        }
        Ok(Signed { intent, signature, signer: recovered })
    }

    #[must_use]
    pub const fn intent(&self) -> &Intent {
        &self.intent
    }

    #[must_use]
    pub const fn signature(&self) -> &Signature {
        &self.signature
    }

    #[must_use]
    pub const fn signer(&self) -> Address {
        self.signer
    }

    /// The intent id the relayer and the account stream report.
    #[must_use]
    pub fn intent_id(&self) -> [u8; 32] {
        intent_id(&self.intent.digest(), self.signer)
    }

    /// The `signed` JSON object: the struct's fields (numbers as decimal strings, addresses
    /// and hashes lowercase), then `v` ("27" or "28"), `r` and `s`.
    #[must_use]
    pub fn payload(&self) -> Json {
        let typed = self.intent.typed();
        let mut fields: Vec<(String, Json)> =
            typed.fields().iter().map(|(name, value)| ((*name).to_owned(), payload_value(value))).collect();
        fields.push(("v".to_owned(), Json::String(self.signature.v().to_string())));
        fields.push(("r".to_owned(), Json::String(self.signature.r_hex())));
        fields.push(("s".to_owned(), Json::String(self.signature.s_hex())));
        Json::Object(fields)
    }

    /// Where it is posted: `/intents`, or `/intents/<id>/cancel` for a cancel.
    #[must_use]
    pub fn path(&self) -> String {
        match &self.intent {
            Intent::CancelIntent(cancel) => format!("/intents/{}/cancel", hex::encode_prefixed(&cancel.intent_id)),
            _ => "/intents".to_owned(),
        }
    }

    /// The JSON body posted to [`Signed::path`]. `{action, signed}` plus the HyperCore route
    /// for a withdrawal, or for a cancel the payload without `intentId`.
    #[must_use]
    pub fn body(&self) -> String {
        let payload = self.payload();
        let Some(action) = self.intent.action() else {
            let Json::Object(fields) = payload else {
                return payload.compact();
            };
            return Json::Object(fields.into_iter().filter(|(name, _)| name != "intentId").collect()).compact();
        };
        let mut body = vec![("action".to_owned(), Json::String(action.to_owned())), ("signed".to_owned(), payload)];
        if matches!(self.intent, Intent::WithdrawToCore(_)) {
            body.push((
                "withdrawal".to_owned(),
                Json::Object(vec![("route".to_owned(), Json::String(HYPERCORE_ROUTE.to_owned()))]),
            ));
        }
        Json::Object(body).compact()
    }
}

fn payload_value(value: &Field<'_>) -> Json {
    match value {
        Field::String(text) => Json::String((*text).to_owned()),
        Field::Address(address) => Json::String(address.to_hex()),
        Field::Bytes32(bytes) => Json::String(hex::encode_prefixed(bytes)),
        Field::Bool(flag) => Json::Bool(*flag),
        Field::Uint32(number) => Json::String(number.to_string()),
        Field::Uint64(number) => Json::String(number.to_string()),
        Field::Uint128(number) => Json::String(number.to_string()),
        Field::Uint256(word) => Json::String(word_decimal(word)),
        Field::Uint256Array(words) => Json::Array(words.iter().map(|word| Json::String(word_decimal(word))).collect()),
    }
}

#[cfg(test)]
#[path = "tests/intent.rs"]
mod tests;
