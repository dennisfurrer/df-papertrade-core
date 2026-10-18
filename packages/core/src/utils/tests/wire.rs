//! What a wire error quotes of the value it could not read: all of it, or its first 120
//! characters and `...`, cut between characters however many bytes each takes.

use super::*;

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
}
