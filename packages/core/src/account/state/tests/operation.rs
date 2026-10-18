//! The stream's 10-field operation rows: each action read into its kind, and every row the venue
//! would not send refused, naming the field.

use super::*;

const ID: &str = "0x71a9f6c0143e531d619cb9145175c02f37e030bf2d00695e32a635fa552f14d2";

/// A row for `action` with the middle fields and close ids given as JSON.
fn row(action: &str, middle: &str, ids: &str) -> Json {
    Json::parse(&format!(r#"["{ID}","{action}",{middle},1791862851323,"1791862941",{ids}]"#)).unwrap()
}

const NONE: &str = "null,null,null,null,null";

#[test]
fn each_action_reads_into_its_kind() {
    let amount = |raw: &str| format!(r#"null,null,null,"{raw}",null"#);
    let cases = [
        (
            "openPosition",
            r#"0,false,true,"10000000000000000000",100"#.to_owned(),
            "null",
            OperationKind::OpenPosition {
                instrument_id: 0,
                is_long: false,
                use_debt: true,
                margin: 10_000_000_000_000_000_000,
                leverage: 100,
            },
        ),
        (
            "closePositions",
            NONE.to_owned(),
            r#"["411276","7"]"#,
            OperationKind::ClosePositions { position_ids: vec![411_276, 7] },
        ),
        (
            "withdrawToCore",
            amount("10000000"),
            "null",
            OperationKind::WithdrawToCore { amount: 10_000_000 * COLLATERAL_PER_USDC },
        ),
        ("registerSessionKey", NONE.to_owned(), "null", OperationKind::RegisterSessionKey),
        ("paperStake", amount("5"), "null", OperationKind::PaperStake { amount: 5 }),
        ("paperUnstake", amount("6"), "null", OperationKind::PaperUnstake { amount: 6 }),
        ("paperClaim", NONE.to_owned(), "null", OperationKind::PaperClaim),
        ("paperDistribute", NONE.to_owned(), "null", OperationKind::PaperDistribute),
    ];
    for (action, middle, ids, kind) in cases {
        let operation = Operation::read(&row(action, &middle, ids)).unwrap();
        assert_eq!((operation.kind.action(), &operation.kind), (action, &kind));
        assert_eq!(operation.intent_id, hash_of(&Json::String(ID.to_owned()), "id").unwrap());
        assert_eq!((operation.accepted_at_ms, operation.deadline), (1_791_862_851_323, 1_791_862_941));
    }
}

#[test]
fn a_row_the_venue_would_not_send_is_refused() {
    let thirteen = format!("[{}]", (1..=13).map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(","));
    let nine = Json::parse(&format!(r#"["{ID}","paperClaim",null,null,null,null,null,1,"2"]"#)).unwrap();
    let cases = [
        ("nine fields", nine, "operation_tuple"),
        ("an action it does not know", row("paperBurn", NONE, "null"), "operation_action"),
        ("ids on an open", row("openPosition", r#"0,true,false,"1",1"#, r#"["1"]"#), "operation_close_ids"),
        ("a close with no ids", row("closePositions", NONE, "null"), "operation_close_ids"),
        ("a close of nothing", row("closePositions", NONE, "[]"), "operation_close_ids"),
        ("a close of thirteen", row("closePositions", NONE, &thirteen), "operation_close_ids"),
        ("a close of one twice", row("closePositions", NONE, r#"["1","1"]"#), "operation_close_ids"),
        ("an open with no leverage", row("openPosition", r#"0,true,false,"1",null"#, "null"), "operation_open"),
        ("an open at zero leverage", row("openPosition", r#"0,true,false,"1",0"#, "null"), "operation_open"),
        ("a stake of no amount", row("paperStake", NONE, "null"), "operation_amount"),
        ("a withdrawal of no amount", row("withdrawToCore", NONE, "null"), "operation_amount"),
        ("a side that is no bool", row("openPosition", r#"0,"long",false,"1",1"#, "null"), "operation_open"),
    ];
    for (name, row, what) in cases {
        assert_eq!(Operation::read(&row).map_err(|bad| bad.what), Err(what.to_owned()), "{name}");
    }
}
