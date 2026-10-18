//! The stream's 10-field position rows, and a position's notional.

use super::*;

fn row(entry: &str, leverage: &str, intent: &str) -> Json {
    Json::parse(&format!(
        r#"["411276",0,true,false,"{entry}","10000000000000000000",{leverage},"824315",1791862800000,{intent}]"#
    ))
    .unwrap()
}

#[test]
fn a_row_reads_into_its_position() {
    let id = "0xa2e19a7fd16ba48958d0663302f2cf9071c8ba4b75a7686931f905f04f51b2d5";
    let position = Position::read(&row("832245", "100", &format!("\"{id}\""))).unwrap();
    let expected = Position {
        position_id: 411_276,
        instrument_id: 0,
        is_long: true,
        use_debt: false,
        entry_price: 832_245,
        margin: 10_000_000_000_000_000_000,
        leverage: 100,
        bust_price: 824_315,
        opened_at_ms: 1_791_862_800_000,
        intent_id: Some(hash_of(&Json::String(id.to_owned()), "id").unwrap()),
    };
    assert_eq!(position, expected);
    assert_eq!(position.notional(), 1_000_000_000_000_000_000_000, "$10 at 100x");
    assert_eq!(Position::read(&row("832245", "100", "null")).unwrap().intent_id, None, "older than intent ids");
}

#[test]
fn a_row_with_no_entry_or_no_leverage_is_refused() {
    for (name, row) in [("no entry", row("0", "100", "null")), ("no leverage", row("832245", "0", "null"))] {
        assert_eq!(Position::read(&row).map_err(|bad| bad.what), Err("position_tuple".to_owned()), "{name}");
    }
}
