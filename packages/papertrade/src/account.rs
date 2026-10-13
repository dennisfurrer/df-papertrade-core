//! One wallet's account as the relayer's indexer streams it
//! (`GET /state/user/live?wallet=`, a server-sent event stream; [`crate::sse`]
//! splits it): a snapshot, then patches, each naming the revision it applies
//! to. [`Account::apply`] folds a frame in with the app's own checks; a patch
//! that does not follow on, or does not add up, is [`AccountError::OutOfSync`]
//! and the stream is opened again for a fresh snapshot.
//!
//! The stream is public by address: anyone can watch any wallet.

use hl_core::{Address, Json};

use crate::{
    units::COLLATERAL_PER_USDC,
    wire::{
        BadWire, address_of, array_of, bad, bool_of, field, hash_of, i128_of, nullable, parse, str_of, tuple, u32_of,
        u64_of, u128_of,
    },
};

/// The most operations one close lists.
const MAX_OPERATION_CLOSE_IDS: usize = 12;

/// An open position. Amounts in collateral units, prices raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub position_id: u128,
    pub instrument_id: u32,
    pub is_long: bool,
    /// Opened from the queued balance.
    pub use_debt: bool,
    pub entry_price: u128,
    pub margin: u128,
    pub leverage: u32,
    /// Where it is liquidated, whole margin lost.
    pub bust_price: u128,
    pub opened_at_ms: u64,
    /// The intent that opened it; none for one opened before intent ids.
    pub intent_id: Option<[u8; 32]>,
}

impl Position {
    /// Margin × leverage, collateral units.
    #[must_use]
    pub fn notional(&self) -> u128 {
        self.margin.saturating_mul(u128::from(self.leverage))
    }

    fn read(row: &Json) -> Result<Position, BadWire> {
        let f = tuple(row, 10, "position_tuple")?;
        let position = Position {
            position_id: u128_of(&f[0], "position_id")?,
            instrument_id: u32_of(&f[1], "position_instrument")?,
            is_long: bool_of(&f[2], "position_side")?,
            use_debt: bool_of(&f[3], "position_debt")?,
            entry_price: u128_of(&f[4], "position_entry")?,
            margin: u128_of(&f[5], "position_margin")?,
            leverage: u32_of(&f[6], "position_leverage")?,
            bust_price: u128_of(&f[7], "position_bust")?,
            opened_at_ms: u64_of(&f[8], "position_opened_at")?,
            intent_id: nullable(&f[9], "position_intent", hash_of)?,
        };
        if position.entry_price == 0 || position.leverage == 0 {
            return Err(bad("position_tuple", row));
        }
        Ok(position)
    }
}

/// What a queued operation does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationKind {
    OpenPosition {
        instrument_id: u32,
        is_long: bool,
        use_debt: bool,
        margin: u128,
        leverage: u32,
    },
    ClosePositions {
        position_ids: Vec<u128>,
    },
    /// `amount` in collateral units (the intent named USDC's 6 decimals).
    WithdrawToCore {
        amount: u128,
    },
    RegisterSessionKey,
    /// PAPER, 18 decimals.
    PaperStake {
        amount: u128,
    },
    PaperUnstake {
        amount: u128,
    },
    PaperClaim,
    PaperDistribute,
}

impl OperationKind {
    /// The relayer's name for it.
    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self {
            OperationKind::OpenPosition { .. } => "openPosition",
            OperationKind::ClosePositions { .. } => "closePositions",
            OperationKind::WithdrawToCore { .. } => "withdrawToCore",
            OperationKind::RegisterSessionKey => "registerSessionKey",
            OperationKind::PaperStake { .. } => "paperStake",
            OperationKind::PaperUnstake { .. } => "paperUnstake",
            OperationKind::PaperClaim => "paperClaim",
            OperationKind::PaperDistribute => "paperDistribute",
        }
    }
}

/// An intent the relayer has accepted and not yet settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub intent_id: [u8; 32],
    pub kind: OperationKind,
    pub accepted_at_ms: u64,
    /// Unix seconds after which it expires unsent.
    pub deadline: u64,
}

impl Operation {
    fn read(row: &Json) -> Result<Operation, BadWire> {
        let f = tuple(row, 10, "operation_tuple")?;
        let action = str_of(&f[1], "operation_action")?;
        let instrument = nullable(&f[2], "operation_instrument", u32_of)?;
        let is_long = nullable(&f[3], "operation_open", bool_of)?;
        let use_debt = nullable(&f[4], "operation_open", bool_of)?;
        let amount = nullable(&f[5], "operation_amount", u128_of)?;
        let leverage = nullable(&f[6], "operation_leverage", u32_of)?;
        let close_ids = match &f[9] {
            Json::Null => None,
            ids => {
                let ids = array_of(ids, "operation_close_ids")?
                    .iter()
                    .map(|id| u128_of(id, "operation_close_id"))
                    .collect::<Result<Vec<_>, _>>()?;
                let distinct = ids.iter().enumerate().all(|(index, id)| !ids[..index].contains(id));
                if ids.is_empty() || ids.len() > MAX_OPERATION_CLOSE_IDS || !distinct {
                    return Err(bad("operation_close_ids", &f[9]));
                }
                Some(ids)
            }
        };
        if action != "closePositions" && close_ids.is_some() {
            return Err(bad("operation_close_ids", &f[9]));
        }
        let amount_of = |what| amount.ok_or_else(|| bad(what, &f[5]));
        let kind = match action {
            "openPosition" => match (instrument, is_long, use_debt, amount, leverage) {
                (Some(instrument_id), Some(is_long), Some(use_debt), Some(margin), Some(leverage)) if leverage > 0 => {
                    OperationKind::OpenPosition { instrument_id, is_long, use_debt, margin, leverage }
                }
                _ => return Err(bad("operation_open", row)),
            },
            "closePositions" => OperationKind::ClosePositions {
                position_ids: close_ids.ok_or_else(|| bad("operation_close_ids", row))?,
            },
            "withdrawToCore" => OperationKind::WithdrawToCore {
                amount: amount_of("operation_amount")?.saturating_mul(COLLATERAL_PER_USDC),
            },
            "paperStake" => OperationKind::PaperStake { amount: amount_of("operation_amount")? },
            "paperUnstake" => OperationKind::PaperUnstake { amount: amount_of("operation_amount")? },
            "registerSessionKey" => OperationKind::RegisterSessionKey,
            "paperClaim" => OperationKind::PaperClaim,
            "paperDistribute" => OperationKind::PaperDistribute,
            _ => return Err(bad("operation_action", &f[1])),
        };
        Ok(Operation {
            intent_id: hash_of(&f[0], "operation_intent")?,
            kind,
            accepted_at_ms: u64_of(&f[7], "operation_time")?,
            deadline: u64_of(&f[8], "operation_deadline")?,
        })
    }
}

/// A position a patch reports closed or liquidated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedPosition {
    pub position_id: u128,
    pub instrument_id: u32,
    pub is_long: bool,
    pub margin: u128,
    pub leverage: u32,
    pub liquidated: bool,
    /// After impact and fees; negative for a loss.
    pub net_pnl: i128,
    /// PAPER minted for the loss, 18 decimals.
    pub paper_minted: u128,
}

impl ClosedPosition {
    fn read(row: &Json) -> Result<ClosedPosition, BadWire> {
        let f = tuple(row, 8, "closed_position_tuple")?;
        Ok(ClosedPosition {
            position_id: u128_of(&f[0], "closed_position_id")?,
            instrument_id: u32_of(&f[1], "closed_position_instrument")?,
            is_long: bool_of(&f[2], "closed_position_side")?,
            margin: u128_of(&f[3], "closed_position_margin")?,
            leverage: u32_of(&f[4], "closed_position_leverage")?,
            liquidated: bool_of(&f[5], "closed_position_kind")?,
            net_pnl: i128_of(&f[6], "closed_position_pnl")?,
            paper_minted: u128_of(&f[7], "closed_position_paper")?,
        })
    }
}

/// How an intent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
    Expired,
    Cancelled,
    Unknown,
}

/// Balances held aside for intents the relayer has accepted, and what is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Funds {
    pub reserved_balance: u128,
    pub reserved_queued: u128,
    /// What a new open from the balance, or a withdrawal, may use.
    pub available_balance: u128,
    /// What a new open from the queued balance may use.
    pub available_queued: u128,
}

/// The wallet's PAPER, and its staking position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paper {
    pub balance: u128,
    pub staked: u128,
    pub pending_base: u128,
    pub reward_debt: u128,
}

impl Paper {
    /// USDC rewards claimable now (collateral units), at the staking
    /// accumulator `acc_reward_per_share` (scaled by 10^27), as PaperStaking
    /// counts them.
    #[must_use]
    pub fn pending_reward(&self, acc_reward_per_share: u128) -> u128 {
        let earned = crate::math::mul_div(self.staked, acc_reward_per_share, 1_000_000_000_000_000_000_000_000_000);
        let fresh = earned.map_or(0, |earned| earned.saturating_sub(self.reward_debt));
        self.pending_base.saturating_add(fresh)
    }
}

/// Totals since the wallet's first deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lifetime {
    pub deposited: u128,
    pub withdrawn: u128,
    pub realized_pnl: i128,
    pub fees_paid: u128,
    pub traded_notional: u128,
    pub paper_rewards_claimed: u128,
}

/// One wallet, as of its latest frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub address: Address,
    pub account_revision: u64,
    pub indexed_through_block: u64,
    pub wallet_revision: u64,
    /// Unix seconds of the last block the indexer read.
    pub indexed_through_timestamp: u64,
    /// Registered session keys and when each expires, Unix seconds.
    pub session_keys: Vec<(Address, u64)>,
    pub queue_revision: u128,
    /// Paid out, in collateral units.
    pub balance: u128,
    /// Owed by the payout queue, waiting on harvests.
    pub queued: u128,
    /// Margin in open positions.
    pub locked: u128,
    pub funds: Funds,
    pub queue_node_count: u64,
    /// The queued balance the next queue entries could fund as margin.
    pub consumable_debt: u128,
    pub paper: Paper,
    pub lifetime: Lifetime,
    pub operations: Vec<Operation>,
    /// Newest first, by position id.
    pub positions: Vec<Position>,
    /// The closes and liquidations the latest patch reported.
    pub closed: Vec<ClosedPosition>,
    /// The intent outcomes the latest patch reported.
    pub outcomes: Vec<([u8; 32], Outcome)>,
}

/// Why a frame was not folded in.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccountError {
    #[error(transparent)]
    Wire(#[from] BadWire),
    /// The frame is not the next one, or does not add up: open the stream again.
    #[error("the account stream is out of sync: {0}")]
    OutOfSync(&'static str),
    #[error("the frame is for {found}, not {expected}")]
    OtherWallet { expected: Address, found: Address },
}

/// What a frame did to the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Snapshot,
    Patch,
    /// Older than what is held; nothing changed.
    Stale,
}

impl Account {
    /// A snapshot frame alone.
    pub fn parse(text: &str) -> Result<Account, AccountError> {
        let json = parse(text, "wallet_frame")?;
        if json.get("baseAccountRevision").is_some() {
            return Err(AccountError::OutOfSync("a patch with no snapshot before it"));
        }
        Account::snapshot(&json)
    }

    /// Folds in a `wallet` event's data: a snapshot replaces, a patch applies
    /// to the revision it names.
    pub fn apply(held: Option<&Account>, text: &str, wallet: Address) -> Result<(Account, Applied), AccountError> {
        let json = parse(text, "wallet_frame")?;
        let found = address_of(field(&json, "address")?, "wallet_identity")?;
        if found != wallet {
            return Err(AccountError::OtherWallet { expected: wallet, found });
        }
        if json.get("baseAccountRevision").is_none() {
            let account = Account::snapshot(&json)?;
            if let Some(held) = held
                && account.account_revision <= held.account_revision
            {
                return Ok((held.clone(), Applied::Stale));
            }
            return Ok((account, Applied::Snapshot));
        }
        let Some(held) = held else {
            return Err(AccountError::OutOfSync("a patch with no snapshot before it"));
        };
        Ok((held.patch(&json)?, Applied::Patch))
    }

    fn snapshot(json: &Json) -> Result<Account, AccountError> {
        let mut account = Account {
            address: address_of(field(json, "address")?, "wallet_identity")?,
            account_revision: u64_of(field(json, "accountRevision")?, "account_revision")?,
            indexed_through_block: 0,
            wallet_revision: 0,
            indexed_through_timestamp: 0,
            session_keys: Vec::new(),
            queue_revision: 0,
            balance: 0,
            queued: 0,
            locked: 0,
            funds: Funds::default(),
            queue_node_count: 0,
            consumable_debt: 0,
            paper: Paper::default(),
            lifetime: read_lifetime(field(json, "lifetime")?)?,
            operations: array_of(field(json, "operations")?, "operations")?
                .iter()
                .map(Operation::read)
                .collect::<Result<_, _>>()?,
            positions: array_of(field(json, "openPositions")?, "positions")?
                .iter()
                .map(Position::read)
                .collect::<Result<_, _>>()?,
            closed: Vec::new(),
            outcomes: Vec::new(),
        };
        account.state(json, false)?;
        let descending = account.positions.windows(2).all(|pair| pair[0].position_id > pair[1].position_id);
        if !descending {
            return Err(AccountError::OutOfSync("positions out of order"));
        }
        account.check()?;
        Ok(account)
    }

    fn patch(&self, json: &Json) -> Result<Account, AccountError> {
        let base = u64_of(field(json, "baseAccountRevision")?, "account_revision")?;
        if base != self.account_revision {
            return Err(AccountError::OutOfSync("the patch is for another revision"));
        }
        let mut next = self.clone();
        next.account_revision = u64_of(field(json, "accountRevision")?, "account_revision")?;
        if next.account_revision <= self.account_revision {
            return Err(AccountError::OutOfSync("the patch does not move the revision on"));
        }
        if let Some(state) = json.get("state") {
            next.state(state, true)?;
        }
        if let Some(lifetime) = json.get("lifetime") {
            next.lifetime = read_lifetime(lifetime)?;
        }
        next.closed = match json.get("closedPositions") {
            Some(rows) => {
                array_of(rows, "closed_positions")?.iter().map(ClosedPosition::read).collect::<Result<_, _>>()?
            }
            None => Vec::new(),
        };
        next.outcomes = match json.get("intentOutcomes") {
            Some(rows) => array_of(rows, "intent_outcomes")?.iter().map(read_outcome).collect::<Result<_, _>>()?,
            None => Vec::new(),
        };

        let removed = array_of(field(json, "removedPositionIds")?, "wallet_patch")?
            .iter()
            .map(|id| u128_of(id, "removed_position_id"))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, id) in removed.iter().enumerate() {
            if removed[..index].contains(id) || !next.positions.iter().any(|position| position.position_id == *id) {
                return Err(AccountError::OutOfSync("removes a position it does not hold"));
            }
        }
        next.positions.retain(|position| !removed.contains(&position.position_id));
        for row in array_of(field(json, "addedPositions")?, "wallet_patch")? {
            let position = Position::read(row)?;
            if removed.contains(&position.position_id)
                || next.positions.iter().any(|held| held.position_id == position.position_id)
            {
                return Err(AccountError::OutOfSync("adds a position it already holds"));
            }
            next.positions.push(position);
        }
        next.positions.sort_by(|a, b| b.position_id.cmp(&a.position_id));

        let removed = array_of(field(json, "removedOperationIds")?, "wallet_patch")?
            .iter()
            .map(|id| hash_of(id, "operation_intent"))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, id) in removed.iter().enumerate() {
            if removed[..index].contains(id) || !next.operations.iter().any(|operation| operation.intent_id == *id) {
                return Err(AccountError::OutOfSync("removes an operation it does not hold"));
            }
        }
        next.operations.retain(|operation| !removed.contains(&operation.intent_id));
        for row in array_of(field(json, "addedOperations")?, "wallet_patch")? {
            let operation = Operation::read(row)?;
            if removed.contains(&operation.intent_id)
                || next.operations.iter().any(|held| held.intent_id == operation.intent_id)
            {
                return Err(AccountError::OutOfSync("adds an operation it already holds"));
            }
            next.operations.push(operation);
        }
        next.check()?;
        Ok(next)
    }

    /// The state fields: all of them in a snapshot, any of them in a patch.
    fn state(&mut self, json: &Json, partial: bool) -> Result<(), AccountError> {
        let get = |key: &str| -> Result<Option<&Json>, BadWire> {
            match json.get(key) {
                Some(value) => Ok(Some(value)),
                None if partial => Ok(None),
                None => field(json, key).map(Some),
            }
        };
        if let Some(value) = get("indexedThroughBlock")? {
            self.indexed_through_block = u64_of(value, "wallet_block")?;
        }
        if let Some(value) = get("walletRevision")? {
            self.wallet_revision = u64_of(value, "wallet_revision")?;
        }
        if let Some(value) = get("indexedThroughTimestamp")? {
            self.indexed_through_timestamp = u64_of(value, "wallet_timestamp")?;
        }
        if let Some(value) = get("sessionKeys")? {
            self.session_keys = array_of(value, "wallet_session_keys")?
                .iter()
                .map(|row| {
                    let f = tuple(row, 2, "wallet_session_keys")?;
                    Ok((address_of(&f[0], "wallet_session_keys")?, u64_of(&f[1], "wallet_session_expiry")?))
                })
                .collect::<Result<_, BadWire>>()?;
        }
        if let Some(value) = get("queueRevision")? {
            self.queue_revision = u128_of(value, "queue_revision")?;
        }
        if let Some(value) = get("balance")? {
            self.balance = u128_of(value, "wallet_balance")?;
        }
        if let Some(value) = get("queued")? {
            self.queued = u128_of(value, "wallet_queued")?;
        }
        if let Some(value) = get("locked")? {
            self.locked = u128_of(value, "wallet_locked")?;
        }
        if let Some(value) = get("funds")? {
            self.funds = Funds {
                reserved_balance: u128_of(field(value, "reservedBalance")?, "funds_reservedBalance")?,
                reserved_queued: u128_of(field(value, "reservedQueued")?, "funds_reservedQueued")?,
                available_balance: u128_of(field(value, "availableBalance")?, "funds_availableBalance")?,
                available_queued: u128_of(field(value, "availableQueued")?, "funds_availableQueued")?,
            };
        }
        if let Some(value) = get("queueNodeCount")? {
            self.queue_node_count = u64_of(value, "wallet_queue_count")?;
        }
        if let Some(value) = get("consumableDebt")? {
            self.consumable_debt = u128_of(value, "wallet_consumable_debt")?;
        }
        if let Some(value) = get("paper")? {
            self.paper = Paper {
                balance: u128_of(field(value, "balance")?, "paper_balance")?,
                staked: u128_of(field(value, "staked")?, "paper_staked")?,
                pending_base: u128_of(field(value, "pendingBase")?, "paper_pending")?,
                reward_debt: u128_of(field(value, "rewardDebt")?, "paper_debt")?,
            };
        }
        Ok(())
    }

    /// The sums the app checks every frame against.
    fn check(&self) -> Result<(), AccountError> {
        if self.wallet_revision > self.indexed_through_block {
            return Err(AccountError::OutOfSync("the wallet revision is past the indexed block"));
        }
        let locked = self.positions.iter().try_fold(0u128, |sum, position| sum.checked_add(position.margin));
        if locked != Some(self.locked) {
            return Err(AccountError::OutOfSync("locked margin is not the positions' margin"));
        }
        let available_balance = self.balance.saturating_sub(self.funds.reserved_balance);
        let available_queued = self.queued.saturating_sub(self.funds.reserved_queued);
        if self.funds.available_balance != available_balance || self.funds.available_queued != available_queued {
            return Err(AccountError::OutOfSync("available funds do not add up"));
        }
        Ok(())
    }

    /// The registered expiry of `key`, Unix seconds.
    #[must_use]
    pub fn session_expiry(&self, key: Address) -> Option<u64> {
        self.session_keys.iter().find(|(address, _)| *address == key).map(|(_, expiry)| *expiry)
    }

    /// Whether the wallet has ever deposited.
    #[must_use]
    pub const fn has_deposited(&self) -> bool {
        self.lifetime.deposited > 0
    }
}

fn read_lifetime(json: &Json) -> Result<Lifetime, BadWire> {
    Ok(Lifetime {
        deposited: u128_of(field(json, "depositedRaw")?, "lifetime_deposited")?,
        withdrawn: u128_of(field(json, "withdrawnRaw")?, "lifetime_withdrawn")?,
        realized_pnl: i128_of(field(json, "realizedPnlRaw")?, "lifetime_realized")?,
        fees_paid: u128_of(field(json, "feesPaidRaw")?, "lifetime_fees")?,
        traded_notional: u128_of(field(json, "tradedNotionalRaw")?, "lifetime_notional")?,
        paper_rewards_claimed: u128_of(field(json, "paperRewardsClaimedRaw")?, "lifetime_paper")?,
    })
}

fn read_outcome(row: &Json) -> Result<([u8; 32], Outcome), BadWire> {
    let f = tuple(row, 2, "intent_outcome_tuple")?;
    let outcome = match str_of(&f[1], "intent_outcome")? {
        "succeeded" => Outcome::Succeeded,
        "failed" => Outcome::Failed,
        "expired" => Outcome::Expired,
        "cancelled" => Outcome::Cancelled,
        "unknown" => Outcome::Unknown,
        _ => return Err(bad("intent_outcome", &f[1])),
    };
    Ok((hash_of(&f[0], "intent_outcome")?, outcome))
}

#[cfg(test)]
#[path = "tests/account.rs"]
mod tests;
