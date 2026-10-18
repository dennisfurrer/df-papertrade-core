//! Withdrawals over a scripted wire: within the available balance, signed by the wallet itself.

use df_hyperliquid_core::client::Method;

use super::*;
use crate::test_support::{NOW_MS, Wire, accepted, account, block_on, client, owner, store, wallet};
use crate::trade::intent::intent_id;

#[test]
fn a_withdrawal_over_the_balance_sends_nothing_and_one_within_it_is_the_wallets() {
    let store = store(None, 40, 1);
    let account = account("");
    let wire = Wire::default();
    let over = block_on(client(&wire).withdraw(&store, &wallet(), Some(&account), 101_000_000)).unwrap_err();
    assert!(matches!(over, Error::InsufficientFunds(_)));
    assert!(wire.sent().is_empty());

    let intent = WithdrawToCore::new(owner(), 50_000_000, 40, deadline(NOW_MS)).unwrap();
    let id = intent_id(&Intent::WithdrawToCore(intent).digest(), owner());
    let wire = Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, &accepted(id, "withdrawToCore"));
    let answer = block_on(client(&wire).withdraw(&store, &wallet(), Some(&account), 50_000_000)).unwrap();
    assert_eq!(answer.intent_id, id, "signed by the wallet itself, not a session key");
}
