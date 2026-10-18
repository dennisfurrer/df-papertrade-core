//! Which intents the relayer takes now, read from `intakeControls`: its revision, the least PAPER
//! a stake moves and the actions it has paused.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, array_of, str_of, tuple, u64_of, u128_of};

/// Which intents the relayer accepts now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intake {
    pub revision: u64,
    /// The least PAPER one stake moves, 18 decimals.
    pub paper_stake_minimum: u128,
    /// Actions the relayer refuses for now, such as `openPosition`.
    pub paused: Vec<String>,
}

impl Intake {
    /// Whether the relayer takes `action` now.
    #[must_use]
    pub fn accepts(&self, action: &str) -> bool {
        !self.paused.iter().any(|paused| paused == action)
    }

    pub(super) fn read(row: &Json) -> Result<Intake, BadWire> {
        let f = tuple(row, 3, "trading_intake")?;
        let paused = array_of(&f[2], "trading_intake")?
            .iter()
            .map(|action| str_of(action, "trading_intake_action").map(str::to_owned))
            .collect::<Result<_, _>>()?;
        Ok(Intake {
            revision: u64_of(&f[0], "trading_intake_revision")?,
            paper_stake_minimum: u128_of(&f[1], "trading_paper_stake_minimum")?,
            paused,
        })
    }
}
