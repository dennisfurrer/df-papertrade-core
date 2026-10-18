//! What a deposit reads and computes without the wire: the deposit address in the relayer's
//! answer, and a USDC amount as the decimal a Hyperliquid send signs. Planning and sending are
//! journeys in the mocked suite.

use super::*;

#[test]
fn a_deposit_check_names_the_proxy() {
    for (answer, read) in [
        (r#"{"proxy":"0xc125C9A929Bd75555a027DD563C89A116Bd41C50"}"#, Ok("0xc125c9a929bd75555a027dd563c89a116bd41c50")),
        (
            r#"{"ok":true,"proxy":"0xeb9426c82652bd8dd95742027ad301a545eaf800"}"#,
            Ok("0xeb9426c82652bd8dd95742027ad301a545eaf800"),
        ),
        (r#"{"ok":true}"#, Err("proxy")),
        (r#"{"proxy":"0xc125"}"#, Err("proxy")),
        (r#"{"proxy":null}"#, Err("proxy")),
        ("registered", Err("deposit_check")),
    ] {
        let proxy = checked_address(answer).map(|proxy| proxy.to_hex()).map_err(|bad| bad.what);
        assert_eq!(proxy, read.map(str::to_owned).map_err(str::to_owned), "{answer}");
    }
}

#[test]
fn a_usdc_amount_is_the_decimal_a_send_signs() {
    for (usdc, signed) in [
        (25_000_000, Some("25")),
        (26_000_000, Some("26")),
        (10_500_000, Some("10.5")),
        (1, Some("0.000001")),
        (u64::MAX, Some("18446744073709.551615")),
        (0, None),
    ] {
        let amount = amount(usdc);
        assert_eq!(amount.as_ref().ok().map(Amount::as_str), signed, "{usdc}");
        assert_eq!(amount.is_err(), matches!(amount, Err(Error::Invalid(_))));
    }
}
