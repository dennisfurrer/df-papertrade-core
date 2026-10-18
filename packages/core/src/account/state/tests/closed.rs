//! The closes a patch reports and how its intents ended.

use super::*;

#[test]
fn a_closed_row_reads_with_its_signed_pnl() {
    let row = r#"["411276",0,true,"10000000000000000000",100,true,"-108141232449579150","2766363925334821349"]"#;
    let closed = ClosedPosition::read(&Json::parse(row).unwrap()).unwrap();
    let expected = ClosedPosition {
        position_id: 411_276,
        instrument_id: 0,
        is_long: true,
        margin: 10_000_000_000_000_000_000,
        leverage: 100,
        liquidated: true,
        net_pnl: -108_141_232_449_579_150,
        paper_minted: 2_766_363_925_334_821_349,
    };
    assert_eq!(closed, expected);
}

#[test]
fn each_outcome_reads_and_no_other() {
    let id = "0x71a9f6c0143e531d619cb9145175c02f37e030bf2d00695e32a635fa552f14d2";
    let cases = [
        ("succeeded", Ok(Outcome::Succeeded)),
        ("failed", Ok(Outcome::Failed)),
        ("expired", Ok(Outcome::Expired)),
        ("cancelled", Ok(Outcome::Cancelled)),
        ("unknown", Ok(Outcome::Unknown)),
        ("dropped", Err("intent_outcome".to_owned())),
    ];
    for (word, expected) in cases {
        let row = Json::parse(&format!(r#"["{id}","{word}"]"#)).unwrap();
        assert_eq!(read_outcome(&row).map(|(_, outcome)| outcome).map_err(|bad| bad.what), expected, "{word}");
    }
}
