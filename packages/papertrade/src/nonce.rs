//! Intent nonces. The client chooses them: the contracts take any unused one,
//! so the app starts each lane at a random point and counts up, keeping the
//! next one per wallet in storage. Two lanes, as the app keeps them: the
//! Exchange's ([`Lane::Exchange`]: session keys, opens, closes, withdrawals)
//! and PaperStaking's ([`Lane::Staking`]). Sans-IO: the caller brings the
//! random bytes and keeps the counters.

use crate::intent::Intent;

/// Which contract's nonces an intent spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Exchange,
    Staking,
}

impl Lane {
    /// The lane `intent` spends from; `None` for a cancel, which has no nonce.
    #[must_use]
    pub const fn of(intent: &Intent) -> Option<Lane> {
        match intent {
            Intent::Stake(_) | Intent::Unstake(_) | Intent::Claim(_) => Some(Lane::Staking),
            Intent::CancelIntent(_) => None,
            _ => Some(Lane::Exchange),
        }
    }
}

/// A lane's first nonce, as the app picks it: three random bytes read as a
/// number, one if they are all zero, times 256.
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
    /// Fresh lanes from six random bytes.
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

    /// Hands out `lane`'s next nonce and moves it on; spent even if the intent
    /// is never sent, as a nonce is cheap and a reuse is refused.
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
