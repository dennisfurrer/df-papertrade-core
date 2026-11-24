//! Client-chosen intent nonces. The contracts accept any unused nonce, so each lane starts at
//! a random point and counts up. The Exchange and PaperStaking have separate lanes. The caller
//! supplies the random bytes and persists the counters.

use crate::trade::intent::Intent;

/// Which contract's nonces an intent spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Exchange,
    Staking,
}

impl Lane {
    /// The lane `intent` spends from. `None` for a cancel, which has no nonce.
    #[must_use]
    pub const fn of(intent: &Intent) -> Option<Lane> {
        match intent {
            Intent::Stake(_) | Intent::Unstake(_) | Intent::Claim(_) => Some(Lane::Staking),
            Intent::CancelIntent(_) => None,
            _ => Some(Lane::Exchange),
        }
    }
}

/// How far ahead of a lane's next nonce a nonce the relayer reports may be and still move the
/// lane on, as papertrade.xyz allows: further is taken for a wrong report and left alone.
pub const CATCH_UP_WINDOW: u128 = 65_536;

/// The lane's next nonce once `recent`, the latest nonce the relayer has seen the wallet use in
/// it (the stream's `recentNonces`), is counted: `recent + 1` when `recent` is at or past `next`
/// and at most [`CATCH_UP_WINDOW`] ahead, else `None` (nothing to move).
#[must_use]
pub fn caught_up(next: u128, recent: u128) -> Option<u128> {
    (recent >= next && recent - next <= CATCH_UP_WINDOW).then(|| recent.checked_add(1)).flatten()
}

/// A lane's first nonce: three random bytes as a big-endian number (at least 1), times 256.
#[must_use]
pub fn first(random: [u8; 3]) -> u128 {
    let drawn = random.iter().fold(0u128, |number, byte| (number << 8) | u128::from(*byte));
    drawn.max(1) * 256
}

/// The next nonce of each lane for one wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nonces {
    exchange: u128,
    staking: u128,
}

impl Nonces {
    /// Fresh lanes from three random bytes each.
    #[must_use]
    pub fn seeded(exchange: [u8; 3], staking: [u8; 3]) -> Nonces {
        Nonces { exchange: first(exchange), staking: first(staking) }
    }

    /// Lanes resumed from the next nonces kept in storage.
    #[must_use]
    pub const fn resume(exchange: u128, staking: u128) -> Nonces {
        Nonces { exchange, staking }
    }

    /// The nonce `lane` would hand out next.
    #[must_use]
    pub const fn peek(&self, lane: Lane) -> u128 {
        match lane {
            Lane::Exchange => self.exchange,
            Lane::Staking => self.staking,
        }
    }

    /// Moves `lane` past `recent`, the latest nonce the relayer has seen the wallet use there
    /// from any device ([`caught_up`]). Whether it moved.
    pub fn catch_up(&mut self, lane: Lane, recent: u128) -> bool {
        let slot = match lane {
            Lane::Exchange => &mut self.exchange,
            Lane::Staking => &mut self.staking,
        };
        match caught_up(*slot, recent) {
            Some(next) => {
                *slot = next;
                true
            }
            None => false,
        }
    }

    /// Hands out `lane`'s next nonce and advances the lane. Treat it as spent even if the
    /// intent is never sent, since a reused nonce is refused.
    pub fn take(&mut self, lane: Lane) -> u128 {
        let slot = match lane {
            Lane::Exchange => &mut self.exchange,
            Lane::Staking => &mut self.staking,
        };
        let nonce = *slot;
        *slot = slot.saturating_add(1);
        nonce
    }
}

#[cfg(test)]
#[path = "tests/nonce.rs"]
mod tests;
