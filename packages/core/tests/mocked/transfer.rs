//! Deposits from the wallet's Hyperliquid balance to its deposit address: the address registered
//! with the relayer and checked against the one the factory derives, the sends planned from each
//! balance, each signed by the wallet and posted once to Hyperliquid's `/exchange`, and every way
//! a deposit stops. The node answers as mainnet did for the vectors' wallet (`fixtures/hyperevm/`).

use std::cell::RefCell;
use std::future::{Future, ready};

use df_hyperliquid_core::account::{LocalKey, Signature, recover};
use df_hyperliquid_core::client::{Network, TransportError};
use df_hyperliquid_core::utils::{Address, hex};
use df_papertrade_core::Error;
use df_papertrade_core::account::{Purpose, SignRequest, Wallet, WalletError};
use df_papertrade_core::client::RelayerError;
use df_papertrade_core::client::deployment::DEPOSIT_PROXY_FACTORY;
use df_papertrade_core::transfer::{ACTIVATION_FEE_USDC, Deposit, MIN_DEPOSIT_USDC, Mode, Source};
use serde_json::{Value, json};

use crate::support::venue::Venue;
use crate::support::{NOW, client, fixture, owner, wallet};

const CHECK: &str = "/funding/deposits/check";
/// HyperCore's precompile that says whether an address has an account.
const CORE_USER_EXISTS: &str = "0x0000000000000000000000000000000000000810";
/// The vectors' wallet's deposit address, as the factory derives it on mainnet.
const PROXY: &str = "0xeb9426c82652bd8dd95742027ad301a545eaf800";
/// What Hyperliquid answers a send it took.
const TAKEN: &str = r#"{"status":"ok","response":{"type":"default"}}"#;

/// How the wallet answers what it is asked to sign.
#[derive(Clone, Copy)]
enum Answer {
    Signs,
    /// Signs with another account's key, like a wallet that switched accounts.
    SignsAsAnother,
    Declines,
}

/// The vectors' wallet, keeping every request it is asked to sign.
struct Asked {
    answer: Answer,
    asked: RefCell<Vec<SignRequest>>,
}

impl Asked {
    fn new(answer: Answer) -> Asked {
        Asked { answer, asked: RefCell::default() }
    }

    fn purposes(&self) -> Vec<Purpose> {
        self.asked.borrow().iter().map(|request| request.purpose).collect()
    }
}

impl Wallet for Asked {
    fn address(&self) -> Address {
        owner()
    }

    fn sign(&self, request: &SignRequest) -> impl Future<Output = Result<Signature, WalletError>> {
        self.asked.borrow_mut().push(request.clone());
        let key = match self.answer {
            Answer::Signs => wallet().0,
            Answer::SignsAsAnother => LocalKey::from_bytes(&[7; 32]).unwrap(),
            Answer::Declines => return ready(Err(WalletError::Declined)),
        };
        ready(key.sign_digest(&request.digest).map_err(|error| WalletError::Failed(error.to_string())))
    }
}

/// The relayer and the factory agree on the deposit address, which has a HyperCore account if
/// `opened`.
fn agreed(opened: bool) -> Venue {
    let exists = if opened { "hyperevm/core_user_opened.json" } else { "hyperevm/core_user_new.json" };
    Venue::new()
        .post(CHECK, r#"{"proxy":"0xEb9426C82652BD8dd95742027ad301A545eaf800"}"#)
        .rpc_call(&DEPOSIT_PROXY_FACTORY.to_hex(), &fixture("hyperevm/deposit_address.json"))
        .rpc_call(CORE_USER_EXISTS, &fixture(exists))
}

fn padded(address: &str) -> String {
    format!("000000000000000000000000{}", &address[2..])
}

/// The `eth_call` the client makes: `data` to `to` at the latest block.
fn eth_call(to: &str, data: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "eth_call", "params": [{"to": to, "data": data}, "latest"]})
}

/// Each body posted to `/exchange` held against the send it was planned as: the same action,
/// its nonce, and a signature that recovers to the owner. Gives each send's type and amount.
fn sent(venue: &Venue, deposit: &Deposit) -> Vec<(String, String)> {
    let exchanged = venue.exchanged();
    assert!(exchanged.len() <= deposit.sends.len(), "{exchanged:?}");
    exchanged
        .iter()
        .zip(&deposit.sends)
        .map(|(body, send)| {
            let action = send.action();
            let planned: Value = serde_json::from_str(&action.wire(Network::Mainnet)).unwrap();
            assert_eq!(body["action"], planned);
            assert_eq!(body["nonce"], action.nonce());
            let signature = &body["signature"];
            let word = |key: &str| hex::decode_array(signature[key].as_str().unwrap()).unwrap();
            let v = u8::try_from(signature["v"].as_u64().unwrap()).unwrap();
            let signed = Signature::from_parts(word("r"), word("s"), v);
            assert_eq!(recover(&action.digest(Network::Mainnet), &signed), Ok(owner()));
            (planned["type"].as_str().unwrap().to_owned(), planned["amount"].as_str().unwrap().to_owned())
        })
        .collect()
}

fn pairs(sends: &[(&str, &str)]) -> Vec<(String, String)> {
    sends.iter().map(|(kind, amount)| ((*kind).to_owned(), (*amount).to_owned())).collect()
}

#[tokio::test]
async fn a_wallet_s_deposit_address_is_the_one_the_factory_derives() {
    let venue = Venue::new().rpc_call(&DEPOSIT_PROXY_FACTORY.to_hex(), &fixture("hyperevm/deposit_address.json"));

    let address = client(&venue).deposit_address(owner()).await.unwrap();

    assert_eq!(address.to_hex(), PROXY);
    let compute = format!("0xd600539a{}", padded(&owner().to_hex()));
    assert_eq!(venue.calls(), [eth_call(&DEPOSIT_PROXY_FACTORY.to_hex(), &compute)]);
    assert!(venue.relayed().is_empty(), "reading it registers nothing with the relayer");
}

#[tokio::test]
async fn a_deposit_from_perps_opens_the_address_then_sends_to_it() {
    let venue = agreed(false).exchange(TAKEN).exchange(TAKEN);
    let client = client(&venue);
    let wallet = Asked::new(Answer::Signs);

    let deposit = client.plan_deposit(owner(), 25_000_000, Source::Perp, Mode::Standard).await.unwrap();
    client.send_deposit(&wallet, &deposit).await.unwrap();

    assert_eq!((deposit.to.to_hex(), deposit.opens_account), (PROXY.to_owned(), true));
    assert_eq!(venue.relayed(), [format!("POST {CHECK}")], "registered with the sweeper once");
    assert_eq!(venue.posted(CHECK), [json!({"user": owner().to_hex()})]);
    let compute = format!("0xd600539a{}", padded(&owner().to_hex()));
    let exists = format!("0x{}", padded(PROXY));
    let calls = [eth_call(&DEPOSIT_PROXY_FACTORY.to_hex(), &compute), eth_call(CORE_USER_EXISTS, &exists)];
    assert_eq!(venue.calls(), calls);
    let both = pairs(&[("usdClassTransfer", "26"), ("spotSend", "25")]);
    assert_eq!(sent(&venue, &deposit), both, "the amount and the 1 USDC that opens the address");
    let exchanged = venue.exchanged();
    assert_eq!((&exchanged[0]["nonce"], &exchanged[1]["nonce"]), (&json!(NOW), &json!(NOW + 1)));
    assert_eq!(
        (&exchanged[0]["action"]["toPerp"], &exchanged[1]["action"]["destination"]),
        (&json!(false), &json!(PROXY))
    );
    assert_eq!(wallet.purposes(), [Purpose::DepositToSpot, Purpose::Deposit]);
}

#[tokio::test]
async fn each_balance_plans_its_own_sends_to_the_address() {
    for (mode, source, opened, planned) in [
        (Mode::Standard, Source::Perp, true, pairs(&[("usdClassTransfer", "25"), ("spotSend", "25")])),
        (Mode::Standard, Source::Spot, true, pairs(&[("spotSend", "25")])),
        (Mode::Standard, Source::Spot, false, pairs(&[("spotSend", "25")])),
        (Mode::Pooled, Source::Spot, false, pairs(&[("sendAsset", "25")])),
        (Mode::Pooled, Source::Perp, true, pairs(&[("sendAsset", "25")])),
    ] {
        let venue = planned.iter().fold(agreed(opened), |venue, _| venue.exchange(TAKEN));
        let client = client(&venue);
        let wallet = Asked::new(Answer::Signs);

        let deposit = client.plan_deposit(owner(), 25_000_000, source, mode).await.unwrap();
        client.send_deposit(&wallet, &deposit).await.unwrap();

        let case = format!("{mode:?} from {source:?}, opened: {opened}");
        assert_eq!(deposit.opens_account, !opened, "{case}");
        let fee = if opened { 0 } else { ACTIVATION_FEE_USDC };
        assert_eq!(deposit.activation_fee, fee, "{case}: what opening the address costs, from any balance");
        assert_eq!(sent(&venue, &deposit), planned, "{case}");
        let last = venue.exchanged().pop().unwrap();
        assert_eq!(last["action"]["destination"], PROXY, "{case}");
        let purposes = planned.iter().map(|(kind, _)| match kind.as_str() {
            "usdClassTransfer" => Purpose::DepositToSpot,
            _ => Purpose::Deposit,
        });
        assert_eq!(wallet.purposes(), purposes.collect::<Vec<_>>(), "{case}");
    }
}

#[tokio::test]
async fn a_deposit_under_ten_usdc_is_refused_before_anything_is_sent() {
    let venue = Venue::new();

    let small = client(&venue).plan_deposit(owner(), MIN_DEPOSIT_USDC - 1, Source::Spot, Mode::Standard).await;

    assert_eq!(small.map(|_| ()), Err(Error::Invalid("a deposit brings in at least 10 USDC".to_owned())));
    assert!(venue.sent().is_empty(), "not even registered");
}

#[tokio::test]
async fn an_address_the_factory_does_not_derive_is_refused_before_any_send() {
    let busy_proxy = r#"{"proxy":"0xA9cF30b9BBA30d52B82D9C677107a421C1e0dC38"}"#;
    let venue = Venue::new()
        .post(CHECK, busy_proxy)
        .rpc_call(&DEPOSIT_PROXY_FACTORY.to_hex(), &fixture("hyperevm/deposit_address.json"));

    let planned = client(&venue).plan_deposit(owner(), 25_000_000, Source::Spot, Mode::Standard).await;

    let refused = Error::Decode("the relayer named a deposit address the factory does not derive".to_owned());
    assert_eq!(planned.map(|_| ()), Err(refused));
    assert_eq!(venue.calls().len(), 1, "the factory was asked, and HyperCore never was");
}

#[tokio::test]
async fn a_registration_the_relayer_refuses_or_cannot_name_plans_nothing() {
    let refused = Venue::new().post(CHECK, "").status(CHECK, 400, r#"{"ok":false,"error":"origin_not_allowed"}"#);
    let nameless = Venue::new().post(CHECK, r#"{"ok":true}"#);

    let refused_plan = client(&refused).plan_deposit(owner(), 25_000_000, Source::Spot, Mode::Standard).await;
    let nameless_plan = client(&nameless).register_deposit(owner()).await;

    let origin = RelayerError::Refused { status: 400, code: Some("origin_not_allowed".to_owned()), message: None };
    assert_eq!(refused_plan.map(|_| ()), Err(Error::Relayer(origin)));
    assert_eq!(nameless_plan, Err(Error::Decode("proxy: missing".to_owned())));
    for venue in [refused, nameless] {
        assert_eq!(venue.relayed(), [format!("POST {CHECK}")]);
        assert!(venue.calls().is_empty(), "no chain read follows");
    }
}

#[tokio::test]
async fn a_send_hyperliquid_refuses_or_fails_leaves_the_rest_unsent() {
    let refusing = agreed(true).exchange(r#"{"status":"err","response":"Insufficient balance for withdrawal"}"#);
    let failing = agreed(true).exchange(TAKEN).status("/exchange", 502, "bad gateway");

    let gateway = TransportError::Status { status: 502, body: "bad gateway".to_owned() };
    for (venue, stopped) in [
        (refusing, Error::Exchange("Insufficient balance for withdrawal".to_owned())),
        (failing, Error::Transport(gateway)),
    ] {
        let client = client(&venue);
        let deposit = client.plan_deposit(owner(), 25_000_000, Source::Perp, Mode::Standard).await.unwrap();

        let refused = client.send_deposit(&wallet(), &deposit).await.unwrap_err();

        assert_eq!(refused, stopped);
        assert_eq!(sent(&venue, &deposit), pairs(&[("usdClassTransfer", "25")]), "the spot send was never made");
    }
}

#[tokio::test]
async fn a_signature_declined_or_from_another_account_is_never_posted() {
    for answer in [Answer::Declines, Answer::SignsAsAnother] {
        let venue = agreed(true);
        let client = client(&venue);
        let deposit = client.plan_deposit(owner(), 25_000_000, Source::Spot, Mode::Standard).await.unwrap();
        let wallet = Asked::new(answer);

        let unsigned = client.send_deposit(&wallet, &deposit).await.unwrap_err();

        match answer {
            Answer::Declines => assert_eq!(unsigned, Error::Wallet(WalletError::Declined)),
            _ => assert!(matches!(unsigned, Error::Signature(_)), "{unsigned:?}"),
        }
        assert_eq!(wallet.purposes(), [Purpose::Deposit], "asked once");
        assert!(venue.exchanged().is_empty(), "nothing posted to Hyperliquid");
    }
}
