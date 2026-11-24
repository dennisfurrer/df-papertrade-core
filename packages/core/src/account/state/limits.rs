//! The account's own open interest caps (`oiCaps`) and the notional it holds on each market,
//! against which the venue's account cap
//! ([`TradingState::account_oi_cap`](crate::markets::TradingState::account_oi_cap)) is checked.
//!
//! The cap counts notional (margin x leverage, collateral units, 18 decimals) on one market, long
//! and short together, open positions and queued opens alike, as papertrade.xyz counts it.

use df_hyperliquid_core::utils::Json;

use super::{Account, OperationKind};
use crate::markets::TradingState;
use crate::utils::wire::{BadWire, array_of, bad, nullable, tuple, u32_of, u128_of};

/// One entry of the stream's `oiCaps`: a cap the relayer set for this account alone, in place of
/// the venue's account cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OiCap {
    /// The market it applies to. `None`: every market without an entry of its own.
    pub instrument_id: Option<u32>,
    /// The most notional the account may hold there, collateral units (18 decimals). `None`: no
    /// cap.
    pub cap: Option<u128>,
}

impl OiCap {
    /// Reads `oiCaps`: `[instrumentId or null, capRaw or null]` rows, each market at most once.
    pub(super) fn read_all(json: &Json) -> Result<Vec<OiCap>, BadWire> {
        let mut caps: Vec<OiCap> = Vec::new();
        for row in array_of(json, "oi_caps")? {
            let f = tuple(row, 2, "oi_cap_tuple")?;
            let cap = OiCap {
                instrument_id: nullable(&f[0], "oi_cap_market", u32_of)?,
                cap: nullable(&f[1], "oi_cap_amount", u128_of)?,
            };
            if caps.iter().any(|seen| seen.instrument_id == cap.instrument_id) {
                return Err(bad("oi_cap_duplicate", json));
            }
            caps.push(cap);
        }
        Ok(caps)
    }
}

impl Account {
    /// The most notional this account may hold on `instrument_id`, collateral units: its own cap
    /// for that market, else its own cap for every market, else the venue's account cap. `None`
    /// for no cap.
    #[must_use]
    pub fn oi_cap(&self, state: &TradingState, instrument_id: u32) -> Option<u128> {
        let own = |id: Option<u32>| self.oi_caps.iter().find(|entry| entry.instrument_id == id);
        match own(Some(instrument_id)).or_else(|| own(None)) {
            Some(entry) => entry.cap,
            None => state.account_oi_cap,
        }
    }

    /// The notional this account holds on `instrument_id`, collateral units: its open positions
    /// there and its queued opens there, long and short together.
    #[must_use]
    pub fn oi_used(&self, instrument_id: u32) -> u128 {
        let open = self
            .positions
            .iter()
            .filter(|position| position.instrument_id == instrument_id)
            .fold(0u128, |sum, position| sum.saturating_add(position.notional()));
        self.operations.iter().fold(open, |sum, operation| match operation.kind {
            OperationKind::OpenPosition { instrument_id: id, margin, leverage, .. } if id == instrument_id => {
                sum.saturating_add(margin.saturating_mul(u128::from(leverage)))
            }
            _ => sum,
        })
    }

    /// The notional this account may still open on `instrument_id`, collateral units: its cap
    /// less what it holds there, 0 at or past the cap. `None` for no cap. Pass it to
    /// [`open_limits`](crate::trade::math::open_limits) and
    /// [`check_open`](crate::trade::math::check_open).
    #[must_use]
    pub fn oi_room(&self, state: &TradingState, instrument_id: u32) -> Option<u128> {
        self.oi_cap(state, instrument_id).map(|cap| cap.saturating_sub(self.oi_used(instrument_id)))
    }
}

/// The stream's `recentNonces`: the latest nonce the relayer has seen this wallet use in each
/// lane, from any device. A lane it has seen none in is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecentNonces {
    /// `session`: the Exchange lane ([`Lane::Exchange`](crate::utils::Lane::Exchange)), for
    /// opens, closes, session key registrations and withdrawals.
    pub exchange: Option<u128>,
    /// `paper`: the PaperStaking lane ([`Lane::Staking`](crate::utils::Lane::Staking)), for
    /// stakes, unstakes and claims.
    pub staking: Option<u128>,
}

impl RecentNonces {
    pub(super) fn read(json: &Json) -> Result<RecentNonces, BadWire> {
        let lane = |key: &str| match json.get(key) {
            None => Ok(None),
            Some(value) => nullable(value, "recent_nonce", u128_of),
        };
        if !matches!(json, Json::Object(_)) {
            return Err(bad("recent_nonces", json));
        }
        Ok(RecentNonces { exchange: lane("session")?, staking: lane("paper")? })
    }

    /// The latest nonce seen in `lane`.
    #[must_use]
    pub const fn get(&self, lane: crate::utils::Lane) -> Option<u128> {
        match lane {
            crate::utils::Lane::Exchange => self.exchange,
            crate::utils::Lane::Staking => self.staking,
        }
    }
}

#[cfg(test)]
#[path = "tests/limits.rs"]
mod tests;
