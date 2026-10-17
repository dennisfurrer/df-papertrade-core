//! Contract reads on HyperEVM: deposit address, BBO, win fee and staking rewards.
//! Each is a pure `eth_call` ([`Read`]) run at `latest`, the only block mainnet's RPC serves.

use df_hyperliquid_core::client::evm::rpc::BlockTag;
use df_hyperliquid_core::client::evm::{AbiError, CallRequest, Function, Rpc, Token};
use df_hyperliquid_core::utils::Address;

use crate::client::deployment::{DEPOSIT_PROXY_FACTORY, EXCHANGE, PAPER_STAKING};

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
    Rpc(#[from] df_hyperliquid_core::client::evm::RpcError),
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
    pub async fn run<H: df_hyperliquid_core::client::Http>(&self, rpc: &Rpc<H>) -> Result<T, ReadError> {
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

/// Hyperliquid's best bid and ask for perp `asset_index`, raw at the market's price scale.
/// The oracle prices from it.
#[must_use]
pub fn bbo(
    asset_index: u32,
) -> df_hyperliquid_core::client::evm::Read<df_hyperliquid_core::client::evm::precompiles::Bbo> {
    df_hyperliquid_core::client::evm::precompiles::bbo(asset_index)
}

#[cfg(test)]
#[path = "tests/chain.rs"]
mod tests;
