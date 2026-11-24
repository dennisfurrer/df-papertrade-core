//! An intent with its signature and signer: the intent id the relayer and the account stream
//! report, and the payload, path and body it is posted with.

use df_hyperliquid_core::account::Signature;
use df_hyperliquid_core::utils::sign::eip712::word_decimal;
use df_hyperliquid_core::utils::sign::{Field, keccak256};
use df_hyperliquid_core::utils::{Address, Json, hex};

use super::{HYPERCORE_ROUTE, Intent};

/// The keccak256 of the intent id tag, which starts every intent id preimage.
#[must_use]
pub fn intent_id_tag() -> [u8; 32] {
    keccak256(b"papertrade:intent-id:v2")
}

/// The intent id of `digest` signed by `signer`: `keccak256(tag || digest || signer)`, with
/// the tag from [`intent_id_tag`].
#[must_use]
pub fn intent_id(digest: &[u8; 32], signer: Address) -> [u8; 32] {
    keccak256(&[&intent_id_tag()[..], &digest[..], &signer.bytes()[..]].concat())
}

impl Intent {
    /// Attaches a signature made elsewhere, trusting that `signer` made it. [`Signed::recover`]
    /// verifies the signer instead.
    #[must_use]
    pub fn signed(self, signature: Signature, signer: Address) -> Signed {
        Signed { intent: self, signature, signer }
    }
}

/// An intent with its signature and signer, ready for the relayer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    intent: Intent,
    signature: Signature,
    signer: Address,
}

/// Why a signature was not taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadSignature {
    #[error(transparent)]
    Curve(#[from] df_hyperliquid_core::account::SignError),
    #[error("signed by {recovered}, not {expected}")]
    WrongSigner { expected: Address, recovered: Address },
}

impl Signed {
    /// Recovers the signer and refuses any signer other than `expected`.
    pub fn recover(intent: Intent, signature: Signature, expected: Address) -> Result<Signed, BadSignature> {
        let recovered = df_hyperliquid_core::account::recover(&intent.digest(), &signature)?;
        if recovered != expected {
            return Err(BadSignature::WrongSigner { expected, recovered });
        }
        Ok(Signed { intent, signature, signer: recovered })
    }

    #[must_use]
    pub const fn intent(&self) -> &Intent {
        &self.intent
    }

    #[must_use]
    pub const fn signature(&self) -> &Signature {
        &self.signature
    }

    #[must_use]
    pub const fn signer(&self) -> Address {
        self.signer
    }

    /// The intent id the relayer and the account stream report.
    #[must_use]
    pub fn intent_id(&self) -> [u8; 32] {
        intent_id(&self.intent.digest(), self.signer)
    }

    /// The `signed` JSON object: the struct's fields (numbers as decimal strings, addresses
    /// and hashes lowercase), then `v` ("27" or "28"), `r` and `s`.
    #[must_use]
    pub fn payload(&self) -> Json {
        let typed = self.intent.typed();
        let mut fields: Vec<(String, Json)> =
            typed.fields().iter().map(|(name, value)| ((*name).to_owned(), payload_value(value))).collect();
        fields.push(("v".to_owned(), Json::String(self.signature.v().to_string())));
        fields.push(("r".to_owned(), Json::String(self.signature.r_hex())));
        fields.push(("s".to_owned(), Json::String(self.signature.s_hex())));
        Json::Object(fields)
    }

    /// Where it is posted: `/intents`, or `/intents/<id>/cancel` for a cancel.
    #[must_use]
    pub fn path(&self) -> String {
        match &self.intent {
            Intent::CancelIntent(cancel) => format!("/intents/{}/cancel", hex::encode_prefixed(&cancel.intent_id)),
            _ => "/intents".to_owned(),
        }
    }

    /// The JSON body posted to [`Signed::path`]. `{action, signed}` plus the HyperCore route
    /// for a withdrawal, or for a cancel the payload without `intentId`.
    #[must_use]
    pub fn body(&self) -> String {
        let payload = self.payload();
        let Some(action) = self.intent.action() else {
            let Json::Object(fields) = payload else {
                return payload.compact();
            };
            return Json::Object(fields.into_iter().filter(|(name, _)| name != "intentId").collect()).compact();
        };
        let mut body = vec![("action".to_owned(), Json::String(action.to_owned())), ("signed".to_owned(), payload)];
        if matches!(self.intent, Intent::WithdrawToCore(_)) {
            body.push((
                "withdrawal".to_owned(),
                Json::Object(vec![("route".to_owned(), Json::String(HYPERCORE_ROUTE.to_owned()))]),
            ));
        }
        Json::Object(body).compact()
    }
}

fn payload_value(value: &Field<'_>) -> Json {
    match value {
        Field::String(text) => Json::String((*text).to_owned()),
        Field::Address(address) => Json::String(address.to_hex()),
        Field::Bytes32(bytes) => Json::String(hex::encode_prefixed(bytes)),
        Field::Bool(flag) => Json::Bool(*flag),
        Field::Uint32(number) => Json::String(number.to_string()),
        Field::Uint64(number) => Json::String(number.to_string()),
        Field::Uint128(number) => Json::String(number.to_string()),
        Field::Uint256(word) => Json::String(word_decimal(word)),
        Field::Uint256Array(words) => Json::Array(words.iter().map(|word| Json::String(word_decimal(word))).collect()),
        Field::Bytes(bytes) => Json::String(hex::encode_prefixed(bytes)),
        // A field type added after this was written: as a wallet carries it.
        other => other.to_json(),
    }
}

#[cfg(test)]
#[path = "tests/signed.rs"]
mod tests;
