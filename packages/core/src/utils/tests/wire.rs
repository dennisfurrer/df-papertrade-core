//! The readers of the relayer's JSON: decimal strings or numbers for integers, nothing else, and
//! what a wire error quotes of the value it could not read: all of it, or its first 120
//! characters and `...`, cut between characters however many bytes each takes.

use super::*;

fn json(text: &str) -> Json {
    Json::parse(text).unwrap()
}

/// What `read` makes of each JSON text: the value as text, or the field the error names.
fn reads<T: ToString>(read: impl Fn(&Json, &str) -> Result<T, BadWire>, rows: &[(&str, Option<&str>)]) {
    for (text, value) in rows {
        let read = read(&json(text), "what").map(|value| value.to_string());
        assert_eq!(read.map_err(|bad| bad.what), value.map(str::to_owned).ok_or_else(|| "what".to_owned()), "{text}");
    }
}

#[test]
fn integers_read_from_digits_as_a_string_or_a_number() {
    let max = u128::MAX.to_string();
    let past = format!(r#""{max}0""#);
    reads(u128_of, &[(r#""5646""#, Some("5646")), ("37", Some("37")), (&format!(r#""{max}""#), Some(&max))]);
    reads(u128_of, &[(&past, None), (r#""""#, None), (r#""-1""#, None), (r#""1.5""#, None), ("1e3", None)]);
    reads(u128_of, &[(r#"" 1""#, None), ("null", None), ("true", None), ("[1]", None)]);
    reads(u64_of, &[("18446744073709551615", Some("18446744073709551615")), ("18446744073709551616", None)]);
    reads(u32_of, &[(r#""4294967295""#, Some("4294967295")), ("4294967296", None)]);
    reads(i128_of, &[(r#""-327""#, Some("-327")), ("4499154030980584574848767", Some("4499154030980584574848767"))]);
    reads(i128_of, &[(r#""-""#, None), (r#""""#, None), (r#""--5""#, None), (r#""+5""#, None), ("false", None)]);
    reads(i128_of, &[(&format!(r#""{max}""#), None)]);
}

#[test]
fn text_flags_addresses_hashes_and_decimals_read_only_as_themselves() {
    let hash = "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d";
    reads(bool_of, &[("true", Some("true")), ("false", Some("false")), (r#""true""#, None), ("1", None)]);
    reads(
        |json, what| str_of(json, what).map(str::to_owned),
        &[(r#""BTC""#, Some("BTC")), ("0", None), ("null", None)],
    );
    reads(
        |json, what| address_of(json, what).map(|address| address.to_hex()),
        &[
            (r#""0xF522D8F0EA3C5D0350D3A05DB1DF074ECF0B2FD7""#, Some("0xf522d8f0ea3c5d0350d3a05db1df074ecf0b2fd7")),
            (r#""0xf522""#, None),
            ("7", None),
        ],
    );
    let quoted = format!(r#""{hash}""#);
    reads(|json, what| hash_of(json, what).map(|hash| hex::encode_prefixed(&hash)), &[(&quoted, Some(hash))]);
    reads(|json, what| hash_of(json, what).map(|hash| hash.len()), &[(r#""0x668e""#, None), ("[]", None)]);
    reads(dec_of, &[("83335.5", Some("83335.5")), ("-327.15", Some("-327.15")), (r#""2492.35""#, Some("2492.35"))]);
    reads(dec_of, &[("true", None), (r#""about 5""#, None)]);
}

#[test]
fn a_null_reads_as_nothing_and_a_tuple_has_its_length() {
    assert_eq!(nullable(&json("null"), "what", u64_of), Ok(None));
    assert_eq!(nullable(&json(r#""7""#), "what", u64_of), Ok(Some(7)));
    assert_eq!(nullable(&json("true"), "what", u64_of).map_err(|bad| bad.what), Err("what".to_owned()));
    assert_eq!(array_of(&json("[1,2]"), "what").map(<[Json]>::len), Ok(2));
    assert_eq!(array_of(&json("{}"), "what").map_err(|bad| bad.what), Err("what".to_owned()));
    for (text, len) in
        [("[1,2,3]", Ok(3)), ("[1,2]", Err("what")), ("[1,2,3,4]", Err("what")), (r#""abc""#, Err("what"))]
    {
        let tuple = tuple(&json(text), 3, "what").map(<[Json]>::len).map_err(|bad| bad.what);
        assert_eq!(tuple, len.map_err(str::to_owned), "{text}");
    }
    assert_eq!(field(&json("{}"), "proxy"), Err(BadWire { what: "proxy".to_owned(), found: "missing".to_owned() }));
    assert_eq!(parse("{", "summary").map_err(|bad| bad.what), Err("summary".to_owned()));
}

#[test]
fn a_quoted_value_is_cut_at_120_characters_never_inside_one() {
    let long = "a".repeat(200);
    let emoji = "\u{1F400}".repeat(130);
    for (value, quoted) in [
        ("short".to_owned(), r#""short""#.to_owned()),
        (long.clone(), format!("\"{}...", &long[..119])),
        (emoji.clone(), format!("\"{}...", "\u{1F400}".repeat(119))),
    ] {
        assert_eq!(bad("name", &Json::String(value)).found, quoted);
    }
    assert_eq!(bad("name", &json("[1,2]")).to_string(), "name: [1,2]");
}
