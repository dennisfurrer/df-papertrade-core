//! Float-free readers for the relayer's JSON. Amounts come as decimal strings and small counts
//! as JSON numbers. Each reader accepts either and names the field it failed on.

use df_hyperliquid_core::utils::{Address, Dec, Json, hex};

/// A relayer answer that failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{what}: {found}")]
pub struct BadWire {
    /// The field that failed.
    pub what: String,
    /// What was found instead, truncated.
    pub found: String,
}

pub(crate) fn bad(what: &str, found: &Json) -> BadWire {
    let mut text = found.compact();
    if let Some((cut, _)) = text.char_indices().nth(120) {
        text.truncate(cut);
        text.push_str("...");
    }
    BadWire { what: what.to_owned(), found: text }
}

pub(crate) fn parse(text: &str, what: &str) -> Result<Json, BadWire> {
    Json::parse(text).map_err(|error| BadWire { what: what.to_owned(), found: error.to_string() })
}

pub(crate) fn field<'a>(json: &'a Json, key: &str) -> Result<&'a Json, BadWire> {
    json.get(key).ok_or_else(|| BadWire { what: key.to_owned(), found: "missing".to_owned() })
}

pub(crate) fn digits<'a>(json: &'a Json, what: &str) -> Result<&'a str, BadWire> {
    match json {
        Json::String(text) | Json::Number(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
            Ok(text)
        }
        _ => Err(bad(what, json)),
    }
}

pub(crate) fn u128_of(json: &Json, what: &str) -> Result<u128, BadWire> {
    digits(json, what)?.parse().map_err(|_| bad(what, json))
}

pub(crate) fn u64_of(json: &Json, what: &str) -> Result<u64, BadWire> {
    digits(json, what)?.parse().map_err(|_| bad(what, json))
}

pub(crate) fn u32_of(json: &Json, what: &str) -> Result<u32, BadWire> {
    digits(json, what)?.parse().map_err(|_| bad(what, json))
}

pub(crate) fn i128_of(json: &Json, what: &str) -> Result<i128, BadWire> {
    let text = match json {
        Json::String(text) | Json::Number(text) => text.as_str(),
        _ => return Err(bad(what, json)),
    };
    let body = text.strip_prefix('-').unwrap_or(text);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad(what, json));
    }
    text.parse().map_err(|_| bad(what, json))
}

pub(crate) fn bool_of(json: &Json, what: &str) -> Result<bool, BadWire> {
    match json {
        Json::Bool(flag) => Ok(*flag),
        _ => Err(bad(what, json)),
    }
}

pub(crate) fn str_of<'a>(json: &'a Json, what: &str) -> Result<&'a str, BadWire> {
    json.as_str().ok_or_else(|| bad(what, json))
}

pub(crate) fn address_of(json: &Json, what: &str) -> Result<Address, BadWire> {
    str_of(json, what)?.parse().map_err(|_| bad(what, json))
}

pub(crate) fn hash_of(json: &Json, what: &str) -> Result<[u8; 32], BadWire> {
    hex::decode_array(str_of(json, what)?).map_err(|_| bad(what, json))
}

/// A JSON float (a dollar figure or price), read exactly as written.
pub(crate) fn dec_of(json: &Json, what: &str) -> Result<Dec, BadWire> {
    match json {
        Json::String(text) | Json::Number(text) => Dec::parse(text).map_err(|_| bad(what, json)),
        _ => Err(bad(what, json)),
    }
}

pub(crate) fn nullable<T>(
    json: &Json,
    what: &str,
    read: impl Fn(&Json, &str) -> Result<T, BadWire>,
) -> Result<Option<T>, BadWire> {
    match json {
        Json::Null => Ok(None),
        other => read(other, what).map(Some),
    }
}

pub(crate) fn array_of<'a>(json: &'a Json, what: &str) -> Result<&'a [Json], BadWire> {
    json.as_array().ok_or_else(|| bad(what, json))
}

/// A fixed-length tuple, as the relayer packs rows.
pub(crate) fn tuple<'a>(json: &'a Json, len: usize, what: &str) -> Result<&'a [Json], BadWire> {
    let items = array_of(json, what)?;
    if items.len() != len {
        return Err(bad(what, json));
    }
    Ok(items)
}

#[cfg(test)]
#[path = "tests/wire.rs"]
mod tests;
