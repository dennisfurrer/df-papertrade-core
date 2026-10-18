//! The stored form: 48 bytes that read back to the same record, and a secret that never prints.

use super::*;
use crate::test_support::record;

#[test]
fn a_record_keeps_its_secret_and_times_in_48_bytes() {
    let kept = record(true);
    let bytes = kept.to_bytes();
    assert_eq!(bytes.len(), 48);
    assert_eq!(SessionRecord::from_bytes(&bytes), Some(kept.clone()));
    assert_eq!(SessionRecord::from_bytes(&record(false).to_bytes()).unwrap().registered_at_ms, None);
    assert_eq!(SessionRecord::from_bytes(&bytes[..47]), None);
    assert!(!format!("{kept:?}").contains("4c0883"), "the secret is never printed");
}
