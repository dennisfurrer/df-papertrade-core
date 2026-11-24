//! Contract reads on HyperEVM: the deposit address, the win fee, the payout curve in force,
//! staking rewards, builder fees and the builder a session key names. Each is a
//! pure `eth_call` ([`Read`]) run at `latest`, the only block mainnet's RPC serves. The book's
//! BBO is HyperCore's own precompile, read through
//! [`df_hyperliquid_core::client::evm::precompiles::bbo`].

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::client::evm::{AbiError, BlockTag, CallRequest, Function, Rpc, RpcError, Token};
use df_hyperliquid_core::utils::Address;

use crate::client::deployment::{
    BUILDER_REGISTRY, DEPOSIT_PROXY_FACTORY, EXCHANGE, PAPER_STAKING, SESSION_KEY_MANAGER,
};

/// One contract read: the call and the decoder for its answer.
pub struct Read<T> {
    pub to: Address,
    pub data: Vec<u8>,
    decode: fn(&[Token]) -> Option<T>,
    function: Function,
}

impl<T> std::fmt::Debug for Read<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Read").field("to", &self.to).field("function", &self.function.signature()).finish()
    }
}

/// Why a read failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error("the answer does not fit the function's outputs: {0}")]
    Abi(#[from] AbiError),
    #[error("the answer is not the value the function returns")]
    Shape,
}

impl<T> Read<T> {
    fn new(to: Address, signature: &str, args: &[Token], decode: fn(&[Token]) -> Option<T>) -> Result<Self, AbiError> {
        let function = Function::parse(signature)?;
        Ok(Read { to, data: function.encode(args)?, decode, function })
    }

    /// The `eth_call` that makes the read.
    #[must_use]
    pub fn request(&self) -> CallRequest {
        CallRequest::to(self.to, self.data.clone())
    }

    /// Decodes the answer against the function's outputs.
    pub fn decode(&self, answer: &[u8]) -> Result<T, ReadError> {
        let tokens = self.function.decode_output(answer)?;
        (self.decode)(&tokens).ok_or(ReadError::Shape)
    }

    /// Runs the read at the latest block.
    pub async fn run<H: Http>(&self, rpc: &Rpc<H>) -> Result<T, ReadError> {
        let answer = rpc.call(&self.request(), BlockTag::Latest).await?;
        self.decode(&answer)
    }
}

/// The wallet's deposit address, the CREATE2 proxy the factory derives for it (deployed
/// or not). USDC sent there on HyperCore is credited to the wallet.
pub fn deposit_address(wallet: Address) -> Result<Read<Address>, AbiError> {
    Read::new(
        DEPOSIT_PROXY_FACTORY,
        "computeProxyAddress(address user) view returns (address)",
        &[Token::address(wallet)],
        |tokens| tokens.first()?.as_address(),
    )
}

/// The Exchange's win fee, 18-decimal fraction.
pub fn win_fee_rate() -> Result<Read<u128>, AbiError> {
    Read::new(EXCHANGE, "winFeeRate() view returns (uint256)", &[], |tokens| tokens.first()?.as_u128())
}

/// Whether the off-hours keeper has switched winning closes to the off-hours payout curve
/// (`offHoursPayout()` on the Exchange). The relayer's
/// [`TradingState::off_hours`](crate::markets::TradingState::off_hours) mirrors it.
pub fn off_hours() -> Result<Read<bool>, AbiError> {
    Read::new(EXCHANGE, "offHoursPayout() view returns (bool)", &[], |tokens| tokens.first()?.as_bool())
}

/// A session key as the SessionKeyManager holds it for its wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredKey {
    /// When it stops signing, Unix seconds. 0 for a key never registered or revoked.
    pub expiry: u64,
    /// The builder code its registration named (`feeAddress`), `None` for none.
    pub builder: Option<Address>,
}

/// `key` as registered for `wallet` (`sessionKeys(wallet, key)` on the SessionKeyManager): its
/// expiry and the builder it names.
pub fn session_key(wallet: Address, key: Address) -> Result<Read<RegisteredKey>, AbiError> {
    Read::new(
        SESSION_KEY_MANAGER,
        "sessionKeys(address user, address key) view returns (uint64, address)",
        &[Token::address(wallet), Token::address(key)],
        |tokens| {
            let builder = tokens.get(1)?.as_address()?;
            Some(RegisteredKey { expiry: tokens.first()?.as_u64()?, builder: (!builder.is_zero()).then_some(builder) })
        },
    )
}

/// USDC rewards PaperStaking has distributed to `wallet` but not paid, in collateral units.
pub fn pending_reward(wallet: Address) -> Result<Read<u128>, AbiError> {
    Read::new(
        PAPER_STAKING,
        "pendingReward(address user) view returns (uint256)",
        &[Token::address(wallet)],
        |tokens| tokens.first()?.as_u128(),
    )
}

/// What `builder` has earned and not claimed, in collateral units.
pub fn builder_fees(builder: Address) -> Result<Read<u128>, AbiError> {
    Read::new(EXCHANGE, "builderFees(address builder) view returns (uint256)", &[Token::address(builder)], |tokens| {
        tokens.first()?.as_u128()
    })
}

/// Whether the BuilderRegistry approves `builder`: only then may a session key name it.
pub fn is_builder(builder: Address) -> Result<Read<bool>, AbiError> {
    Read::new(
        BUILDER_REGISTRY,
        "isApproved(address builder) view returns (bool)",
        &[Token::address(builder)],
        |tokens| tokens.first()?.as_bool(),
    )
}

/// The builder fee on a close's PnL, 18-decimal fraction (0.5% at contract set 2's start).
pub fn builder_fee_rate() -> Result<Read<u128>, AbiError> {
    Read::new(BUILDER_REGISTRY, "feeRate() view returns (uint64)", &[], |tokens| tokens.first()?.as_u128())
}

/// The builder fee a close through `builder` pays now, 18-decimal fraction: the registry's fee
/// rate while it approves `builder`, else 0 (`feeRateOf(builder)` on the BuilderRegistry).
pub fn builder_fee_rate_of(builder: Address) -> Result<Read<u128>, AbiError> {
    Read::new(
        BUILDER_REGISTRY,
        "feeRateOf(address builder) view returns (uint64)",
        &[Token::address(builder)],
        |tokens| tokens.first()?.as_u128(),
    )
}

/// The transaction a builder's own wallet sends to the Exchange to claim its fees, paid in HYPE
/// gas: they are credited to its balance there, and what the LP cannot cover joins the payout
/// queue.
pub fn claim_builder_fees() -> Result<CallRequest, AbiError> {
    let function = Function::parse("claimBuilderFees()")?;
    Ok(CallRequest::to(EXCHANGE, function.encode(&[])?))
}

#[cfg(test)]
#[path = "tests/chain.rs"]
mod tests;
