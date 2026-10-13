//! The account stream as captured from the relayer (`fixtures/live_*.txt`, a
//! busy wallet over five minutes and one with a queued close), folded frame
//! by frame with every check the app makes.

use hl_core::Address;

use super::*;
use crate::sse::{Events, WALLET};

const STREAM: &str = include_str!("../../fixtures/live_stream.txt");
const QUEUED: &str = include_str!("../../fixtures/live_queued_close.txt");

fn frames(stream: &str) -> Vec<String> {
    Events::new().push(stream).into_iter().filter(|event| event.name == WALLET).map(|event| event.data).collect()
}

fn wallet_of(frame: &str) -> Address {
    let json = hl_core::Json::parse(frame).unwrap();
    json.get("address").unwrap().as_str().unwrap().parse().unwrap()
}

fn fold(stream: &str) -> Vec<Account> {
    let frames = frames(stream);
    let wallet = wallet_of(&frames[0]);
    let mut held: Option<Account> = None;
    let mut seen = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let (account, applied) = Account::apply(held.as_ref(), frame, wallet).unwrap();
        assert_eq!(applied, if index == 0 { Applied::Snapshot } else { Applied::Patch });
        seen.push(account.clone());
        held = Some(account);
    }
    seen
}

#[test]
fn a_busy_wallet_folds_frame_by_frame() {
    let seen = fold(STREAM);
    assert_eq!(seen.len(), 13);
    let opened: usize = seen.windows(2).filter(|pair| pair[1].positions.len() > pair[0].positions.len()).count();
    let closed: Vec<&ClosedPosition> = seen.iter().flat_map(|account| &account.closed).collect();
    assert!(opened > 0 && !closed.is_empty(), "the capture opens and closes positions");
    for account in &seen {
        assert_eq!(account.locked, account.positions.iter().map(|position| position.margin).sum::<u128>());
        assert!(account.positions.windows(2).all(|pair| pair[0].position_id > pair[1].position_id));
    }
    // A closed position leaves the open ones in the same patch.
    for pair in seen.windows(2) {
        for gone in &pair[1].closed {
            assert!(pair[0].positions.iter().any(|position| position.position_id == gone.position_id));
            assert!(!pair[1].positions.iter().any(|position| position.position_id == gone.position_id));
        }
    }
    let last = seen.last().unwrap();
    assert!(last.has_deposited());
    assert!(last.outcomes.iter().all(|(_, outcome)| *outcome != Outcome::Unknown) || last.outcomes.is_empty());
}

/// `0x9587…`: a snapshot with open positions, then a close queued behind 36
/// other intents.
#[test]
fn a_queued_close_is_an_operation() {
    let seen = fold(QUEUED);
    let first = &seen[0];
    assert!(!first.positions.is_empty());
    assert_eq!(first.session_keys.len(), 1);
    let (key, expiry) = first.session_keys[0];
    assert_eq!(first.session_expiry(key), Some(expiry));
    let expired = &seen[1].outcomes;
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].1, Outcome::Expired);
    let last = seen.last().unwrap();
    let close = last.operations.iter().find(|operation| operation.kind.action() == "closePositions").unwrap();
    assert_eq!(close.kind, OperationKind::ClosePositions { position_ids: vec![407_570] });
    assert_eq!(close.deadline, 1_791_863_080);
}

#[test]
fn a_patch_out_of_turn_is_out_of_sync() {
    let frames = frames(STREAM);
    let wallet = wallet_of(&frames[0]);
    let (snapshot, _) = Account::apply(None, &frames[0], wallet).unwrap();
    assert!(matches!(Account::apply(None, &frames[1], wallet), Err(AccountError::OutOfSync(_))));
    assert!(matches!(Account::apply(Some(&snapshot), &frames[2], wallet), Err(AccountError::OutOfSync(_))));
    let stranger: Address = "0x0000000000000000000000000000000000000001".parse().unwrap();
    assert!(matches!(Account::apply(None, &frames[0], stranger), Err(AccountError::OtherWallet { .. })));
    let (again, applied) = Account::apply(Some(&snapshot), &frames[0], wallet).unwrap();
    assert_eq!((applied, again), (Applied::Stale, snapshot));
}

#[test]
fn a_snapshot_that_does_not_add_up_is_refused() {
    let frames = frames(QUEUED);
    let broken = frames[0].replacen("\"locked\":\"", "\"locked\":\"1", 1);
    assert!(matches!(Account::parse(&broken), Err(AccountError::OutOfSync(_))));
}

/// PaperStaking's `pendingReward`: the base, and what the accumulator has
/// added since the debt was struck (`staked × acc / 10^27 − debt`).
#[test]
fn the_pending_reward_is_paperstakings() {
    let paper = Paper { balance: 0, staked: 3 * 10u128.pow(24), pending_base: 7, reward_debt: 10u128.pow(22) };
    assert_eq!(paper.pending_reward(10u128.pow(25)), 7 + 3 * 10u128.pow(22) - 10u128.pow(22));
    assert_eq!(paper.pending_reward(0), 7, "never below the base");
}
