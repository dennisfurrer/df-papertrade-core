use super::*;

#[test]
fn the_leaderboard_reads() {
    let text = r#"{"ok":true,"registry":"0xfdc6550436dd75ce8a0b145f935b7d3a64f53248","feeRateRaw":"5000000000000000","asOfMs":1794845015401,"totals":{"builders":2,"users":4,"closes":9,"notionalRaw":"10","notionalDayRaw":"0","accruedRaw":"3"},"builders":[{"address":"0xace5237ffc80df0964f0ebfd077c7ef63031659c","name":"EVPLUS","users":46,"closes":6016,"notionalRaw":"1188489301672299458543110811","notionalDayRaw":"0","accruedRaw":"1558781109847821686848"},{"address":"0x0000000000000000000000000000000000000001","name":null,"users":0,"closes":0,"notionalRaw":"0","notionalDayRaw":"0","accruedRaw":"0"}]}"#;
    let b = Builders::parse(text).unwrap();
    assert_eq!(b.registry, Some(crate::client::deployment::BUILDER_REGISTRY));
    assert_eq!(b.fee_rate, Some(5_000_000_000_000_000));
    assert_eq!(b.builders.len(), 2);
    assert_eq!(b.builders[0].name.as_deref(), Some("EVPLUS"));
    assert_eq!(b.builders[0].accrued, 1_558_781_109_847_821_686_848);
    assert_eq!(b.builders[1].name, None);
    let none = r#"{"ok":true,"registry":null,"feeRateRaw":null,"asOfMs":1,"totals":{},"builders":[]}"#;
    assert_eq!(Builders::parse(none).unwrap().registry, None);
}

#[test]
fn a_builder_reads() {
    let text = r#"{"ok":true,"address":"0xace5237ffc80df0964f0ebfd077c7ef63031659c","approved":true,"unclaimedRaw":"665382673189025","routing":{"users":46,"sessionKeys":96,"closes":6016,"notionalRaw":"1188489301672299458543110811","asOfMs":1794845142984,"daily":[],"hourly":[],"usersDaily":[],"usersFourHourly":[],"usersHourly":[]},"earnings":null}"#;
    let b = Builder::parse(text).unwrap();
    assert!(b.approved);
    assert_eq!((b.unclaimed, b.session_keys, b.closes), (665_382_673_189_025, 96, 6016));
}
