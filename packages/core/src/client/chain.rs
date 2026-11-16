//! Contract reads on HyperEVM: the deposit address, the win fee, staking rewards and builder
//! fees. Each is a
//! pure `eth_call` ([`Read`]) run at `latest`, the only block mainnet's RPC serves. The book's
//! BBO is HyperCore's own precompile, read through
//! [`df_hyperliquid_core::client::evm::precompiles::bbo`].

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::client::evm::{AbiError, BlockTag, CallRequest, Function, Rpc, RpcError, Token};
use df_hyperliquid_core::utils::Address;

use crate::client::deployment::{BUILDER_REGISTRY, DEPOSIT_PROXY_FACTORY, EXCHANGE, PAPER_STAKING};

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
