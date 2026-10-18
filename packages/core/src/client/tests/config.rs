//! The endpoints a client reaches: mainnet's hosts, with the relayer at the venue's origin or
//! behind the app's own proxy.

use super::*;
use crate::test_support::{RELAY, RPC, Wire, now, owner, sevens};

#[test]
fn mainnet_names_the_venue_s_hosts() {
    let config = Config::mainnet(now, sevens);
    assert_eq!(config.relayer, "https://exchange.papertrade.xyz");
    assert_eq!(config.rpc, RPC);
    assert_eq!(config.exchange, "https://api.hyperliquid.xyz/exchange");
    assert_eq!(((config.now_ms)(), config.relayer(RELAY).relayer), (now(), RELAY.to_owned()));
}

#[test]
fn a_page_reaches_the_relayer_through_its_app() {
    let wallet = owner().to_hex();
    for (config, stream) in [
        (Config::mainnet(now, sevens), format!("https://exchange.papertrade.xyz/state/user/live?wallet={wallet}")),
        (Config::mainnet(now, sevens).relayer(RELAY), format!("{RELAY}/state/user/live?wallet={wallet}")),
        (Config::mainnet(now, sevens).relayer("/api/papertrade/"), format!("{RELAY}/state/user/live?wallet={wallet}")),
    ] {
        assert_eq!(crate::Client::new(Wire::default(), config).stream_url(owner()), stream);
    }
}
