//! Folding the captured streams frame by frame: a busy wallet over five minutes and a wallet with
//! a queued close. A frame out of turn, a snapshot that does not add up and a patch that does not
//! follow on are each refused with the reason to reopen the stream.

use super::*;
use crate::account::state::{ClosedPosition, OperationKind, Outcome};
use crate::live::{Events, WALLET};
use crate::test_support::{QUEUED, STREAM};

fn frames(stream: &str) -> Vec<String> {
    Events::new().push(stream).into_iter().filter(|event| event.name == WALLET).map(|event| event.data).collect()
}

fn wallet_of(frame: &str) -> Address {
    let json = df_hyperliquid_core::utils::Json::parse(frame).unwrap();
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

/// The busy wallet's snapshot, folded.
fn snapshot() -> (Account, Address) {
    let frames = frames(STREAM);
    let wallet = wallet_of(&frames[0]);
    (Account::apply(None, &frames[0], wallet).unwrap().0, wallet)
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
    for pair in seen.windows(2) {
        for gone in &pair[1].closed {
            assert!(pair[0].positions.iter().any(|position| position.position_id == gone.position_id));
            assert!(!pair[1].positions.iter().any(|position| position.position_id == gone.position_id));
        }
    }
    assert!(seen.last().unwrap().has_deposited());
}

/// The fixture is a snapshot with open positions, then a close queued behind 36 other intents.
#[test]
fn a_queued_close_is_an_operation() {
    let seen = fold(QUEUED);
    let first = &seen[0];
    assert!(!first.positions.is_empty());
    assert_eq!(first.session_keys.len(), 1);
    let (key, expiry) = first.session_keys[0];
    assert_eq!(first.session_expiry(key), Some(expiry));
    assert_eq!(first.session_expiry(Address::ZERO), None);
    let expired = &seen[1].outcomes;
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].1, Outcome::Expired);
    let last = seen.last().unwrap();
    let close = last.operations.iter().find(|operation| operation.kind.action() == "closePositions").unwrap();
    assert_eq!(close.kind, OperationKind::ClosePositions { position_ids: vec![407_570] });
    assert_eq!(close.deadline, 1_791_863_080);
}

#[test]
fn a_frame_out_of_turn_is_refused() {
    let frames = frames(STREAM);
    let (snapshot, wallet) = snapshot();
    let stranger: Address = "0x0000000000000000000000000000000000000001".parse().unwrap();
    let cases = [
        (
            "a patch first",
            Account::apply(None, &frames[1], wallet).map(drop),
            AccountError::OutOfSync("a patch with no snapshot before it"),
        ),
        (
            "a patch read as a snapshot",
            Account::parse(&frames[1]).map(drop),
            AccountError::OutOfSync("a patch with no snapshot before it"),
        ),
        (
            "a patch skipped",
            Account::apply(Some(&snapshot), &frames[2], wallet).map(drop),
            AccountError::OutOfSync("the patch is for another revision"),
        ),
        (
            "another wallet's frame",
            Account::apply(None, &frames[0], stranger).map(drop),
            AccountError::OtherWallet { expected: stranger, found: wallet },
        ),
    ];
    for (name, got, expected) in cases {
        assert_eq!(got, Err(expected), "{name}");
    }
}

#[test]
fn a_snapshot_no_newer_than_the_held_account_changes_nothing() {
    let (snapshot, wallet) = snapshot();
    let frame = &frames(STREAM)[0];
    assert_eq!(Account::apply(Some(&snapshot), frame, wallet), Ok((snapshot.clone(), Applied::Stale)));
    let older = frame.replace("\"accountRevision\":1", "\"accountRevision\":0");
    assert_eq!(Account::apply(Some(&snapshot), &older, wallet), Ok((snapshot, Applied::Stale)));
}

#[test]
fn a_snapshot_that_does_not_add_up_is_refused() {
    let queued = &frames(QUEUED)[0];
    let busy = &frames(STREAM)[0];
    let cases = [
        (
            "locked is not the margin",
            queued.replacen("\"locked\":\"", "\"locked\":\"1", 1),
            "locked margin is not the positions' margin",
        ),
        (
            "available is not the balance less the reserve",
            queued.replacen("\"availableBalance\":\"", "\"availableBalance\":\"1", 1),
            "available funds do not add up",
        ),
        (
            "the wallet revision is past the indexed block",
            queued.replacen("\"walletRevision\":48399112", "\"walletRevision\":48399113", 1),
            "the wallet revision is past the indexed block",
        ),
        ("positions out of order", busy.replacen("[\"411276\"", "[\"411000\"", 1), "positions out of order"),
    ];
    for (name, frame, why) in cases {
        assert_eq!(Account::parse(&frame), Err(AccountError::OutOfSync(why)), "{name}");
    }
}

#[test]
fn a_patch_that_does_not_follow_on_is_refused() {
    let frames = frames(STREAM);
    let (snapshot, wallet) = snapshot();
    let second = Account::apply(Some(&snapshot), &frames[1], wallet).unwrap().0;
    let replayed = frames[1]
        .replace("\"accountRevision\":2,\"baseAccountRevision\":1", "\"accountRevision\":3,\"baseAccountRevision\":2");
    let held = r#"["411276",0,true,false,"832245","10000000000000000000",100,"824315",1791862800000,null]"#;
    let cases = [
        (
            "stays at its base",
            &snapshot,
            frames[1].replace("\"accountRevision\":2", "\"accountRevision\":1"),
            "the patch does not move the revision on",
        ),
        (
            "removes a position not held",
            &snapshot,
            frames[1].replace("\"removedPositionIds\":[]", "\"removedPositionIds\":[\"1\"]"),
            "removes a position it does not hold",
        ),
        (
            "adds a position held",
            &snapshot,
            frames[1].replace("\"addedPositions\":[]", &format!("\"addedPositions\":[{held}]")),
            "adds a position it already holds",
        ),
        (
            "removes an operation not held",
            &snapshot,
            frames[2].replace("\"baseAccountRevision\":2", "\"baseAccountRevision\":1"),
            "removes an operation it does not hold",
        ),
        ("adds an operation held", &second, replayed, "adds an operation it already holds"),
    ];
    for (name, held, frame, why) in cases {
        assert_eq!(Account::apply(Some(held), &frame, wallet), Err(AccountError::OutOfSync(why)), "{name}");
    }
}
