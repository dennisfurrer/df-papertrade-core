//! Deposits from the wallet's Hyperliquid balance, as the venue's own app
//! sends them.
//!
//! Every wallet has its own deposit address, a CREATE2 proxy derived from it
//! ([`Client::deposit_address`]). The relayer is asked first, which registers
//! the address with its sweeper; then the USDC goes to it on HyperCore, and
//! the sweeper moves it into the venue and credits the wallet within a few
//! blocks. At least 10 USDC must arrive; one deposit at a time, as a second
//! in the same window can hide the first from the sweep.
//!
//! The sends, each signed by the wallet under Hyperliquid's signature chain
//! and posted to its `/exchange` once:
//!
//! - a standard account sends from spot (`spotSend`); from its perp balance
//!   it first moves the amount to spot (`usdClassTransfer`), with the 1 USDC
//!   that opens the address's HyperCore account when it has none yet;
//! - a unified or portfolio-margin account sends from its pooled balance
//!   (`sendAsset`, spot to spot).
//!
//! Which the account is, and what each balance holds, is the caller's to read
//! and say ([`Mode`], [`Source`]).

use hl_core::{Address, Dec, ExchangeResponse, Http, Network, Request, UserAction};
use hl_evm::Rpc;
use hl_papertrade::relayer::Call;
use hl_transfer::{Amount, SPOT_DEX, SendAsset, SpotSend, UsdClassTransfer};

use crate::wallet::{Purpose, SignRequest, Wallet};
use crate::{Client, Error};

/// What the 1 USDC opening a new HyperCore account costs, in USDC units (6
/// decimals).
pub const ACTIVATION_FEE_USDC: u64 = 1_000_000;
/// The least a deposit brings in, USDC units: 10 USDC.
pub const MIN_DEPOSIT_USDC: u64 = 10_000_000;

/// Where on Hyperliquid the USDC is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Spot,
    /// A standard account's perp balance, moved to spot first.
    Perp,
}

/// How the Hyperliquid account keeps its balances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Separate spot and perp balances.
    Standard,
    /// One pooled balance (unified, portfolio margin).
    Pooled,
}

/// A deposit ready for the wallet: the address, whether it still needs its
/// HyperCore account opened, and the sends in order.
#[derive(Debug, Clone)]
pub struct Deposit {
    pub to: Address,
    pub opens_account: bool,
    pub sends: Vec<Send>,
}

/// One Hyperliquid send of a deposit.
#[derive(Debug, Clone)]
pub enum Send {
    ToSpot(UsdClassTransfer),
    Spot(SpotSend),
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

    /// What it does, for the trader to read before the wallet asks.
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
    /// Registers `owner`'s deposit address with the relayer's sweeper and
    /// returns it.
    pub async fn register_deposit(&self, owner: Address) -> Result<Address, Error> {
        let text = self.relayer().send(&Call::deposit_check(owner)).await?;
        Ok(hl_papertrade::stats::deposit_address(&text)?)
    }

    /// The sends that deposit `usdc` (6 decimals) from `source` on an account
    /// kept as `mode`, with the address registered and checked against the
    /// one derived on chain.
    pub async fn plan_deposit(&self, owner: Address, usdc: u64, source: Source, mode: Mode) -> Result<Deposit, Error> {
        if usdc < MIN_DEPOSIT_USDC {
            return Err(Error::Invalid("a deposit brings in at least 10 USDC".into()));
        }
        let to = self.register_deposit(owner).await?;
        if to != self.deposit_address(owner).await? {
            return Err(Error::Decode("the relayer named a deposit address the factory does not derive".into()));
        }
        let rpc = Rpc::new(self.http().clone(), self.config().rpc.clone());
        let exists = rpc.read(&hl_evm::precompiles::core_user_exists(to)).await.map_err(Error::Precompile)?;
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

    /// Has the wallet sign each send of `deposit` in turn and posts it to
    /// Hyperliquid once. A send that fails leaves the ones after it unsent.
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
            hl_core::verify(&request.digest, &signature, owner).map_err(|e| Error::Signature(e.to_string()))?;
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
