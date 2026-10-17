//! Deposits from the wallet's Hyperliquid balance to its deposit address, a per-wallet CREATE2
//! proxy whose USDC the relayer's sweeper credits to the venue within a few blocks. Make one
//! deposit at a time, as a second in the same window can hide the first from the sweep.

use df_hyperliquid_core::utils::{Address, Dec};

use df_hyperliquid_core::client::{ExchangeResponse, Http, Network, Request};

use crate::client::relayer::Call;
use df_hyperliquid_core::client::evm::Rpc;
use df_hyperliquid_core::transfer::{Amount, SPOT_DEX, SendAsset, SpotSend, UsdClassTransfer};
use df_hyperliquid_core::utils::sign::UserAction;

use crate::account::wallet::{Purpose, SignRequest, Wallet};
use crate::client::{Client, Error};

/// The fee that opens a new HyperCore account: 1 USDC, in 6-decimal units.
pub const ACTIVATION_FEE_USDC: u64 = 1_000_000;
/// The minimum deposit: 10 USDC, in 6-decimal units.
pub const MIN_DEPOSIT_USDC: u64 = 10_000_000;

/// Which Hyperliquid balance the USDC comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Spot,
    /// A standard account's perp balance, moved to spot first.
    Perp,
}

/// How the Hyperliquid account holds its balances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Separate spot and perp balances.
    Standard,
    /// One pooled balance (unified, portfolio margin).
    Pooled,
}

/// A planned deposit, ready for the wallet to sign.
#[derive(Debug, Clone)]
pub struct Deposit {
    pub to: Address,
    /// Whether the address has no HyperCore account yet.
    pub opens_account: bool,
    pub sends: Vec<Send>,
}

/// One Hyperliquid send of a deposit.
#[derive(Debug, Clone)]
pub enum Send {
    /// `usdClassTransfer` from the perp balance to spot.
    ToSpot(UsdClassTransfer),
    /// `spotSend` to the deposit address.
    Spot(SpotSend),
    /// `sendAsset` from a pooled balance to the deposit address, spot to spot.
    Pooled(SendAsset),
}

impl Send {
    #[must_use]
    pub fn action(&self) -> &dyn UserAction {
        match self {
            Send::ToSpot(action) => action,
            Send::Spot(action) => action,
            Send::Pooled(action) => action,
        }
    }

    #[must_use]
    pub const fn purpose(&self) -> Purpose {
        match self {
            Send::ToSpot(_) => Purpose::DepositToSpot,
            Send::Spot(_) | Send::Pooled(_) => Purpose::Deposit,
        }
    }

    /// What the send does, to show before the wallet asks.
    #[must_use]
    pub const fn describe(&self) -> &'static str {
        match self {
            Send::ToSpot(_) => "move USDC from perps to spot",
            Send::Spot(_) | Send::Pooled(_) => "send USDC to the deposit address",
        }
    }
}

fn amount(usdc: u64) -> Result<Amount, Error> {
    let value = Dec::new(i128::from(usdc), 6).ok_or_else(|| Error::Decode("amount out of range".into()))?;
    Amount::cut(value, 6).map_err(|e| Error::Invalid(e.to_string()))
}

impl<H: Http + Clone> Client<H> {
    /// Registers `owner`'s deposit address with the relayer's sweeper and returns it.
    pub async fn register_deposit(&self, owner: Address) -> Result<Address, Error> {
        let text = self.relayer().send(&Call::deposit_check(owner)).await?;
        Ok(crate::markets::stats::deposit_address(&text)?)
    }

    /// Plans a deposit of `usdc` (6 decimals, at least 10 USDC) from `source`, after registering
    /// the address and checking it against the one derived on chain. A perp balance moves to spot
    /// first, plus [`ACTIVATION_FEE_USDC`] when the address has no HyperCore account.
    pub async fn plan_deposit(&self, owner: Address, usdc: u64, source: Source, mode: Mode) -> Result<Deposit, Error> {
        if usdc < MIN_DEPOSIT_USDC {
            return Err(Error::Invalid("a deposit brings in at least 10 USDC".into()));
        }
        let to = self.register_deposit(owner).await?;
        if to != self.deposit_address(owner).await? {
            return Err(Error::Decode("the relayer named a deposit address the factory does not derive".into()));
        }
        let rpc = Rpc::new(self.http().clone(), self.config().rpc.clone());
        let exists = rpc
            .read(&df_hyperliquid_core::client::evm::precompiles::core_user_exists(to))
            .await
            .map_err(Error::Precompile)?;
        let opens_account = !exists;
        let nonce = self.now_ms();
        let network = Network::Mainnet;
        let sends = match (mode, source) {
            (Mode::Pooled, _) => {
                vec![Send::Pooled(SendAsset::usdc(network, to, SPOT_DEX, SPOT_DEX, amount(usdc)?, nonce))]
            }
            (Mode::Standard, Source::Spot) => {
                vec![Send::Spot(SpotSend::new(to, network.usdc().wire, amount(usdc)?, nonce))]
            }
            (Mode::Standard, Source::Perp) => {
                let fee = if opens_account { ACTIVATION_FEE_USDC } else { 0 };
                vec![
                    Send::ToSpot(UsdClassTransfer::new(amount(usdc.saturating_add(fee))?, false, nonce)),
                    Send::Spot(SpotSend::new(to, network.usdc().wire, amount(usdc)?, nonce + 1)),
                ]
            }
        };
        Ok(Deposit { to, opens_account, sends })
    }

    /// Has the wallet sign each send in order and posts it to Hyperliquid once. A failed send
    /// leaves the later ones unsent.
    pub async fn send_deposit(&self, wallet: &impl Wallet, deposit: &Deposit) -> Result<(), Error> {
        let owner = wallet.address();
        let network = Network::Mainnet;
        for send in &deposit.sends {
            let action = send.action();
            let request = SignRequest {
                purpose: send.purpose(),
                json: action.wallet_json(network),
                digest: action.digest(network),
            };
            let signature = wallet.sign(&request).await?;
            df_hyperliquid_core::account::verify(&request.digest, &signature, owner)
                .map_err(|e| Error::Signature(e.to_string()))?;
            let answer = self
                .http()
                .send(Request::post_json(self.config().exchange.clone(), action.body(&signature, network)))
                .await?;
            match ExchangeResponse::parse(&answer.text()).map_err(|e| Error::Decode(e.to_string()))? {
                ExchangeResponse::Ok(_) => {}
                ExchangeResponse::Err(why) => return Err(Error::Exchange(why)),
            }
        }
        Ok(())
    }
}
