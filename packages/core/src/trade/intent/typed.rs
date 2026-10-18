//! [`Intent`], one intent of any kind: which key signs it, the domain it is signed under, and
//! its EIP-712 struct, digest and wallet typed data.

use df_hyperliquid_core::utils::Address;
use df_hyperliquid_core::utils::sign::eip712::uint256;
use df_hyperliquid_core::utils::sign::{Domain, Field, TypedStruct};

use super::{CancelIntent, Claim, Close, Key, OpenPosition, RegisterSessionKey, Staking, WithdrawToCore};
use crate::client::deployment::{CANCEL_DOMAIN, EXCHANGE_DOMAIN, STAKING_DOMAIN};

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
                .field("positionIds", Field::Uint256Array(intent.position_ids()))
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
}

#[cfg(test)]
#[path = "tests/typed.rs"]
mod tests;
