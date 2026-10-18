//! The relayer a client reaches: the venue's origin, or the app's own proxy.

use super::*;
use crate::test_support::{RELAY, Wire, client, now, owner, sevens};

#[test]
fn a_page_reaches_the_relayer_through_its_app() {
    let wire = Wire::default();
    let client = client(&wire);
    assert_eq!(client.config().relayer, RELAY);
    assert_eq!(client.stream_url(owner()), format!("{RELAY}/state/user/live?wallet={}", owner().to_hex()));
    assert!(Config::mainnet(now, sevens).relayer.starts_with("https://"));
}
