//! Closing whole positions by id, up to [`MAX_CLOSE_IDS`] in one intent.

use df_hyperliquid_core::utils::Address;
use df_hyperliquid_core::utils::sign::eip712::uint256;

use super::Refusal;

/// Most position ids one close intent may name.
pub const MAX_CLOSE_IDS: usize = 12;

/// Closes whole positions. A position is never closed in part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Close {
    pub user: Address,
    position_ids: Vec<[u8; 32]>,
    pub nonce: u128,
    pub deadline: u64,
}

impl Close {
    /// Positions by id, 1 to [`MAX_CLOSE_IDS`] of them, none twice.
    pub fn new(user: Address, ids: &[u128], nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        Close::of_words(user, ids.iter().map(|id| uint256(*id)).collect(), nonce, deadline)
    }

    /// Positions by their `uint256` words, for an id past 128 bits.
    pub fn of_words(user: Address, words: Vec<[u8; 32]>, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if words.is_empty() || words.len() > MAX_CLOSE_IDS {
            return Err(Refusal::CloseCount(words.len()));
        }
        for (index, word) in words.iter().enumerate() {
            if words[..index].contains(word) {
                return Err(Refusal::DuplicatePosition);
            }
        }
        Ok(Close { user, position_ids: words, nonce, deadline })
    }

    /// The position ids as `uint256` words.
    #[must_use]
    pub fn position_ids(&self) -> &[[u8; 32]] {
        &self.position_ids
    }
}
