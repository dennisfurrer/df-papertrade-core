//! A refusal in the relayer's own words: its code from `error`, else `code`, and its message,
//! each only when the body is the relayer's JSON and the value is text.

use super::*;

#[test]
fn a_refusal_reads_the_relayers_code_and_message() {
    let expired = r#"{"ok":false,"error":"deadline_expired","message":"This request expired before it was accepted."}"#;
    for (status, body, code, message) in [
        (422, expired, Some("deadline_expired"), Some("This request expired before it was accepted.")),
        (200, r#"{"ok":false,"code":"intake_action_paused"}"#, Some("intake_action_paused"), None),
        (409, r#"{"error":"nonce_used","code":"conflict"}"#, Some("nonce_used"), None),
        (400, r#"{"error":7,"code":"origin_not_allowed"}"#, Some("origin_not_allowed"), None),
        (404, "<html><head><title>404 Not Found</title></head></html>", None, None),
        (502, "", None, None),
        (500, "[]", None, None),
    ] {
        assert_eq!(
            refusal(status, body.as_bytes()),
            RelayerError::Refused { status, code: code.map(str::to_owned), message: message.map(str::to_owned) },
            "{body}"
        );
    }
    assert_eq!(refusal(500, &[0xff, 0xfe]), RelayerError::Refused { status: 500, code: None, message: None });
}

#[test]
fn a_refusal_says_what_the_relayer_said_or_that_it_gave_no_reason() {
    let words = |code: Option<&str>, message: Option<&str>| {
        RelayerError::Refused { status: 422, code: code.map(str::to_owned), message: message.map(str::to_owned) }
            .to_string()
    };
    assert_eq!(
        words(Some("deadline_expired"), Some("expired")),
        "the relayer refused (422, deadline_expired): expired"
    );
    assert_eq!(words(Some("deadline_expired"), None), "the relayer refused (422, deadline_expired): no reason given");
    assert_eq!(words(None, Some("expired")), "the relayer refused (422): expired");
    assert_eq!(words(None, None), "the relayer refused (422): no reason given");
    let other = RelayerError::OtherIntent { sent: "0x01".to_owned(), found: "0x02".to_owned() };
    assert_eq!(other.to_string(), "the relayer accepted a different intent (0x02) than the one sent (0x01)");
}
