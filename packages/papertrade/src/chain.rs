//! What the app reads from HyperEVM itself, each a pure `eth_call` ([`Read`])
//! run through [`hl_evm::Rpc`]: the wallet's deposit address, the BBO the
//! oracle prices at, the Exchange's win fee, and PaperStaking's distributed
//! rewards. Mainnet's RPC serves `latest` only.

use hl_core::Address;
use hl_evm::{AbiError, CallRequest, Function, Rpc, Token, rpc::BlockTag};

use crate::deployment::{DEPOSIT_PROXY_FACTORY, EXCHANGE, PAPER_STAKING};

/// One contract read: the call, and how its answer reads.
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

/// Why a read came back without its value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Rpc(#[from] hl_evm::RpcError),
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

    /// The answer's bytes, read against the function's outputs.
    pub fn decode(&self, answer: &[u8]) -> Result<T, ReadError> {
        let tokens = self.function.decode_output(answer)?;
        (self.decode)(&tokens).ok_or(ReadError::Shape)
    }

    /// Runs the read at the latest block.
    pub async fn run<H: hl_core::Http>(&self, rpc: &Rpc<H>) -> Result<T, ReadError> {
        let answer = rpc.call(&self.request(), BlockTag::Latest).await?;
        self.decode(&answer)
    }
}

/// The wallet's deposit address: the CREATE2 proxy the factory derives for it,
/// deployed or not. USDC sent to it on HyperCore is swept into the Exchange
/// and credited to the wallet.
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

/// The USDC rewards PaperStaking has distributed to `wallet` and not yet paid,
/// collateral units.
pub fn pending_reward(wallet: Address) -> Result<Read<u128>, AbiError> {
    Read::new(
        PAPER_STAKING,
        "pendingReward(address user) view returns (uint256)",
        &[Token::address(wallet)],
        |tokens| tokens.first()?.as_u128(),
    )
}

/// Hyperliquid's best bid and ask for a perp (`asset_index`), the oracle's
/// source, raw at the instrument's price scale.
#[must_use]
pub fn bbo(asset_index: u32) -> hl_evm::Read<hl_evm::precompiles::Bbo> {
    hl_evm::precompiles::bbo(asset_index)
}

#[cfg(test)]
#[path = "tests/chain.rs"]
mod tests;
