//! JWTs, in the one shape this deployment signs and the one it verifies.
//!
//! Compact serialisation, `RS256`, and nothing else. There is no algorithm
//! negotiation here and that is the whole security property: the "alg
//! confusion" family of attacks — `none`, or an RSA public key presented as an
//! HMAC secret — needs a verifier that reads `alg` from the token and does
//! what it says. This one reads `alg` only to refuse anything that is not
//! `RS256`, and picks the key by `kid` out of its own table.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding as _, Signer as _, Verifier as _};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::Value as Json;

use crate::error::{KernelError, Outcome};

/// The only algorithm this deployment signs or verifies.
pub const ALG: &str = "RS256";

/// The token type an id token and a logout token both carry.
pub const TYP_JWT: &str = "JWT";

/// The `typ` a logout token carries, so an RP cannot be talked into accepting
/// one where an id token belongs (Back-Channel Logout §2.4).
pub const TYP_LOGOUT: &str = "logout+jwt";

/// Sign a claim set under a key, as `header.payload.signature`.
pub fn sign(private: &RsaPrivateKey, kid: &str, typ: &str, claims: &Json) -> Outcome<String> {
    let header = serde_json::json!({ "alg": ALG, "typ": typ, "kid": kid });
    let head = B64.encode(serde_json::to_vec(&header).unwrap_or_default());
    let body = B64.encode(
        serde_json::to_vec(claims)
            .map_err(|err| KernelError::Invariant(format!("unserialisable claims: {err}")))?,
    );
    let signing = format!("{head}.{body}");
    let key = SigningKey::<Sha256>::new(private.clone());
    let signature = key.sign(signing.as_bytes());
    Ok(format!("{signing}.{}", B64.encode(signature.to_bytes())))
}

/// A token split into the three things a verifier needs before it has a key.
pub struct Parts {
    /// The decoded header.
    pub header: Json,
    /// The decoded claim set.
    pub claims: Json,
    /// `header.payload`, the bytes that were signed.
    pub signed: String,
    /// The signature.
    pub signature: Vec<u8>,
}

impl Parts {
    /// The `kid` the header names, if it names one.
    #[must_use]
    pub fn kid(&self) -> Option<&str> {
        self.header.get("kid").and_then(Json::as_str)
    }
}

/// Split a compact JWT, refusing anything whose header is not this
/// deployment's one algorithm.
///
/// Reading the claims before the signature is checked is safe *only* because
/// nothing acts on them until [`verify`] has run; the split exists so a
/// verifier can find the `kid` it needs to pick a key.
#[must_use]
pub fn split(token: &str) -> Option<Parts> {
    let mut segments = token.split('.');
    let (Some(head), Some(body), Some(signature), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return None;
    };
    let header: Json = serde_json::from_slice(&B64.decode(head).ok()?).ok()?;
    if header.get("alg").and_then(Json::as_str) != Some(ALG) {
        return None;
    }
    let claims: Json = serde_json::from_slice(&B64.decode(body).ok()?).ok()?;
    Some(Parts {
        header,
        claims,
        signed: format!("{head}.{body}"),
        signature: B64.decode(signature).ok()?,
    })
}

/// Whether these parts were signed by this key.
#[must_use]
pub fn verify(public: &RsaPublicKey, parts: &Parts) -> bool {
    let Ok(signature) = Signature::try_from(parts.signature.as_slice()) else {
        return false;
    };
    VerifyingKey::<Sha256>::new(public.clone())
        .verify(parts.signed.as_bytes(), &signature)
        .is_ok()
}

/// One string claim.
#[must_use]
pub fn claim<'a>(claims: &'a Json, name: &str) -> Option<&'a str> {
    claims.get(name).and_then(Json::as_str)
}

/// One integer claim.
#[must_use]
pub fn claim_int(claims: &Json, name: &str) -> Option<i64> {
    claims.get(name).and_then(Json::as_i64)
}

/// Whether an `aud` claim — a string or an array of them — names this value.
#[must_use]
pub fn audience_contains(claims: &Json, wanted: &str) -> bool {
    match claims.get("aud") {
        Some(Json::String(one)) => one == wanted,
        Some(Json::Array(many)) => many.iter().any(|v| v.as_str() == Some(wanted)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> RsaPrivateKey {
        // Small, because these tests are about the envelope rather than the
        // strength of the key inside it; the deployment mints 2048 bits.
        RsaPrivateKey::new(&mut rand_core::OsRng, 1024).expect("a key")
    }

    #[test]
    fn a_signed_token_verifies_under_its_own_key_and_no_other() {
        let private = key();
        let claims = serde_json::json!({"iss": "https://ronitnath.com", "sub": "abc"});
        let token = sign(&private, "k1", TYP_JWT, &claims).expect("it signs");
        let parts = split(&token).expect("three segments");
        assert_eq!(parts.kid(), Some("k1"));
        assert_eq!(claim(&parts.claims, "sub"), Some("abc"));
        assert!(verify(&RsaPublicKey::from(&private), &parts));
        assert!(!verify(&RsaPublicKey::from(&key()), &parts), "another key");
    }

    #[test]
    fn a_tampered_claim_set_stops_verifying() {
        let private = key();
        let token =
            sign(&private, "k1", TYP_JWT, &serde_json::json!({"sub": "a"})).expect("it signs");
        let mut segments: Vec<&str> = token.split('.').collect();
        let forged = B64.encode(br#"{"sub":"b"}"#);
        segments[1] = &forged;
        let parts = split(&segments.join(".")).expect("still three segments");
        assert_eq!(claim(&parts.claims, "sub"), Some("b"), "it reads as forged");
        assert!(!verify(&RsaPublicKey::from(&private), &parts));
    }

    #[test]
    fn nothing_but_rs256_is_even_split() {
        let head = B64.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let body = B64.encode(br#"{"sub":"a"}"#);
        assert!(split(&format!("{head}.{body}.")).is_none());
        let hs = B64.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        assert!(split(&format!("{hs}.{body}.AAAA")).is_none());
        assert!(split("not.a.jwt").is_none());
        assert!(split("two.parts").is_none());
    }

    #[test]
    fn an_audience_is_a_string_or_a_list_and_never_a_prefix() {
        let one = serde_json::json!({"aud": "c_one"});
        let many = serde_json::json!({"aud": ["c_one", "c_two"]});
        assert!(audience_contains(&one, "c_one"));
        assert!(!audience_contains(&one, "c_on"));
        assert!(audience_contains(&many, "c_two"));
        assert!(!audience_contains(&serde_json::json!({}), "c_one"));
    }
}
