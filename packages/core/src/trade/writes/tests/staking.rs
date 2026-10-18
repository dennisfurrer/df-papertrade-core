//! Staking over a scripted wire, from the staking lane's nonces.

use df_hyperliquid_core::client::Method;

use super::*;
use crate::test_support::{NOW_MS, WAD, Wire, accepted, block_on, client, owner, store, wallet};
use crate::trade::intent::intent_id;

#[test]
fn staking_spends_its_own_lane() {
    let store = store(None, 40, 900);
    let stake = Staking::new(owner(), 5 * WAD, 900, deadline(NOW_MS)).unwrap();
    let id = intent_id(&Intent::Stake(stake).digest(), owner());
    let wire = Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, &accepted(id, "stake"));
    let answer = block_on(client(&wire).stake(&store, &wallet(), 5 * WAD)).unwrap();
    assert_eq!(answer.intent_id, id);
    assert_eq!(block_on(client(&wire).take_nonce(&store, owner(), Lane::Exchange)).unwrap(), 40, "untouched");
}
