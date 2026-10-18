//! What the relayer answers an intent it took: the intent id, action, state and queue revision
//! (none for a cancel), and which states mean it kept the intent.

use super::*;

const ID: &str = "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d";

#[test]
fn an_answer_reads_its_id_action_state_and_revision() {
    let id = df_hyperliquid_core::utils::hex::decode_array(ID).unwrap();
    for (answer, read) in [
        (
            format!(r#"{{"intentId":"{ID}","action":"closePositions","state":"queued","queueRevision":"5646"}}"#),
            Ok(("closePositions", "queued", Some(5_646))),
        ),
        (
            format!(r#"{{"intentId":"{ID}","action":"cancelIntent","state":"cancelled"}}"#),
            Ok(("cancelIntent", "cancelled", None)),
        ),
        (
            format!(r#"{{"intentId":"{ID}","action":"openPosition","state":"queued","queueRevision":-1}}"#),
            Err("queueRevision"),
        ),
        (r#"{"action":"openPosition","state":"queued"}"#.to_owned(), Err("intentId")),
        (format!(r#"{{"intentId":"{}","action":"openPosition","state":"queued"}}"#, &ID[..40]), Err("intentId")),
        (format!(r#"{{"intentId":"{ID}","state":"queued"}}"#), Err("action")),
        (format!(r#"{{"intentId":"{ID}","action":"openPosition","state":1}}"#), Err("state")),
        ("queued".to_owned(), Err("intent_answer")),
    ] {
        let parsed = Accepted::parse(&answer);
        match read {
            Ok((action, state, queue_revision)) => assert_eq!(
                parsed,
                Ok(Accepted { intent_id: id, action: action.to_owned(), state: state.to_owned(), queue_revision }),
                "{answer}"
            ),
            Err(what) => assert_eq!(parsed.map_err(|bad| bad.what), Err(what.to_owned()), "{answer}"),
        }
    }
}

#[test]
fn only_a_queued_prepared_or_confirmed_intent_is_kept() {
    for (state, kept) in [
        ("queued", true),
        ("prepared", true),
        ("confirmed", true),
        ("cancelled", false),
        ("rejected", false),
        ("expired", false),
        ("", false),
    ] {
        let accepted = Accepted {
            intent_id: [0; 32],
            action: "openPosition".to_owned(),
            state: state.to_owned(),
            queue_revision: None,
        };
        assert_eq!(accepted.kept(), kept, "{state}");
    }
}
