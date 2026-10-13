use hl_core::hex;

use super::*;

/// The separators the deployed contracts answer from `DOMAIN_SEPARATOR()`
/// (the session key manager for the exchange domain; PaperStaking), read on
/// chain 2026-10-13.
#[test]
fn the_domains_are_the_ones_on_chain() {
    assert_eq!(
        hex::encode_prefixed(&EXCHANGE_DOMAIN.separator()),
        "0x53a2ab5a64b3239d36b108fb4cd035ebb40ed3f71dec28e37966622670147e77"
    );
    assert_eq!(
        hex::encode_prefixed(&STAKING_DOMAIN.separator()),
        "0x71706d56c8b436981e93987cd97df512528491b243c6d8d1614cad52659e7eb0"
    );
    assert_eq!(EXCHANGE_DOMAIN.name, "String");
    assert_eq!(CANCEL_DOMAIN.verifying_contract, EXCHANGE);
}

#[test]
fn every_address_is_written_out() {
    for address in [
        EXCHANGE,
        BATCH_EXECUTOR,
        PRICE_ORACLE,
        SESSION_KEY_MANAGER,
        DEPOSIT_PROXY_FACTORY,
        PAPER,
        PAPER_STAKING,
        PAPER_TOKENOMICS,
        USDC,
        CORE_DEPOSIT_WALLET,
        TIMELOCK,
        MULTISIG,
    ] {
        assert!(!address.is_zero());
    }
    assert_eq!(EXCHANGE.to_hex(), "0x6cd5661646289fb6e65ea5c032310fded797d0a2");
}

#[test]
fn a_listing_is_found_by_its_symbol() {
    assert_eq!(listing("btc"), Some(BTC));
    assert_eq!(listing("ETH").map(|listing| listing.price_scale), Some(100));
    assert_eq!(listing("SOL"), None);
}
