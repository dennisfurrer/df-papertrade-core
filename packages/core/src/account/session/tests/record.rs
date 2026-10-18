//! The stored form: 48 bytes that read back to the same record, nothing else that reads, a
//! secret that never prints, and one that is no key.

use super::*;
use crate::test_support::{NOW_MS, owner, record, session_address};

#[test]
fn a_record_reads_back_from_its_48_bytes() {
    for kept in [record(true), record(false)] {
        let bytes = kept.to_bytes();
        assert_eq!(bytes.len(), 48);
        assert_eq!(SessionRecord::from_bytes(&bytes), Some(kept.clone()), "{kept:?}");
    }
}

#[test]
fn bytes_of_any_other_length_are_no_record() {
    let bytes = record(true).to_bytes();
    for length in [0, 32, 47] {
        assert_eq!(SessionRecord::from_bytes(&bytes[..length]), None, "{length} bytes");
    }
    assert_eq!(SessionRecord::from_bytes(&[bytes.as_slice(), &[0]].concat()), None, "49 bytes");
}

#[test]
fn the_secret_never_prints() {
    let kept = record(true);
    let expected =
        format!("SessionRecord {{ held_until_ms: {}, registered_at_ms: Some({}), .. }}", kept.held_until_ms, NOW_MS);
    assert_eq!(format!("{kept:?}"), expected);
}

#[test]
fn a_kept_key_signs_for_its_wallet_until_it_is_held() {
    let kept = record(true);
    let signer = kept.signer(owner()).unwrap();
    assert_eq!((kept.address().unwrap(), signer.address()), (session_address(), session_address()));
    assert_eq!((signer.wallet(), signer.held_until_ms()), (owner(), kept.held_until_ms));
}

#[test]
fn a_secret_that_is_no_key_has_no_address_and_no_signer() {
    let zero = SessionRecord { secret: Zeroizing::new([0; 32]), ..record(true) };
    assert!(matches!(zero.address(), Err(Error::Signature(_))));
    assert!(matches!(zero.signer(owner()), Err(Error::Signature(_))));
}
