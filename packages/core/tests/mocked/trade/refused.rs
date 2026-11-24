//! Writes refused: before anything is posted (no session key, a ticket the venue would refuse, more
//! than the account holds, nothing to do), and by the relayer after one post (a refusal status,
//! `{"ok":false}`, another intent kept, an intent rejected). Nothing is ever posted twice.

use df_hyperliquid_core::utils::hex;
use df_papertrade_core::Error;
use df_papertrade_core::account::session_key::RENEW_BEFORE_MS;
use df_papertrade_core::account::{Account, KeyState, OiCap, Operation, OperationKind, SessionRecord};
use df_papertrade_core::client::{Accepted, RelayerError};
use df_papertrade_core::trade::{Intent, Open, OpenPosition, OpenRefusal, deadline, intent_id};
use df_papertrade_core::utils::Lane;
use df_papertrade_core::utils::units::WAD;

use super::{BTC_MARK, account, keep, lanes, next, queued, ready, record, session};
use crate::account::Prompted;
use crate::support::venue::Venue;
use crate::support::{NOW, client, fixture, owner};

/// A BTC long of `margin` dollars at `leverage`.
const fn long(margin: u128, leverage: u32) -> Open {
    Open { instrument_id: 0, is_long: true, margin: margin * WAD, leverage, use_debt: false }
}

#[tokio::test]
async fn an_open_the_venue_would_refuse_is_never_signed() {
    let venue = Venue::new().get("/state/trading", "trading.json");
    let client = client(&venue);
    let unregistered = lanes(77, 1).await;
    keep(&unregistered, &record(false)).await;
    let ending = lanes(77, 1).await;
    keep(&ending, &SessionRecord { held_until_ms: NOW + RENEW_BEFORE_MS, ..record(true) }).await;
    let rich = || account(100_000, 0);
    let cases = [
        ("no session key", lanes(77, 1).await, rich(), long(10, 1000), Error::NeedsKey(KeyState::Missing)),
        ("a key not yet registered", unregistered, rich(), long(10, 1000), Error::NeedsKey(KeyState::Unregistered)),
        ("a key near its end", ending, rich(), long(10, 1000), Error::NeedsKey(KeyState::Missing)),
        (
            "more than the balance",
            ready(77, 1).await,
            account(100, 0),
            long(200, 10),
            Error::Open(OpenRefusal::InsufficientFunds),
        ),
        (
            "a ticket under the minimum",
            ready(77, 1).await,
            rich(),
            long(1, 1000),
            Error::Open(OpenRefusal::BelowMinimumMargin),
        ),
        (
            "a position under the minimum",
            ready(77, 1).await,
            rich(),
            long(10, 99),
            Error::Open(OpenRefusal::BelowMinimumNotional),
        ),
        (
            "past the market's leverage",
            ready(77, 1).await,
            rich(),
            long(10, 1001),
            Error::Open(OpenRefusal::LeverageTooHigh),
        ),
        ("past the side's room", ready(77, 1).await, rich(), long(5_000, 1000), Error::Open(OpenRefusal::OpenInterest)),
        (
            "no such market",
            ready(77, 1).await,
            rich(),
            Open { instrument_id: 9, ..long(10, 1000) },
            Error::Invalid("no market 9".to_owned()),
        ),
    ];
    for (name, store, holds, ticket, expected) in cases {
        assert_eq!(
            client.open(&store, owner(), &holds, ticket, Some(BTC_MARK)).await.map(drop),
            Err(expected),
            "{name}"
        );
        assert_eq!(next(&store, Lane::Exchange).await, 77, "{name}: no nonce taken");
    }
    assert!(venue.posted("/intents").is_empty());
    assert!(venue.relayed().iter().all(|sent| sent == "GET /state/trading"), "each read a fresh state, and only that");
}

/// The account's own cap of $1,000 of notional, and the venue's $10M cap less what the account
/// already holds on BTC, each refuse a $10 open at 1000x ($10,000) before a nonce is taken.
#[tokio::test]
async fn an_open_past_the_account_s_cap_is_never_signed() {
    let venue = Venue::new().get("/state/trading", "trading_mainnet.json");
    let client = client(&venue);
    let own = OiCap { instrument_id: None, cap: Some(1_000 * WAD) };
    let capped = Account { oi_caps: vec![own], ..account(100_000, 0) };
    let mut full = account(100_000, 0);
    full.operations.push(Operation {
        intent_id: [7; 32],
        kind: OperationKind::OpenPosition {
            instrument_id: 0,
            is_long: false,
            use_debt: false,
            margin: 9_995 * WAD,
            leverage: 1000,
        },
        accepted_at_ms: NOW,
        deadline: NOW / 1000 + 90,
    });
    for (name, holds) in [("its own cap", capped), ("the venue's cap, with a queued open", full)] {
        let store = ready(77, 1).await;
        let got = client.open(&store, owner(), &holds, long(10, 1000), Some(BTC_MARK)).await;
        assert_eq!(got.map(drop), Err(Error::Open(OpenRefusal::AccountOpenInterest)), "{name}");
        assert_eq!(next(&store, Lane::Exchange).await, 77, "{name}: no nonce taken");
    }
    assert!(venue.posted("/intents").is_empty());
}

#[tokio::test]
async fn an_open_on_a_venue_that_has_paused_is_never_signed() {
    let trading = fixture("trading_mainnet.json");
    let cases = [
        (
            "trading paused",
            trading.replace("\"tradingPaused\":false", "\"tradingPaused\":true"),
            OpenRefusal::TradingPaused,
        ),
        ("opens paused at intake", trading.replace(",[]]", ",[\"openPosition\"]]"), OpenRefusal::IntakePaused),
    ];
    for (name, state, expected) in cases {
        assert_ne!(state, trading, "{name}: the recording changed");
        let venue = Venue::new().get_text("/state/trading", &state);
        let got =
            client(&venue).open(&ready(77, 1).await, owner(), &account(100, 0), long(10, 1000), Some(BTC_MARK)).await;
        assert_eq!(got.map(drop), Err(Error::Open(expected)), "{name}");
        assert_eq!(venue.relayed(), ["GET /state/trading"], "{name}");
    }
}

#[tokio::test]
async fn a_withdrawal_stake_close_or_cancel_with_nothing_to_do_is_never_signed() {
    let venue = Venue::new();
    let client = client(&venue);
    let (store, keyless, wallet) = (ready(40, 900).await, lanes(40, 900).await, Prompted::new());
    let cases = [
        (
            "a withdrawal past the balance",
            client.withdraw(&store, &wallet, Some(&account(100, 0)), 100_000_001).await.map(drop),
            Error::InsufficientFunds("more than the available balance".to_owned()),
        ),
        (
            "a withdrawal under 10 USDC",
            client.withdraw(&store, &wallet, None, 9_999_999).await.map(drop),
            Error::Invalid("a withdrawal moves at least 10 USDC".to_owned()),
        ),
        (
            "a stake of nothing",
            client.stake(&store, &wallet, 0).await.map(drop),
            Error::Invalid("the amount must be more than zero".to_owned()),
        ),
        (
            "an unstake of nothing",
            client.unstake(&store, &wallet, 0).await.map(drop),
            Error::Invalid("the amount must be more than zero".to_owned()),
        ),
        (
            "a close of nothing",
            client.close(&store, owner(), None, &[]).await.map(drop),
            Error::Invalid("no positions to close".to_owned()),
        ),
        (
            "a close of one twice",
            client.close(&store, owner(), None, &[7, 7]).await.map(drop),
            Error::Invalid("a close names a position twice".to_owned()),
        ),
        (
            "a close naming one position in two of its intents",
            client.close(&store, owner(), None, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 1]).await.map(drop),
            Error::Invalid("a close names a position twice".to_owned()),
        ),
        (
            "a close with no key",
            client.close(&keyless, owner(), None, &[7]).await.map(drop),
            Error::NeedsKey(KeyState::Missing),
        ),
        (
            "a cancel with no key",
            client.cancel(&keyless, owner(), None, [1; 32]).await.map(drop),
            Error::NeedsKey(KeyState::Missing),
        ),
    ];
    for (name, got, expected) in cases {
        assert_eq!(got, Err(expected), "{name}");
    }
    assert!(venue.sent().is_empty(), "nothing reached the relayer");
    assert!(wallet.asked().is_empty(), "nor the wallet");
    // Each refused before it took a nonce.
    assert_eq!((next(&store, Lane::Exchange).await, next(&store, Lane::Staking).await), (40, 900));
    assert_eq!((next(&keyless, Lane::Exchange).await, next(&keyless, Lane::Staking).await), (40, 900));
}

#[tokio::test]
async fn an_open_the_relayer_does_not_keep_is_an_error_and_posted_once() {
    let intent = OpenPosition::new(owner(), 0, true, 10 * WAD, 1000, false, 77, deadline(NOW)).unwrap();
    let sent = intent_id(&Intent::OpenPosition(intent).digest(), session());
    let other = [0xab; 32];
    let rejected =
        format!(r#"{{"intentId":"{}","action":"openPosition","state":"failed"}}"#, hex::encode_prefixed(&sent));
    let refused = |status, code: &str, message: Option<&str>| {
        Error::Relayer(RelayerError::Refused {
            status,
            code: Some(code.to_owned()),
            message: message.map(str::to_owned),
        })
    };
    let cases = [
        (
            "a refusal",
            Venue::new()
                .status("/intents", 429, r#"{"code":"rate_limited","message":"slow down"}"#)
                .post("/intents", "{}"),
            refused(429, "rate_limited", Some("slow down")),
        ),
        (
            "an answer that is not ok",
            Venue::new().post("/intents", r#"{"ok":false,"error":"intake_paused"}"#),
            refused(200, "intake_paused", None),
        ),
        (
            "another intent kept",
            Venue::new().post("/intents", &queued(other, "openPosition")),
            Error::Relayer(RelayerError::OtherIntent {
                sent: hex::encode_prefixed(&sent),
                found: hex::encode_prefixed(&other),
            }),
        ),
        (
            "an intent failed",
            Venue::new().post("/intents", &rejected),
            Error::NotKept(Accepted {
                intent_id: sent,
                action: "openPosition".to_owned(),
                state: "failed".to_owned(),
                queue_revision: None,
            }),
        ),
    ];
    for (name, venue, expected) in cases {
        let venue = venue.get("/state/trading", "trading.json");
        let store = ready(77, 1).await;
        let got = client(&venue).open(&store, owner(), &account(100, 0), long(10, 1000), Some(BTC_MARK)).await;
        assert_eq!(got.map(drop), Err(expected), "{name}");
        assert_eq!(venue.relayed(), ["GET /state/trading", "POST /intents"], "{name}: posted once");
        assert_eq!(next(&store, Lane::Exchange).await, 78, "{name}: the nonce is spent all the same");
    }
}

#[tokio::test]
async fn an_open_with_no_trading_state_is_never_signed() {
    let venue = Venue::new().status("/state/trading", 502, "Bad Gateway").get_text("/state/trading", "{}");
    let got = client(&venue).open(&ready(77, 1).await, owner(), &account(100, 0), long(10, 1000), Some(BTC_MARK)).await;
    assert_eq!(got.map(drop), Err(Error::Relayer(RelayerError::Refused { status: 502, code: None, message: None })));
    assert_eq!(venue.relayed(), ["GET /state/trading"]);
}
